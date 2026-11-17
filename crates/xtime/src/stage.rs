//! Moving an RO on from Inspection or Repair, as a technician, exactly as Xtime's RO page does it (its
//! `RepairOrder_Details_Actions.js`: `calculateTechnicianPermissions`, `updateROModeWithQCCheck`, `updateROMode`).
//!
//!   which moves Xtime offers   [`Check::moves`], from the RO as Xtime's page has it (`var RO`), the store's settings
//!                              (`/JS/StoreData`), its part locations and the inspection form:
//!     Inspection (yours, every active Required item filled in)
//!       all lines decided, all work done   → Send to Review (Complete Inspection / All Work Complete)
//!       all lines decided, work to do      → Work on Primary Lines (Complete Inspection): to Repair
//!       lines still to decide              → Send to Approval (Complete Inspection), or "(Parts Quote Needed)"
//!     Repair (yours)
//!       on hold, and the store makes techs press Start Working → Start Working
//!       all lines decided and done         → Send to Review
//!       lines still to decide              → Send back to Approval
//!   making the move            [`Xtime::move_ro`]: the store's "Repair Order Quality" rating of the advisor first
//!                              (when the store asks for one and it isn't given yet: [`Moved::NeedsRating`]); for
//!                              Approval with parts needed, the parts estimate emailed to parts (when the store does)
//!                              and the parts status set to Waiting; then the stage

use serde_json::{Value, json};
use workbench_net::Method;

use crate::employee::encode;
use crate::inspect::{InspectionForm, script_src, var_value};
use crate::json::{get, text};
use crate::{Error, Result, Xtime};

// Xtime's lookups (ASR.LookUp)
const DISPATCH: i64 = 2;
const INSPECTION: i64 = 3;
const APPROVAL: i64 = 4;
const REPAIR: i64 = 6;
const REVIEW: i64 = 7;
/// EmployeeType: advisors (whose "quality" a tech rates), techs, parts
const ADV: i64 = 8;
const TECH: i64 = 9;
const PRTS: i64 = 13;
/// PartsStatus.Waiting
const PARTS_WAITING: i64 = 2;
/// YellowInspectionItemSettings.InformationOnly
const YELLOW_INFO_ONLY: i64 = 2;
/// ClosedReason a technician's moves carry (Xtime's default)
const CLOSED_REASON_TECH: i64 = 1;

/// A move a technician can make on an RO
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// Inspection done, lines to decide: to Approval. `parts_quote`: Xtime's "(Parts Quote Needed)" one
    CompleteToApproval { parts_quote: bool },
    /// Inspection done, every line decided, work to do: to Repair
    CompleteToRepair,
    /// Inspection done, every line decided and all the work done: to Review
    CompleteToReview,
    /// In Repair with lines still to decide: back to Approval
    BackToApproval,
    /// In Repair, every line decided and done: to Review
    ToReview,
    /// In Repair on hold (the store makes techs say so): Start Working
    StartWorking,
}

impl Move {
    /// What Xtime's button says
    pub fn label(self, parts_enabled: bool) -> &'static str {
        match self {
            Move::CompleteToApproval { parts_quote: false } => "Send to Approval (Complete Inspection)",
            Move::CompleteToApproval { parts_quote: true } if parts_enabled => "Send to Approval (Parts Quote Needed)",
            Move::CompleteToApproval { parts_quote: true } => "Send to Approval",
            Move::CompleteToRepair => "Work on Primary Lines (Complete Inspection)",
            Move::CompleteToReview => "Send to Review (Complete Inspection/All Work Complete)",
            Move::BackToApproval => "Send back to Approval",
            Move::ToReview => "Send to Review",
            Move::StartWorking => "Start Working",
        }
    }

    /// The stage it moves the RO to (none: Start Working)
    pub fn target(self) -> Option<i64> {
        match self {
            Move::CompleteToApproval { .. } | Move::BackToApproval => Some(APPROVAL),
            Move::CompleteToRepair => Some(REPAIR),
            Move::CompleteToReview | Move::ToReview => Some(REVIEW),
            Move::StartWorking => None,
        }
    }

    /// Whether Xtime also sends the RO to parts with it (when the RO needs parts)
    fn also_send_to_parts(self) -> bool {
        matches!(self, Move::CompleteToApproval { .. } | Move::BackToApproval)
    }
}

