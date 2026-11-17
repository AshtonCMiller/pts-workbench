//! One RO, as Xtime's own RO page reads it (the Electron app's `ro.js`, `{ type: 'view' }`): the header, vehicle,
//! people, flags and every line with its notes and parts; and marking a line done, like Xtime's Done button.
//! The customer (name, phone, email, address) is never read out of what Xtime sends.

use serde::Serialize;
use serde_json::{Value, json};
use workbench_net::Method;

use crate::employee::encode;
use crate::json::{get, text};
use crate::{Error, Result, Xtime};

#[derive(Debug, Clone, Default, Serialize)]
pub struct RoView {
    pub ro: String,
    pub tag: String,
    /// Xtime's stage number ([`mode_name`])
    pub mode: i64,
    /// When it got to this stage (Xtime's time, the store's local time)
    pub mode_since: String,
    pub waiter: bool,
    pub express: bool,
    pub internal: bool,
    pub warranty: bool,
    pub cpo: bool,
    pub on_hold: bool,
    pub waiting_parts: bool,
    pub waiting_customer: bool,
    pub waiting_warranty: bool,
    /// Xtime's parts status (its P column): 2 waiting at parts, 3 parts is working on it, 4 complete; 0 none
    pub part_status: i64,
    /// "11/15/2026"
    pub promised_date: String,
    /// "17:00"
    pub promised_time: String,
    pub opened: String,
    /// "Jane Doe (123)", or the number alone
    pub parts_employee: String,
    pub advisor: Person,
    pub technician: Person,
    /// The store's own flags
    pub flags: Vec<String>,
    pub vehicle: Vehicle,
    /// In Xtime's order; deleted lines left out
    pub lines: Vec<Line>,
    /// The RO's Dealership chat conversation (its channel id, which Xtime's server makes: "3590-821267-2611"); a
    /// conversation is started by sending its first message there. Empty when Xtime has no chat for it.
    pub chat_channel: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Person {
    pub name: String,
    pub number: String,
}

impl Person {
    /// "Jane Doe (123)"; "" when nobody
    pub fn label(&self) -> String {
        match (self.name.is_empty(), self.number.is_empty()) {
            (true, _) => String::new(),
            (false, true) => self.name.clone(),
            (false, false) => format!("{} ({})", self.name, self.number),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Vehicle {
    pub year: String,
    pub make: String,
    pub model: String,
    /// In capitals
    pub vin: String,
    pub mileage: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Line {
    /// Xtime's line ID (what Done is sent for)
    pub id: Value,
    /// Where it is in Xtime's list of the RO's lines (as Xtime sends them, deleted ones counted)
    pub index: usize,
    pub rolid: Option<Value>,
    pub parent_id: Option<Value>,
    pub group_id: Option<Value>,
    /// The line letter ("A", "B"…)
    pub dms: String,
    /// For editing it (Xtime's Add Service window starts from these): its service, Xtime's name for it, the details
    /// ("Front brakes"), the labor type's ID, the CDK op code, its colour
    pub sgcid: Option<Value>,
    pub sgid: Option<Value>,
    pub service_name: String,
    pub details: String,
    pub pay_type_id: Option<Value>,
    pub op_code_dms: String,
    pub color: String,
    pub name: String,
    pub op_code: String,
    /// A customer concern (no approve / decline: only recommendations have those)
    pub primary: bool,
    pub approved: bool,
    pub declined: bool,
    pub completed: bool,
    /// Labor type ("C", "W", "I", or Xtime's name for it)
    pub pay_type: String,
    pub hours: Option<f64>,
    pub labor: Option<f64>,
    pub parts: Option<f64>,
    pub total: Option<f64>,
    pub concern: String,
    pub cause: String,
    pub correction: String,
    pub tech_note: String,
    pub advisor_note: String,
    pub manager_note: String,
    pub parts_note: String,
    pub parts_not_needed: bool,
    pub parts_picked_up: bool,
    pub dispatch: String,
    pub estimate: Option<f64>,
    pub tech_start: String,
    pub tech_end: String,
    pub part_list: Vec<Part>,
    pub sub_lines: Vec<String>,
    pub images: usize,
    pub techs: Vec<String>,
    pub added_by: String,
    pub order: f64,
}

impl Line {
    /// The ID as text (for keeping which lines are open)
    pub fn key(&self) -> String {
        text(Some(&self.id))
    }

    /// Whether the work can be done (so marked Done): a customer concern, or an approved recommendation; never a
    /// declined one or one still waiting on approval. (One already marked done can always be unmarked.)
    pub fn can_be_done(&self) -> bool {
        self.completed || (!self.declined && (self.primary || self.approved))
    }

    /// Anything to show when it's opened
    pub fn has_detail(&self) -> bool {
        let short = self
            .name
            .split_once(':')
            .map(|(_, r)| r.trim())
            .unwrap_or(&self.name)
            .to_uppercase();
        !self.concern.is_empty()
            || !self.cause.is_empty()
            || (!self.correction.is_empty() && self.correction.to_uppercase() != short)
            || !self.tech_note.is_empty()
            || !self.advisor_note.is_empty()
            || !self.manager_note.is_empty()
            || !self.parts_note.is_empty()
            || !self.part_list.is_empty()
            || !self.sub_lines.is_empty()
    }

    /// Lines Xtime adds to every RO for the inspection (multipoint, green tires / battery / brakes)
    pub fn is_inspection(&self) -> bool {
        let starts = |s: &str| {
            let u = s.trim_start().to_uppercase();
            ["Q99PX", "GTIRE", "GBATT", "GBK"]
                .iter()
                .any(|c| u.starts_with(c) && !u[c.len()..].starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_'))
        };
        starts(&self.name) || starts(&self.op_code)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Part {
    pub number: String,
    pub description: String,
    pub quantity: Option<f64>,
    pub price: Option<f64>,
    pub status: String,
    pub picked_up: bool,
}

/// Xtime's stages (its ROModes list)
pub fn mode_name(mode: i64) -> String {
    match mode {
        9 => "Pre-RO".into(),
        2 => "Dispatch".into(),
        3 => "Being Inspected".into(),
        4 => "Pending Approval".into(),
        6 => "Being Repaired".into(),
        7 => "Review".into(),
        100 => "Closed".into(),
        n => format!("Stage {n}"),
    }
}

/// One of the RO page's lists of lines (where a line can be dragged to)
#[derive(Debug, Clone, PartialEq)]
pub enum Place {
    /// The customer concerns
    Concerns,
    /// Under a customer concern (its line ID)
    Under(Value),
    /// Additional Service Recommendations (not under a concern)
    Recommendations,
}

/// The same line ID (Xtime sends them as numbers; a string of the same number is the same)
pub fn same_id(a: &Value, b: &Value) -> bool {
    a == b || (!a.is_null() && text(Some(a)) == text(Some(b)))
}

/// The RO's lines in the RO page's groups: customer concerns (each with the recommendations added under it), the
/// other recommendations, and the inspection lines. Indexes into [`RoView::lines`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sections {
    pub concerns: Vec<(usize, Vec<usize>)>,
    pub loose: Vec<usize>,
    pub inspection: Vec<usize>,
}

/// A spiff is paid when the approved recommendations come to more than this (dollars)
pub const SPIFF_OVER: f64 = 150.0;

/// Whether approved recommendations coming to `total` earn a spiff (compared in cents: no rounding hair counts)
pub fn spiff_over(total: f64) -> bool {
    (total * 100.0).round() > (SPIFF_OVER * 100.0).round()
}

impl RoView {
    /// The lines that count toward the RO's spiff: recommendations (under a concern or not, not the inspection lines)
    /// that are approved and not declined
    pub fn spiff_lines(&self) -> Vec<&Line> {
        self.lines
            .iter()
            .filter(|l| !l.primary && !l.is_inspection() && l.approved && !l.declined)
            .collect()
    }

    /// What they come to
    pub fn spiff_total(&self) -> f64 {
        self.spiff_lines().iter().filter_map(|l| l.total).sum()
    }

    /// Over [`SPIFF_OVER`]: the spiff can be printed and turned in
    pub fn spiff_earned(&self) -> bool {
        spiff_over(self.spiff_total())
    }

    /// The lines in one of Xtime's lists, in order (as its RO page has them): the customer concerns, the lines under
    /// one concern (its line ID), or the recommendations on their own (inspection lines included: Xtime lists them
    /// there too)
    pub fn xtime_list(&self, place: &Place) -> Vec<&Line> {
        let top = |l: &Line| l.group_id.as_ref().is_none_or(|g| g.is_null());
        self.lines
            .iter()
            .filter(|l| match place {
                Place::Concerns => top(l) && l.primary,
                Place::Recommendations => top(l) && !l.primary,
                Place::Under(c) => l.group_id.as_ref().is_some_and(|g| same_id(g, c)),
            })
            .collect()
    }

    /// Where a line dropped in `place` lands, as Xtime counts it (from 1): before the line `before`, or after the last
    /// of `shown` (the lines of that list the app shows, in order) when there's none. The line moving isn't counted.
    pub fn drop_order(&self, moving: &Value, place: &Place, before: Option<&Value>, shown: &[&Value]) -> usize {
        let list: Vec<&Line> = self.xtime_list(place).into_iter().filter(|l| !same_id(&l.id, moving)).collect();
        let at = |id: &Value| list.iter().position(|l| same_id(&l.id, id));
        let index = match before.and_then(at) {
            Some(i) => i,
            None => shown
                .iter()
                .filter(|id| !same_id(id, moving))
                .filter_map(|id| at(id))
                .max()
                .map_or(0, |i| i + 1),
        };
        index + 1
    }

    pub fn sections(&self) -> Sections {
        let lines = &self.lines;
        let (insp, main): (Vec<usize>, Vec<usize>) = (0..lines.len()).partition(|&i| lines[i].is_inspection());
        let primaries: Vec<usize> = main.iter().copied().filter(|&i| lines[i].primary).collect();
        // Xtime's own grouping, as its RO page has it: a line's GroupID is the concern it's under (set when it's added
        // under one, or dragged there; a line that shares a concern's letter isn't under it unless Xtime says so)
        let parent_of = |r: &Line| -> Option<usize> {
            let g = r.group_id.as_ref().filter(|g| !g.is_null())?;
            primaries.iter().copied().find(|&p| same_id(&lines[p].id, g))
        };
        let mut concerns: Vec<(usize, Vec<usize>)> = primaries.iter().map(|&p| (p, Vec::new())).collect();
        let mut loose = Vec::new();
        for &i in main.iter().filter(|&&i| !lines[i].primary) {
            match parent_of(&lines[i]).and_then(|p| concerns.iter_mut().find(|(c, _)| *c == p)) {
                Some((_, kids)) => kids.push(i),
                None => loose.push(i),
            }
        }
        Sections {
            concerns,
            loose,
            inspection: insp,
        }
    }
}

// ---------- reading Xtime's answer ----------

fn t(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn field(v: &Value, k: &str) -> String {
    t(get(v, k))
}

fn yes(v: &Value, k: &str) -> bool {
    get(v, k) == Some(&Value::Bool(true))
}

/// JavaScript's `!!x`
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// A number, as JavaScript's `Number()` would read it
fn number(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) if !s.trim().is_empty() => s.trim().parse().ok(),
        _ => None,
    }
}

/// Money and hours: a number that isn't negative
fn money(v: Option<&Value>) -> Option<f64> {
    number(v).filter(|n| n.is_finite() && *n >= 0.0)
}

/// A date, or "" for none (Xtime's "0001-01-01…")
fn day(v: Option<&Value>) -> String {
    let s = t(v);
    if s.starts_with("0001-01-01") { String::new() } else { s }
}

/// A person's name from a user object ({ FullName } / { FirstName, LastName } / "name")
fn name_of(u: Option<&Value>) -> String {
    match u {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(u @ Value::Object(_)) => {
            for k in ["FullName", "DisplayName", "Name"] {
                let s = field(u, k);
                if !s.is_empty() {
                    return s;
                }
            }
            let full = [field(u, "FirstName"), field(u, "LastName")]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            if !full.is_empty() {
                return full;
            }
            let user = field(u, "UserName");
            if user.is_empty() { field(u, "Number") } else { user }
        }
        _ => String::new(),
    }
}

fn pay_of(p: Option<&Value>) -> String {
    match p {
        Some(Value::String(s)) => s.clone(),
        Some(p @ Value::Object(_)) => ["Description", "Name", "Code", "PayTypeCode"]
            .iter()
            .map(|k| field(p, k))
            .find(|s| !s.is_empty())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn first_of(v: &Value, keys: &[&str]) -> Option<Value> {
    keys.iter().filter_map(|k| get(v, k)).find(|x| !x.is_null()).cloned()
}

fn part_of(p: &Value) -> Part {
    let words = |keys: &[&str]| keys.iter().map(|k| field(p, k)).find(|s| !s.is_empty()).unwrap_or_default();
    let status_name = |k: &str| match get(p, k) {
        Some(s @ Value::Object(_)) => field(s, "Name"),
        other => t(other),
    };
    let mut status = status_name("Status");
    if status.is_empty() {
        status = status_name("PartStatus");
    }
    if status.is_empty() {
        status = match get(p, "IsInStock") {
            Some(Value::Bool(true)) => "In stock".into(),
            Some(Value::Bool(false)) => "Not in stock".into(),
            _ => String::new(),
        };
    }
    let picked = first_of(p, &["PickedUpDate", "PartsPickedUpDate"]);
    Part {
        number: words(&["PartNumber", "Number", "PartNo"]),
        description: words(&["Description", "PartDescription", "Name"]),
        quantity: number(first_of(p, &["Quantity", "QuantityRequested", "Qty"]).as_ref()).filter(|q| *q != 0.0),
        price: money(first_of(p, &["Price", "PricePerItem", "UnitPrice"]).as_ref()),
        status,
        picked_up: !day(picked.as_ref()).is_empty(),
    }
}

fn line_of(l: &Value) -> Line {
    let list = |k: &str| get(l, k).and_then(Value::as_array).cloned().unwrap_or_default();
    let opt = |k: &str| get(l, k).filter(|v| !v.is_null()).cloned();
    let special = field(l, "ServiceGuideSpecialItemName");
    let service = field(l, "ServiceName");
    let name = if special.is_empty() {
        let lower = service.to_ascii_lowercase();
        match lower.strip_prefix("[primary]:") {
            Some(_) => service["[primary]:".len()..].trim_start().to_string(),
            None => service.clone(),
        }
    } else {
        special
    };
    let op_dms = field(l, "OpCodeDms");
    let mut pay_type = pay_of(get(l, "PayTypeCode"));
    if pay_type.is_empty() {
        pay_type = if truthy(get(l, "IsWarranty")) {
            "W".into()
        } else if truthy(get(l, "IsInternal")) {
            "I".into()
        } else {
            String::new()
        };
    }
    Line {
        id: get(l, "ID").cloned().unwrap_or(Value::Null),
        index: 0, // set by parse_view (where it is in Xtime's list)
        rolid: opt("ROLID"),
        parent_id: opt("ParentLineId"),
        group_id: get(l, "GroupID").filter(|v| truthy(Some(v))).cloned(),
        dms: field(l, "DMSNumber"),
        sgcid: opt("SGCID"),
        sgid: opt("SGID"),
        service_name: service.clone(),
        details: field(l, "ServiceGuideSpecialItemName"),
        pay_type_id: get(l, "PayTypeCode")
            .filter(|p| p.is_object())
            .and_then(|p| get(p, "ID"))
            .filter(|v| !v.is_null())
            .cloned(),
        op_code_dms: op_dms.clone(),
        color: field(l, "Color"),
        name,
        op_code: if op_dms.is_empty() { field(l, "OpCode") } else { op_dms },
        primary: yes(l, "IsPrimary"),
        approved: yes(l, "Approved"),
        declined: yes(l, "Declined"),
        completed: yes(l, "Completed"),
        pay_type,
        hours: money(get(l, "Hours")),
        labor: money(get(l, "PriceLabor")),
        parts: money(get(l, "PriceParts")),
        total: money(get(l, "Price")),
        concern: field(l, "Complaint"),
        cause: field(l, "Cause"),
        correction: field(l, "Correction"),
        tech_note: field(l, "TechnicianNote"),
        advisor_note: field(l, "AdvisorNote"),
        manager_note: field(l, "ManagerNote"),
        parts_note: field(l, "PartsNote"),
        parts_not_needed: yes(l, "PartsNotNeeded"),
        parts_picked_up: !day(get(l, "PartsPickedUpDate")).is_empty(),
        dispatch: field(l, "DispatchCode"),
        estimate: money(get(l, "EstimatedDuration")),
        tech_start: day(get(l, "TechnicianStartTime")),
        tech_end: day(get(l, "TechnicianEndTime")),
        part_list: list("Parts").iter().map(part_of).collect(),
        sub_lines: list("SubLines")
            .iter()
            .map(|x| {
                ["Description", "Name", "OpCode"]
                    .iter()
                    .map(|k| field(x, k))
                    .find(|s| !s.is_empty())
                    .unwrap_or_default()
            })
            .filter(|s| !s.is_empty())
            .collect(),
        images: list("Images").len(),
        techs: list("LineTechnicians")
            .iter()
            .map(|x| name_of(Some(get(x, "Technician").filter(|v| truthy(Some(v))).unwrap_or(x))))
            .filter(|s| !s.is_empty())
            .collect(),
        added_by: name_of(get(l, "CreateUser")),
        order: number(get(l, "DisplayOrder")).filter(|n| n.is_finite()).unwrap_or(0.0),
    }
}

/// The RO from what Xtime's RO address sends (`{ Data: {…} }` or the RO itself). `staff`: the store's staff list, for
/// names (Xtime's RO has only IDs and numbers for its people); empty when it couldn't be had.
pub fn parse_view(data: &Value, staff: &[Value]) -> Result<RoView> {
    let ro = get(data, "Data").filter(|d| d.is_object()).unwrap_or(data);
    let raw = get(ro, "RepairOrderLines")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Format("Xtime didn't send this RO's lines".into()))?;
    let full_name = |e: &Value| {
        [field(e, "FirstName"), field(e, "LastName")]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let staff_by = |id: Option<&Value>, num: &str| {
        staff.iter().find(|e| {
            id.is_some_and(|id| !id.is_null() && text(get(e, "ID")) == text(Some(id))) || (!num.is_empty() && text(get(e, "Number")) == num)
        })
    };
    let person = |obj: Option<&Value>, id: Option<&Value>| {
        let obj = obj.filter(|o| o.is_object());
        let has_name = obj.is_some_and(|o| truthy(get(o, "FirstName")) || truthy(get(o, "LastName")));
        let e = if has_name {
            obj
        } else {
            let num = obj.map(|o| field(o, "Number")).unwrap_or_default();
            staff_by(id.filter(|v| !v.is_null()).or_else(|| obj.and_then(|o| get(o, "ID"))), &num)
        };
        Person {
            name: e.map(full_name).unwrap_or_else(|| name_of(obj)),
            number: t(e.and_then(|e| get(e, "Number")).or_else(|| obj.and_then(|o| get(o, "Number")))),
        }
    };
    let parts_number = field(ro, "PartsNumber");
    let parts_employee = if parts_number.is_empty() {
        String::new()
    } else {
        match staff.iter().find(|e| text(get(e, "Number")) == parts_number) {
            Some(e) => format!("{} ({parts_number})", full_name(e)),
            None => parts_number,
        }
    };
    let v = get(ro, "Vehicle").filter(|v| v.is_object()).cloned().unwrap_or(json!({}));
    let mut lines: Vec<Line> = raw
        .iter()
        .enumerate()
        .filter(|(_, l)| l.is_object() && !truthy(get(l, "IsDeleted")))
        .map(|(i, l)| Line { index: i, ..line_of(l) })
        .collect();
    lines.sort_by(|a, b| a.order.total_cmp(&b.order)); // stable: Xtime's order for equal ones
    let opened = day(get(ro, "DMSOpenDate"));
    Ok(RoView {
        ro: field(ro, "RONumber"),
        tag: field(ro, "TagNumber"),
        mode: number(get(ro, "CurrentMode")).filter(|n| n.is_finite()).unwrap_or(0.0) as i64,
        mode_since: day(get(ro, "ModeStartTime")),
        waiter: truthy(get(ro, "IsWaiter")),
        express: truthy(get(ro, "IsExpress")),
        internal: truthy(get(ro, "IsInternal")),
        warranty: truthy(get(ro, "IsWarranty")),
        cpo: truthy(get(ro, "IsCPO")),
        on_hold: truthy(get(ro, "IsOnHold")),
        waiting_parts: truthy(get(ro, "WaitingForParts")),
        waiting_customer: truthy(get(ro, "WaitingForCustomer")),
        waiting_warranty: truthy(get(ro, "WaitingForWarranty")),
        part_status: number(get(ro, "PartStatusID")).filter(|n| n.is_finite()).unwrap_or(0.0) as i64,
        chat_channel: field(ro, "ChatChannelId"),
        promised_date: field(ro, "PromisedDate"),
        promised_time: field(ro, "PromisedTime"),
        opened: if opened.is_empty() { day(get(ro, "CreateDate")) } else { opened },
        parts_employee,
        advisor: person(get(ro, "Advisor"), get(ro, "AdvisorID")),
        technician: person(get(ro, "Technician"), get(ro, "TechnicianID")),
        flags: get(ro, "CustomFlags")
            .and_then(Value::as_array)
            .map(|fs| {
                fs.iter()
                    .map(|f| {
                        ["Name", "Description", "Acronym"]
                            .iter()
                            .map(|k| field(f, k))
                            .find(|s| !s.is_empty())
                            .unwrap_or_default()
                    })
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        vehicle: Vehicle {
            year: field(&v, "Year"),
            make: field(&v, "Make"),
            model: field(&v, "Model"),
            vin: field(&v, "VIN").to_uppercase(),
            mileage: number(get(&v, "Mileage")).filter(|m| *m != 0.0),
        },
        lines,
    })
}

// ---------- asking Xtime ----------

impl Xtime {
    /// A call to one of Xtime's addresses, as its RO page makes it (JSON both ways). A refusal comes back with
    /// Xtime's own reason when it gives one (ExceptionMessage / Message).
    pub(crate) async fn ro_call(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        let mut req = self.request(method, path)?.header("Accept", "application/json, text/plain, */*");
        if let Some(b) = body {
            req = req.header("Content-Type", "application/json;charset=UTF-8").body(b.to_string());
        }
        let res = self.send(req).await?;
        if matches!(res.status.as_u16(), 401 | 403) || (res.redirected && !res.is_json()) {
            return Err(Error::SessionExpired);
        }
        if !res.ok() {
            let why = serde_json::from_str::<Value>(&res.body).ok().and_then(|b| {
                ["ExceptionMessage", "Message", "message"]
                    .iter()
                    .find_map(|k| b.get(*k).and_then(Value::as_str).map(str::to_string))
            });
            return Err(match why {
                Some(w) if !w.trim().is_empty() => Error::Format(w),
                _ => Error::Status(res.status.as_u16()),
            });
        }
        Ok(if res.is_json() {
            res.json().unwrap_or(Value::Null)
        } else {
            serde_json::from_str(&res.body).unwrap_or(Value::Null)
        })
    }

    /// One RO for the app's Xtime View (with names from the store's staff list)
    pub async fn ro_view(&self, store: &str, ro: &str) -> Result<RoView> {
        let data = self
            .ro_call(
                Method::GET,
                &format!("/api/Stores/{}/RepairOrder/{}", encode(store.trim()), encode(ro)),
                None,
            )
            .await?;
        let staff = self.staff_list(&crate::paths::employees(store)).await;
        parse_view(&data, staff.as_deref().map(Vec::as_slice).unwrap_or(&[]))
    }

    /// Accepts an RO in Dispatch: it becomes yours and goes to Inspection, exactly like the Accept button on Xtime's RO
    /// page (Xtime's InspectUILibrary: its view-history event, the technician set, then the stage). `tech_id`: your
    /// technician ID (the one your ROs carry).
    pub async fn accept_ro(&self, store: &str, ro: &str, tech_id: &str) -> Result<()> {
        let tech: Value = tech_id
            .trim()
            .parse::<i64>()
            .map(Value::from)
            .unwrap_or_else(|_| Value::from(tech_id.trim()));
        // (Xtime sends these two together, and moves the RO on only when both worked)
        self.ro_call(
            Method::POST,
            &format!("/api/Events/{}", encode(ro)),
            Some(json!({ "EventType": { "ID": 193 }, "Description": "" })),
        )
        .await?;
        self.ro_call(
            Method::PUT,
            &format!(
                "/api/Stores/{}/RepairOrders/{}/Technician?format=json",
                encode(store.trim()),
                encode(ro)
            ),
            Some(json!({ "TechnicianID": tech })),
        )
        .await?;
        self.set_mode(store, ro, 3).await
    }

    /// Moves an RO to another stage (`mode`: 3 Inspection, 4 Approval, 6 Repair, 7 Review), as Xtime's Accept does.
    /// (Only Accept uses it so far: what Xtime's own Send to Approval / Review buttons send isn't known yet.)
    async fn set_mode(&self, store: &str, ro: &str, mode: i64) -> Result<()> {
        let path = format!(
            "/api/Stores/{}/RepairOrders/{}/ROMode?format=json",
            encode(store.trim()),
            encode(ro)
        );
        self.ro_call(Method::PUT, &path, Some(json!({ "ROModeID": mode, "ClosedReason": 1 })))
            .await
            .map(|_| ())
    }

    /// Marks a line done (or not), exactly like Xtime's Done button
    pub async fn set_done(&self, store: &str, ro: &str, line_id: &Value, done: bool) -> Result<()> {
        let path = format!(
            "/api/Stores/{}/RepairOrders/{}/Lines/{}/Completed?format=json",
            encode(store.trim()),
            encode(ro),
            encode(&text(Some(line_id)))
        );
        self.ro_call(Method::PUT, &path, Some(json!({ "ID": line_id, "Value": done })))
            .await
            .map(|_| ())
    }

    /// Moves a line, as dragging it on the RO page does: under a customer concern (`group`: the concern's line ID) or
    /// on its own (none), at `order` (where it lands in that list, from 1: see [`RoView::drop_order`]).
    /// (Xtime's RODetailsService.updateDisplayOrder.)
    pub async fn move_line(&self, store: &str, ro: &str, line_id: &Value, group: Option<&Value>, order: usize) -> Result<()> {
        let path = format!(
            "/api/Stores/{}/RepairOrders/{}/Lines/{}/DisplayOrder?format=json",
            encode(store.trim()),
            encode(ro),
            encode(&text(Some(line_id)))
        );
        let body = json!({ "ID": line_id, "GroupID": group.cloned().unwrap_or(Value::Null), "DisplayOrder": order });
        self.ro_call(Method::PUT, &path, Some(body)).await.map(|_| ())
    }
}

// ---------- times, as the RO page shows them ----------

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// "17:05" → "5:05 PM"
pub fn clock_time(h: u32, m: u32) -> String {
    let (h12, half) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    format!("{h12}:{m:02} {half}")
}

/// Xtime's "2026-11-15T10:30:00" (no zone: the store's local time) → "Nov 15, 10:30 AM"; anything else as it is
pub fn when(s: &str) -> String {
    let n = |a: usize, b: usize| s.get(a..b).and_then(|x| x.parse::<u32>().ok());
    match (n(0, 4), n(5, 7), n(8, 10), n(11, 13), n(14, 16)) {
        (Some(_), Some(mo @ 1..=12), Some(d), Some(h @ 0..=23), Some(mi @ 0..=59)) => {
            format!("{} {d}, {}", MONTHS[mo as usize - 1], clock_time(h, mi))
        }
        (Some(_), Some(mo @ 1..=12), Some(d), _, _) => format!("{} {d}", MONTHS[mo as usize - 1]),
        _ => s.to_string(),
    }
}

/// The promise date and time: "11/15/2026" and "17:00" → ("Nov 15", "5:00 PM")
pub fn promised(date: &str, time: &str) -> (String, String) {
    let parts: Vec<u32> = date.split('/').filter_map(|p| p.trim().parse().ok()).collect();
    let d = match parts.as_slice() {
        [m @ 1..=12, d, _y] => format!("{} {d}", MONTHS[*m as usize - 1]),
        _ => date.to_string(),
    };
    let hm: Vec<u32> = time.split(':').take(2).filter_map(|p| p.trim().parse().ok()).collect();
    let t = match hm.as_slice() {
        [h @ 0..=23, m @ 0..=59] => clock_time(*h, *m),
        _ => time.to_string(),
    };
    (d, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Value {
        json!({ "Data": {
            "RONumber": 820367, "TagNumber": "T12", "CurrentMode": "6", "ModeStartTime": "2026-11-15T09:05:00",
            "ChatChannelId": "3590-820367-2611",
            "IsWaiter": true, "WaitingForParts": true, "PartsNumber": "55",
            "PromisedDate": "11/15/2026", "PromisedTime": "17:00",
            "AdvisorID": 9, "TechnicianID": 77, "Technician": null,
            "Customer": { "FirstName": "Pat", "LastName": "Smith", "Phone": "555" },
            "CustomFlags": [{ "Name": "Loaner" }],
            "Vehicle": { "Year": 2026, "Make": "Ford", "Model": "Bronco", "VIN": "1fmee2bp0tla00000", "Mileage": 1234 },
            "RepairOrderLines": [
                { "ID": 3, "DMSNumber": "A", "IsPrimary": false, "Approved": true, "ServiceName": "Wipers", "DisplayOrder": 3,
                  "GroupID": 1, "Price": 40.5, "Hours": 0.3, "Parts": [{ "PartNumber": "WB-1", "Quantity": 2, "IsInStock": true }] },
                { "ID": 1, "DMSNumber": "A", "IsPrimary": true, "ServiceName": "[Primary]: Check noise", "Complaint": "Noise up front",
                  "DisplayOrder": 1, "Completed": true, "PayTypeCode": { "Code": "C" } },
                { "ID": 2, "DMSNumber": "B", "IsPrimary": false, "ServiceName": "Cabin filter", "DisplayOrder": 2, "Declined": true },
                { "ID": 4, "DMSNumber": "C", "ServiceName": "Q99PX Multipoint", "DisplayOrder": 4 },
                { "ID": 5, "IsDeleted": true, "ServiceName": "gone" }
            ]
        }})
    }

    fn staff() -> Vec<Value> {
        vec![
            json!({ "ID": 9, "Number": "123", "FirstName": "Tony", "LastName": "B" }),
            json!({ "ID": 77, "Number": "456", "FirstName": "Ashton", "LastName": "M" }),
            json!({ "ID": 50, "Number": "55", "FirstName": "Parts", "LastName": "Guy" }),
        ]
    }

    #[test]
    fn reads_the_ro_without_the_customer() {
        let v = parse_view(&sample(), &staff()).unwrap();
        assert_eq!((v.ro.as_str(), v.tag.as_str(), v.mode), ("820367", "T12", 6));
        assert!(v.waiter && v.waiting_parts && !v.express);
        assert_eq!(v.advisor.label(), "Tony B (123)");
        assert_eq!(v.technician.label(), "Ashton M (456)");
        assert_eq!(v.parts_employee, "Parts Guy (55)");
        assert_eq!(v.flags, vec!["Loaner"]);
        assert_eq!(v.vehicle.vin, "1FMEE2BP0TLA00000");
        assert_eq!(v.chat_channel, "3590-820367-2611");
        assert_eq!(v.vehicle.mileage, Some(1234.0));
        assert_eq!(v.lines.iter().map(|l| l.key()).collect::<Vec<_>>(), ["1", "2", "3", "4"]);
        assert_eq!(v.lines.iter().map(|l| l.index).collect::<Vec<_>>(), [1, 2, 0, 3]); // where Xtime had them
        let first = &v.lines[0];
        assert_eq!((first.name.as_str(), first.pay_type.as_str()), ("Check noise", "C"));
        assert!(first.completed && first.primary && first.has_detail());
        assert!(first.can_be_done()); // a concern
        assert!(!v.lines[1].can_be_done()); // declined
        assert!(v.lines[2].can_be_done()); // approved
        assert!(!v.lines[3].can_be_done()); // waiting on approval
        assert_eq!(v.lines[2].part_list[0].status, "In stock");
        assert!(!serde_json::to_string(&v).unwrap().contains("Smith"));
    }

    #[test]
    fn groups_lines_like_the_ro_page() {
        let v = parse_view(&sample(), &[]).unwrap();
        let s = v.sections();
        assert_eq!(s.concerns, vec![(0, vec![2])]); // Wipers (GroupID 1) under the concern
        assert_eq!(s.loose, vec![1]); // Cabin filter (line B, no concern B)
        assert_eq!(s.inspection, vec![3]);
        assert_eq!(v.advisor.label(), ""); // no staff list: no names
    }

    #[test]
    fn works_out_where_a_dropped_line_lands() {
        let data = json!({ "Data": { "RONumber": 1, "RepairOrderLines": [
            { "ID": 1, "DMSNumber": "A", "IsPrimary": true, "ServiceName": "[Primary]: Check noise", "DisplayOrder": 1 },
            { "ID": 7, "IsPrimary": true, "ServiceName": "[Primary]: Oil change", "DisplayOrder": 2 },
            { "ID": 3, "IsPrimary": false, "ServiceName": "Wipers", "GroupID": 1, "DisplayOrder": 1 },
            { "ID": 2, "IsPrimary": false, "ServiceName": "Cabin filter", "DisplayOrder": 1 },
            { "ID": 4, "ServiceName": "Q99PX Multipoint", "DisplayOrder": 2 },
            { "ID": 6, "IsPrimary": false, "ServiceName": "Battery", "DisplayOrder": 3 }
        ] } });
        let v = parse_view(&data, &[]).unwrap();
        let ids = |p: &Place| v.xtime_list(p).iter().map(|l| l.key()).collect::<Vec<_>>();
        assert_eq!(ids(&Place::Concerns), ["1", "7"]);
        assert_eq!(ids(&Place::Under(json!(1))), ["3"]);
        // Xtime's list of recommendations has the inspection line in it too
        assert_eq!(ids(&Place::Recommendations), ["2", "4", "6"]);
        let (two, six) = (json!(2), json!(6));
        // Battery dragged above Cabin filter: first
        assert_eq!(v.drop_order(&json!(6), &Place::Recommendations, Some(&two), &[&two, &six]), 1);
        // Cabin filter dragged to the end (after Battery, the last shown): third, after the inspection line
        assert_eq!(v.drop_order(&json!(2), &Place::Recommendations, None, &[&two, &six]), 3);
        // Cabin filter under the noise concern, after Wipers: second there
        assert_eq!(v.drop_order(&json!(2), &Place::Under(json!(1)), None, &[&json!(3)]), 2);
        // under a concern with nothing under it yet: first
        assert_eq!(v.drop_order(&json!(2), &Place::Under(json!(7)), None, &[]), 1);
        // the oil change concern moved above the noise one
        assert_eq!(v.drop_order(&json!(7), &Place::Concerns, Some(&json!(1)), &[]), 1);
    }

    #[test]
    fn shows_times_as_the_page_does() {
        assert_eq!(when("2026-11-15T09:05:00"), "Nov 15, 9:05 AM");
        assert_eq!(when("2026-11-15T17:30:00.123"), "Nov 15, 5:30 PM");
        assert_eq!(when("soon"), "soon");
        assert_eq!(promised("11/15/2026", "17:00"), ("Nov 15".into(), "5:00 PM".into()));
        assert_eq!(promised("", ""), (String::new(), String::new()));
        assert_eq!(clock_time(0, 5), "12:05 AM");
        assert_eq!(mode_name(6), "Being Repaired");
    }

    #[test]
    fn counts_approved_recommendations_for_the_spiff() {
        let mut v = parse_view(&sample(), &[]).unwrap();
        // Wipers (approved, $40.50) counts; the concern, the declined filter and the inspection line don't
        assert_eq!(v.spiff_lines().iter().map(|l| l.key()).collect::<Vec<_>>(), ["3"]);
        assert!(!v.spiff_earned());
        v.lines[2].total = Some(150.0);
        assert!(!v.spiff_earned()); // $150 isn't more than $150
        v.lines[2].total = Some(150.01);
        assert!(v.spiff_earned());
    }

    #[test]
    fn errors_without_lines() {
        assert!(matches!(parse_view(&json!({ "Data": {} }), &[]), Err(Error::Format(_))));
    }
}