/// What Xtime's page works out about an RO, and the moves it offers you
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Check {
    pub mode: i64,
    /// You're its technician
    pub yours: bool,
    /// Active Required inspection items still empty (Inspection only)
    pub required_missing: usize,
    pub all_decided: bool,
    pub all_completed: bool,
    pub has_been_quoted: bool,
    pub send_to_parts: bool,
    pub parts_enabled: bool,
    pub moves: Vec<Move>,
}

/// A rating Xtime asks for before the move (the store's "Repair Order Quality" of the advisor)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RatingAsk {
    pub title: String,
    pub target_user: Value,
    /// For 3 stars or fewer: a reason picked from these (Xtime's "Other": typed)…
    pub options: Vec<String>,
    pub drop_down: bool,
    /// …or typed
    pub text_field: bool,
}

/// A rating given
#[derive(Debug, Clone, PartialEq)]
pub struct Rating {
    pub target_user: Value,
    /// 1–5
    pub stars: u8,
    pub reason: String,
}

/// How a move went
#[derive(Debug, Clone, PartialEq)]
pub enum Moved {
    Done,
    /// The store wants the advisor rated first (asked again with the rating)
    NeedsRating(RatingAsk),
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn num(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        _ => 0.0,
    }
}

fn path<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().try_fold(v, |v, k| get(v, k))
}

fn list(v: Option<&Value>) -> &[Value] {
    v.and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

/// "/Date(1700000000000)/" or "2023-05-01T00:00:00" → milliseconds (the clock time read as UTC, for comparing)
fn when_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("/Date(") {
        let n: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
        return n.parse().ok();
    }
    let n = |a: usize, b: usize| s.get(a..b).and_then(|x| x.parse::<i64>().ok());
    let (y, m, d) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
    let (h, mi, sec) = (n(11, 13).unwrap_or(0), n(14, 16).unwrap_or(0), n(17, 19).unwrap_or(0));
    // days from 1970-01-01 (Howard Hinnant's days_from_civil)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(((days * 24 + h) * 60 + mi) * 60_000 + sec * 1000)
}

/// The store's "common ops" catalog, for ROs opened since it was turned on (isCommonCatalogEnabled)
fn common_catalog(ro: &Value, store: &Value) -> bool {
    let Some(on) = path(store, &["IntegrationXtime", "CommonOpsEnableDate"]).map(|v| text(Some(v))) else {
        return false;
    };
    match (when_ms(&text(get(ro, "CreateDate"))), when_ms(&on)) {
        (Some(created), Some(since)) => created >= since,
        _ => false,
    }
}

/// An information-only Yellow line (shown to the customer, never decided: isVerdictExemptYellowLine)
fn verdict_exempt(line: &Value, store: &Value) -> bool {
    !truthy(get(line, "IsPrimary"))
        && num(path(store, &["Permissions", "ActiveYellowInspectionItemSetting"])) as i64 == YELLOW_INFO_ONLY
        && text(get(line, "Color")).trim().eq_ignore_ascii_case("yellow")
}

/// Part locations' "In Stock" ID (from the store's PartLocations)
fn in_stock_id(locations: &[Value]) -> Option<f64> {
    locations
        .iter()
        .find(|l| text(get(l, "Location")).trim().eq_ignore_ascii_case("in stock"))
        .map(|l| num(get(l, "ID")))
}

/// Xtime's calculateROData and calculateTechnicianPermissions, for the RO's technician
pub fn check(ro: &Value, store: &Value, locations: &[Value], form: Option<&InspectionForm>, tech_id: &str) -> Check {
    let lines = list(get(ro, "RepairOrderLines"));
    let mode = num(get(ro, "CurrentMode")) as i64;
    let tech = text(get(ro, "TechnicianID"));
    let yours = num(get(ro, "TechnicianID")) > 0.0 && !tech_id.trim().is_empty() && tech.trim() == tech_id.trim();
    let flag = |l: &Value, k: &str| truthy(get(l, k));
    let parts_enabled = truthy(path(store, &["Parts", "EnableParts"]));
    let in_stock = in_stock_id(locations);

    // each line: quoted (decided, or every part priced and counted), parts not in stock
    let line_quoted = |l: &Value| {
        let parts = list(get(l, "Parts"));
        let priced = parts
            .iter()
            .filter(|p| num(get(p, "Price")) > 0.0 && num(get(p, "Quantity")) > 0.0)
            .count();
        flag(l, "Approved") || flag(l, "Declined") || (priced > 0 && priced == parts.len())
    };
    let not_in_stock = |l: &Value| list(get(l, "Parts")).iter().any(|p| Some(num(get(p, "LocationID"))) != in_stock);

    let all_completed = !lines
        .iter()
        .any(|l| (flag(l, "IsPrimary") || flag(l, "Approved")) && !flag(l, "Completed"));
    let all_decided = !lines
        .iter()
        .any(|l| !flag(l, "IsPrimary") && !flag(l, "Approved") && !flag(l, "Declined") && !verdict_exempt(l, store));
    let has_been_quoted = lines
        .iter()
        .all(|l| flag(l, "IsPrimary") || flag(l, "PartsNotNeeded") || line_quoted(l));
    let parts_not_needed = lines.iter().all(|l| flag(l, "IsPrimary") || flag(l, "PartsNotNeeded"));
    let common_lines = common_catalog(ro, store) && lines.iter().any(|l| num(get(l, "SGID")) as i64 == 7);
    let lines_need_parts = !has_been_quoted || lines.iter().any(not_in_stock) || common_lines;
    let send_to_parts = !((parts_not_needed && !truthy(path(store, &["Parts", "EnableNoPartsNotification"])))
        || (!truthy(path(store, &["IntegrationSIS", "EnablePartInfoForcePart"])) && !lines_need_parts));

    // the inspection's active Required items, all filled in (InspectionForm.AllRequiredItemsCompleted)
    let required_missing = form
        .map(|f| {
            f.categories
                .iter()
                .flat_map(|c| &c.items)
                .filter(|i| i.required && i.value.is_empty())
                .count()
        })
        .unwrap_or(usize::MAX);

    let mut moves = Vec::new();
    match mode {
        INSPECTION if yours && required_missing == 0 => {
            if all_decided {
                moves.push(if all_completed {
                    Move::CompleteToReview
                } else {
                    Move::CompleteToRepair
                });
            } else {
                // (#3483) the plain one when it's been quoted and doesn't need to go to parts
                if has_been_quoted && (!parts_enabled || !send_to_parts) {
                    moves.push(Move::CompleteToApproval { parts_quote: false });
                }
                if !has_been_quoted || (parts_enabled && send_to_parts) {
                    moves.push(Move::CompleteToApproval { parts_quote: true });
                }
            }
        }
        REPAIR if yours => {
            if truthy(path(store, &["Permissions", "EnableForceTechToStartWorking"])) && flag(ro, "IsOnHold") {
                moves.push(Move::StartWorking);
            } else {
                if all_decided && all_completed {
                    moves.push(Move::ToReview);
                }
                if !all_decided {
                    moves.push(Move::BackToApproval);
                }
            }
        }
        _ => {}
    }
    Check {
        mode,
        yours,
        required_missing: if form.is_some() { required_missing } else { 0 },
        all_decided,
        all_completed,
        has_been_quoted,
        send_to_parts,
        parts_enabled,
        moves,
    }
}

/// What a move needs from Xtime's page: the RO, the store's settings, its part locations, you
struct Page {
    ro: Value,
    store: Value,
    locations: Vec<Value>,
    user: Option<Value>,
}

impl Xtime {
    async fn stage_page(&self, ro: &str) -> Result<Page> {
        let d = self.details(ro).await?;
        let ro_data = var_value(&d.html, "RO").ok_or_else(|| Error::Format("Xtime's RO page didn't have the RO".into()))?;
        let store = self
            .script_var(d.store.as_deref(), "Store", false)
            .await
            .ok_or_else(|| Error::Format("Couldn't read the store's settings from Xtime".into()))?;
        let locations = self
            .script_var(script_src(&d.html, "PartLocations").as_deref(), "PartLocations", false)
            .await
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        let user = self.script_var(script_src(&d.html, "UserData").as_deref(), "User", false).await;
        Ok(Page {
            ro: ro_data,
            store,
            locations,
            user,
        })
    }

    /// The moves Xtime offers you on an RO now (Inspection and Repair; nothing otherwise)
    pub async fn stage_check(&self, store: &str, ro: &str, tech_id: &str) -> Result<Check> {
        let page = self.stage_page(ro).await?;
        let mode = num(get(&page.ro, "CurrentMode")) as i64;
        let form = if mode == INSPECTION {
            Some(self.inspection(store, ro).await?)
        } else {
            None
        };
        Ok(check(&page.ro, &page.store, &page.locations, form.as_ref(), tech_id))
    }

    /// Makes a move (checked again first: Xtime has to still offer it). `rating`: the one Xtime asked for.
    pub async fn move_ro(&self, store: &str, ro: &str, tech_id: &str, mv: Move, rating: Option<Rating>) -> Result<Moved> {
        let page = self.stage_page(ro).await?;
        let mode = num(get(&page.ro, "CurrentMode")) as i64;
        let form = if mode == INSPECTION {
            Some(self.inspection(store, ro).await?)
        } else {
            None
        };
        let c = check(&page.ro, &page.store, &page.locations, form.as_ref(), tech_id);
        if !c.moves.contains(&mv) {
            return Err(Error::Format(if mode == DISPATCH {
                "the RO is still in Dispatch".into()
            } else {
                "Xtime doesn't offer that for this RO any more: it's changed since it was read".into()
            }));
        }
        let api = Self::api(store, ro);
        let Some(target) = mv.target() else {
            // Start Working: the RO's on hold no more
            self.ro_call(
                Method::PUT,
                &format!("{api}/Info?format=json"),
                Some(json!({ "Data": { "IsOnHold": false } })),
            )
            .await?;
            return Ok(Moved::Done);
        };

        // the store's "Repair Order Quality": a tech rates the advisor before sending the RO on
        let qc = list(get(&page.store, "QualityControls"))
            .iter()
            .find(|q| num(get(q, "TypeId")) as i64 == ADV)
            .cloned();
        if let Some(qc) = qc.filter(|_| target >= APPROVAL) {
            let store_id = path(&page.store, &["StoreID"])
                .map(|v| text(Some(v)))
                .unwrap_or_else(|| store.to_string());
            let me = page
                .user
                .as_ref()
                .and_then(|u| get(u, "EmployeeID"))
                .cloned()
                .unwrap_or_else(|| tech_id.trim().parse::<i64>().map(Value::from).unwrap_or(Value::from(tech_id)));
            let advisor = get(&page.ro, "AdvisorID").cloned().unwrap_or(Value::Null);
            let base = format!("/api/Store/{}/RepairOrders/{}/QualityControl", encode(&store_id), encode(ro));
            match rating {
                Some(r) => {
                    self.ro_call(
                        Method::POST,
                        &format!("{base}/Rating"),
                        Some(json!({ "CreateUser": me, "TargetUser": r.target_user, "StarRating": r.stars, "Reason": r.reason })),
                    )
                    .await?;
                }
                None if num(Some(&advisor)) > 0.0 => {
                    let options = self
                        .ro_call(
                            Method::GET,
                            &format!(
                                "{base}/Type/{ADV}/Options?CreateUser={}&TargetUser={}",
                                encode(&text(Some(&me))),
                                encode(&text(Some(&advisor)))
                            ),
                            None,
                        )
                        .await?;
                    if get(&options, "IsRated") == Some(&Value::Bool(false)) {
                        let mut opts: Vec<&Value> = list(get(&options, "Options")).iter().collect();
                        opts.sort_by(|a, b| num(get(a, "DisplayOrder")).total_cmp(&num(get(b, "DisplayOrder"))));
                        return Ok(Moved::NeedsRating(RatingAsk {
                            title: "Repair Order Quality".into(),
                            target_user: advisor,
                            options: opts.iter().map(|o| text(get(o, "Name"))).filter(|n| !n.is_empty()).collect(),
                            drop_down: truthy(get(&qc, "EnableDropDownOptions")),
                            text_field: truthy(get(&qc, "EnableTextField")),
                        }));
                    }
                }
                None => {} // no advisor on the RO: nobody to rate
            }
        }

        // to parts with it, when the RO needs parts: the parts estimate emailed (when the store does), parts status
        // Waiting
        if mv.also_send_to_parts() && c.send_to_parts && c.parts_enabled {
            if truthy(path(&page.store, &["Notifications", "Notification02_EnableEmail"])) {
                let pdf = format!(
                    "/PDF/PartsEstimate/{}?&language=EN&lineType=All&splitRedYellow=undefined&splitByLineType=undefined\
                     &sendEmail=true&sendSMS=undefined&ccEmailAddress=&customerEmail=&customerCell=undefined",
                    encode(ro)
                );
                // (Xtime opens this in a window and doesn't wait on it: the move goes on either way)
                if let Ok(req) = self.request(Method::GET, &pdf) {
                    let _ = self.send(req).await;
                }
            }
            let common = common_catalog(&page.ro, &page.store)
                && list(get(&page.ro, "RepairOrderLines"))
                    .iter()
                    .any(|l| num(get(l, "SGID")) as i64 == 7);
            let body = if common {
                json!({ "PartStatusID": PARTS_WAITING, "IsCommonCatalog": true })
            } else {
                json!({ "PartStatusID": PARTS_WAITING })
            };
            self.ro_call(Method::PUT, &format!("{api}/PartStatus?format=json"), Some(body))
                .await?;
        }

        self.ro_call(
            Method::PUT,
            &format!("{api}/ROMode?format=json"),
            Some(json!({ "ROModeID": target, "ClosedReason": CLOSED_REASON_TECH })),
        )
        .await?;
        Ok(Moved::Done)
    }
}

// (the other employee types Xtime rates; kept for reference)
const _: [i64; 2] = [TECH, PRTS];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::{Category, Item};

    fn form(missing: bool) -> InspectionForm {
        InspectionForm {
            categories: vec![Category {
                items: vec![Item {
                    required: true,
                    value: if missing { String::new() } else { "8".into() },
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn ro(mode: i64, lines: Value) -> Value {
        json!({ "CurrentMode": mode, "TechnicianID": 77, "RepairOrderLines": lines, "CreateDate": "/Date(1700000000000)/" })
    }

    fn store(parts: bool) -> Value {
        json!({ "Parts": { "EnableParts": parts }, "IntegrationSIS": {}, "Permissions": {} })
    }

    fn locations() -> Vec<Value> {
        vec![
            json!({ "ID": 1, "Location": "In Stock" }),
            json!({ "ID": 2, "Location": "Out of Stock" }),
        ]
    }

    #[test]
    fn inspection_needs_every_required_item() {
        let lines = json!([{ "IsPrimary": true, "Completed": false, "Parts": [] },
                           { "IsPrimary": false, "Parts": [{ "Price": 0, "Quantity": 1, "LocationID": 1 }] }]);
        let c = check(&ro(3, lines.clone()), &store(true), &locations(), Some(&form(true)), "77");
        assert!(c.moves.is_empty() && c.required_missing == 1);
        let c = check(&ro(3, lines.clone()), &store(true), &locations(), Some(&form(false)), "77");
        // not decided, not quoted (no price): the "Parts Quote Needed" one
        assert_eq!(c.moves, [Move::CompleteToApproval { parts_quote: true }]);
        // someone else's RO: nothing
        assert!(
            check(&ro(3, lines), &store(true), &locations(), Some(&form(false)), "5")
                .moves
                .is_empty()
        );
    }

    #[test]
    fn quoted_and_in_stock_goes_straight_to_approval() {
        let lines = json!([{ "IsPrimary": false, "Parts": [{ "Price": 12.5, "Quantity": 1, "LocationID": 1 }] }]);
        let c = check(&ro(3, lines), &store(true), &locations(), Some(&form(false)), "77");
        assert!(c.has_been_quoted && !c.send_to_parts);
        assert_eq!(c.moves, [Move::CompleteToApproval { parts_quote: false }]);
    }

    #[test]
    fn all_decided_goes_to_repair_or_review() {
        let todo = json!([{ "IsPrimary": true, "Completed": false, "Parts": [] },
                          { "Approved": true, "Completed": false, "Parts": [] }]);
        let c = check(&ro(3, todo), &store(false), &locations(), Some(&form(false)), "77");
        assert_eq!(c.moves, [Move::CompleteToRepair]);
        let done = json!([{ "IsPrimary": true, "Completed": true, "Parts": [] },
                          { "Declined": true, "Parts": [] }]);
        let c = check(&ro(3, done), &store(false), &locations(), Some(&form(false)), "77");
        assert_eq!(c.moves, [Move::CompleteToReview]);
    }

    #[test]
    fn repair_to_review_when_all_done() {
        let lines = |done: bool| {
            json!([{ "IsPrimary": true, "Completed": done, "Parts": [] },
                                        { "Approved": true, "Completed": true, "Parts": [] }])
        };
        assert!(check(&ro(6, lines(false)), &store(true), &locations(), None, "77").moves.is_empty());
        assert_eq!(
            check(&ro(6, lines(true)), &store(true), &locations(), None, "77").moves,
            [Move::ToReview]
        );
        let undecided = json!([{ "IsPrimary": false, "Parts": [] }]);
        assert_eq!(
            check(&ro(6, undecided), &store(true), &locations(), None, "77").moves,
            [Move::BackToApproval]
        );
        // on hold, and the store makes techs press Start Working
        let mut held = ro(6, lines(true));
        held["IsOnHold"] = json!(true);
        let s = json!({ "Parts": {}, "Permissions": { "EnableForceTechToStartWorking": true } });
        assert_eq!(check(&held, &s, &locations(), None, "77").moves, [Move::StartWorking]);
    }

    #[test]
    fn info_only_yellow_lines_need_no_verdict() {
        let lines = json!([{ "IsPrimary": true, "Completed": true, "Parts": [] },
                           { "IsPrimary": false, "Color": "Yellow", "Parts": [] }]);
        let s = json!({ "Parts": {}, "Permissions": { "ActiveYellowInspectionItemSetting": 2 } });
        assert_eq!(check(&ro(6, lines), &s, &locations(), None, "77").moves, [Move::ToReview]);
    }

    #[test]
    fn dates() {
        assert_eq!(when_ms("/Date(1700000000000)/"), Some(1_700_000_000_000));
        assert_eq!(when_ms("1970-01-02T00:00:00"), Some(86_400_000));
        assert!(when_ms("2023-11-14T22:13:20").unwrap() == 1_700_000_000_000);
    }
}
