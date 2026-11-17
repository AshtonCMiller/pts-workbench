//! Your ROs from Xtime's dashboard (the Electron app's `dashboard.js`): the same data Xtime's own dashboard loads,
//! reduced to what the app shows for one technician (RO, tag, vehicle, mode, waiting flags, dashboard columns).
//! The customer's last name is the only customer detail kept, as Xtime's dashboard shows it, and only on this computer.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::Value;
use workbench_net::Method;

use crate::json::{field_names, flag, get, js_string, no_leading_zeros, pick, rx, sub, text};
use crate::{Error, Result, Xtime};

#[derive(Debug, Clone, Serialize)]
pub struct Dashboard {
    /// ROs on the whole dashboard (everyone's)
    pub total: usize,
    /// Yours
    pub mine: Vec<AssignedRo>,
    /// ROs in Dispatch with no technician yet (anyone's to accept)
    pub dispatch: Vec<AssignedRo>,
    /// Your name as Xtime has it on your ROs (for the window title); "" when none says
    pub tech_name: String,
    /// The field names (no values) of your first RO, to map Xtime's loosely named dashboard columns
    pub field_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AssignedRo {
    pub ro: String,
    pub tag: String,
    pub year: String,
    pub make: String,
    pub model: String,
    /// In capitals
    pub vin: String,
    /// Xtime's mode number (where the RO is: dispatched, in progress…)
    pub mode: Option<f64>,
    pub waiting_parts: bool,
    pub waiting_customer: bool,
    pub waiting_warranty: bool,
    pub dash: DashColumns,
}

/// The columns of the app's own Xtime dashboard. Xtime's field names aren't documented, so they're matched loosely
/// (see [`Dashboard::field_names`]); values come as Xtime sends them.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DashColumns {
    pub promised: Option<Value>,
    pub media: Option<Value>,
    pub quote: Option<Value>,
    /// The customer's last name (as Xtime's dashboard shows it)
    pub customer: Option<Value>,
    /// The advisor's number or ID
    pub advisor: Option<Value>,
    pub advisor_name: String,
    pub mode_since: Option<Value>,
    pub mode_time: Option<Value>,
    pub parts_since: Option<Value>,
    pub parts_time: Option<Value>,
    pub parts_status: Option<Value>,
    pub express: bool,
    pub waiter: bool,
    pub on_hold: bool,
    pub not_in_stock: bool,
    pub is_new: bool,
}

/// Every RO on the dashboard (they're grouped under DashboardModes; found at any depth, each once, in order)
fn all_ros(data: &Value) -> Vec<&Value> {
    fn walk<'a>(node: &'a Value, seen: &mut HashSet<String>, out: &mut Vec<&'a Value>) {
        match node {
            Value::Array(list) => list.iter().for_each(|v| walk(v, seen, out)),
            Value::Object(map) => {
                if let (Some(ro), Some(_)) = (get(node, "RONumber"), get(node, "Vehicle")) {
                    if seen.insert(js_string(Some(ro))) {
                        out.push(node);
                    }
                    return;
                }
                for v in map.values().filter(|v| v.is_object() || v.is_array()) {
                    walk(v, seen, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(data, &mut HashSet::new(), &mut out);
    out
}

/// Names for advisor numbers and IDs, from the store's staff list (only names are used)
fn staff_names(staff: &[Value]) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for e in staff {
        let full = ["FirstName", "LastName"]
            .iter()
            .filter_map(|k| get(e, k).and_then(Value::as_str).filter(|s| !s.is_empty()))
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        if full.is_empty() {
            continue;
        }
        if let Some(n) = get(e, "Number").filter(|v| !v.is_null()) {
            names.entry(text(Some(n))).or_insert_with(|| full.clone());
        }
        if let Some(id) = get(e, "ID").filter(|v| !v.is_null()) {
            names.insert(format!("id:{}", text(Some(id))), full);
        }
    }
    names
}

fn advisor_name(ro: &Value, adv: Option<&Value>, num: Option<&Value>, names: &HashMap<String, String>) -> String {
    if let Some(n) =
        pick(Some(ro), &[rx!("(?i)^(service)?advisor_?(full_?)?name$")]).or_else(|| pick(adv, &[rx!("(?i)^(full_?|display_?)?name$")]))
    {
        return text(Some(&n));
    }
    let first = pick(adv, &[rx!("(?i)^first_?name$")]);
    let last = pick(adv, &[rx!("(?i)^last_?name$")]);
    if first.is_some() || last.is_some() {
        return [first, last].iter().flatten().map(|v| text(Some(v))).collect::<Vec<_>>().join(" ");
    }
    let num = match num {
        Some(n) if !n.is_null() && n.as_str() != Some("") => text(Some(n)),
        _ => return String::new(),
    };
    names
        .get(&num)
        .or_else(|| names.get(&no_leading_zeros(&num)))
        .or_else(|| names.get(&format!("id:{num}")))
        .cloned()
        .unwrap_or_default()
}

fn dash_of(ro: &Value, names: &HashMap<String, String>) -> DashColumns {
    let r = Some(ro);
    let cust = sub(ro, rx!("(?i)^customer$"));
    let adv = sub(ro, rx!("(?i)^(service)?advisor$"));
    let advisor = pick(r, &[rx!("(?i)^(service)?advisor_?(number|id|num|no)$")]).or_else(|| pick(adv, &[rx!("(?i)^(number|id)$")]));
    DashColumns {
        promised: pick(r, &[rx!("(?i)^promise(d)?(date)?(time)?$"), rx!("(?i)promise")]),
        media: pick(r, &[rx!("(?i)^media(type|sent|status)?$"), rx!("(?i)media")]),
        quote: pick(r, &[rx!("(?i)^quote(type|sent|status)?$"), rx!("(?i)quote")]),
        customer: pick(r, &[rx!("(?i)^customer_?last_?name$")])
            .or_else(|| pick(cust, &[rx!("(?i)^last_?name$")]))
            .or_else(|| pick(r, &[rx!("(?i)^customer_?name$"), rx!("(?i)^last_?name$")]))
            .or_else(|| pick(cust, &[rx!("(?i)name")])),
        advisor_name: advisor_name(ro, adv, advisor.as_ref(), names),
        advisor,
        mode_since: pick(
            r,
            &[
                rx!("(?i)^(current)?mode_?(start|change[d]?|updated?)?_?(date|time|datetime|on)$"),
                rx!("(?i)mode.*(date|time)"),
            ],
        ),
        mode_time: pick(r, &[rx!("(?i)^mode_?time$"), rx!("(?i)time_?in_?mode")]),
        parts_since: pick(
            r,
            &[
                rx!("(?i)^parts?_?(start|request(ed)?|sent)?_?(date|time|datetime)$"),
                rx!("(?i)parts?.*(date|time)"),
            ],
        ),
        parts_time: pick(r, &[rx!("(?i)^parts?_?time$")]),
        parts_status: pick(r, &[rx!("(?i)^parts?_?status$"), rx!("(?i)parts?_?status")]),
        express: flag(pick(r, &[rx!("(?i)^is_?express$"), rx!("(?i)^express(ro)?$")]).as_ref()),
        waiter: flag(
            pick(
                r,
                &[rx!("(?i)^is_?waiter$"), rx!("(?i)^waiter$"), rx!("(?i)customer_?(is_?)?waiting")],
            )
            .as_ref(),
        ),
        on_hold: flag(pick(r, &[rx!("(?i)^(is_?)?on_?hold$"), rx!("(?i)hold")]).as_ref()),
        not_in_stock: flag(pick(r, &[rx!("(?i)not_?in_?stock"), rx!("(?i)parts?_?not_?(in_?)?stock")]).as_ref()),
        is_new: flag(pick(r, &[rx!("(?i)^is_?new$"), rx!("(?i)new_?in_?mode")]).as_ref()),
    }
}

/// The technician's name from one of their ROs (TechnicianName / TechName / Technician: { FirstName, LastName } …)
fn tech_name_from(ro: &Value) -> String {
    if let Some(map) = ro.as_object() {
        for (k, v) in map {
            if rx!("(?i)^tech(nician)?_?(full_?)?name$").is_match(k) {
                if let Some(s) = v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                    return s.to_string();
                }
            }
        }
    }
    if let Some(t) = get(ro, "Technician").filter(|t| t.is_object()) {
        for k in ["FullName", "Name", "DisplayName"] {
            if let Some(s) = get(t, k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
                return s.to_string();
            }
        }
        let parts: Vec<&str> = ["FirstName", "LastName"]
            .iter()
            .filter_map(|k| get(t, k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()))
            .collect();
        if !parts.is_empty() {
            return parts.join(" ");
        }
    }
    String::new()
}

/// One technician's ROs from the dashboard's data. `staff`: the store's staff list (for advisor names), or empty.
pub fn extract(data: &Value, tech_id: &str, staff: &[Value]) -> Dashboard {
    let all = all_ros(data);
    let names = staff_names(staff);
    let is_mine = |ro: &Value| js_string(get(ro, "TechnicianID")) == tech_id;
    let mut field_list = None;
    let mut mine = Vec::new();
    let mut dispatch = Vec::new();
    for &ro in all.iter() {
        let mine_too = is_mine(ro);
        // in Dispatch (mode 2) with no technician: anyone's to accept
        let unassigned =
            matches!(get(ro, "TechnicianID"), None | Some(Value::Null)) || matches!(js_string(get(ro, "TechnicianID")).trim(), "" | "0");
        let in_dispatch = get(ro, "CurrentMode").and_then(|m| match m {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().parse::<f64>().ok(),
            _ => None,
        }) == Some(2.0);
        if !mine_too && !(unassigned && in_dispatch) {
            continue;
        }
        // the vehicle, or the first of a list of them
        let vehicle = get(ro, "Vehicle").and_then(|v| match v {
            Value::Array(a) => a.first(),
            v => Some(v),
        });
        let v = |k: &str| text(vehicle.and_then(|v| get(v, k)));
        // JavaScript's Number(): null and "" are 0, missing or not a number is nothing
        let mode = get(ro, "CurrentMode").and_then(|m| match m {
            Value::Null => Some(0.0),
            Value::Number(n) => n.as_f64(),
            Value::String(s) if s.trim().is_empty() => Some(0.0),
            Value::String(s) => s.trim().parse::<f64>().ok(),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        });
        let row = AssignedRo {
            ro: text(get(ro, "RONumber")),
            tag: text(get(ro, "TagNumber")),
            year: v("year"),
            make: v("make"),
            model: v("model"),
            vin: v("VIN").to_uppercase(),
            mode: mode.filter(|m| m.is_finite()),
            waiting_parts: get(ro, "WaitingForParts") == Some(&Value::Bool(true)),
            waiting_customer: get(ro, "WaitingForCustomer") == Some(&Value::Bool(true)),
            waiting_warranty: get(ro, "WaitingForWarranty") == Some(&Value::Bool(true)),
            dash: dash_of(ro, &names),
        };
        if !mine_too {
            dispatch.push(row);
            continue;
        }
        mine.push(row);
        if field_list.is_none() {
            field_list = ro.as_object().map(field_names);
        }
    }
    let tech_name = all
        .iter()
        .filter(|ro| is_mine(ro))
        .map(|ro| tech_name_from(ro))
        .find(|n| !n.is_empty())
        .unwrap_or_default();
    Dashboard {
        total: all.len(),
        mine,
        dispatch,
        tech_name,
        field_names: field_list,
    }
}

impl Xtime {
    /// Your ROs. `api_path`: normally [`crate::paths::DASHBOARD`]; `employees_path`: the staff list's address with the
    /// store filled in (for advisor names; "" to skip)
    pub async fn dashboard(&self, tech_id: &str, api_path: &str, employees_path: &str) -> Result<Dashboard> {
        let res = self
            .send(self.request(Method::GET, api_path)?.header("Accept", "application/json"))
            .await?;
        if matches!(res.status.as_u16(), 401 | 403) {
            return Err(Error::SessionExpired);
        }
        if !res.ok() {
            return Err(Error::Status(res.status.as_u16()));
        }
        // a run-out sign-in comes back as Xtime's sign-in page (HTML, not data)
        if res.redirected || !res.is_json() {
            return Err(Error::SessionExpired);
        }
        let data: Value = res
            .json()
            .map_err(|e| Error::Format(format!("Xtime's dashboard wasn't readable: {e}")))?;
        let staff = if employees_path.is_empty() {
            None
        } else {
            self.staff_list(employees_path).await
        };
        Ok(extract(&data, tech_id, staff.as_deref().map(Vec::as_slice).unwrap_or(&[])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({ "DashboardModes": [
            { "Mode": 1, "ROs": [
                { "RONumber": 820367, "TagNumber": "T12", "TechnicianID": 77, "CurrentMode": 3,
                  "Vehicle": { "year": 2026, "make": "Ford", "model": "Bronco", "VIN": "1fmee2bp0tla00000" },
                  "WaitingForParts": true, "PromiseDateTime": "2026-11-14T17:00:00", "AdvisorNumber": "0123",
                  "Customer": { "FirstName": "Pat", "LastName": "Smith", "Phone": "555" }, "IsWaiter": "Y",
                  "Technician": { "FirstName": "Ashton", "LastName": "M" } },
                { "RONumber": 820400, "TechnicianID": 12, "Vehicle": [{ "VIN": "x" }] }
            ]},
            { "Mode": 3, "ROs": [ { "RONumber": 820367, "TechnicianID": 77, "Vehicle": {} } ] }
        ]})
    }

    #[test]
    fn finds_my_ros_once() {
        let staff = vec![json!({ "ID": 9, "Number": "123", "FirstName": "Tony", "LastName": "B" })];
        let d = extract(&sample(), "77", &staff);
        assert_eq!(d.total, 2);
        assert_eq!(d.mine.len(), 1);
        let ro = &d.mine[0];
        assert_eq!(
            (ro.ro.as_str(), ro.tag.as_str(), ro.year.as_str(), ro.vin.as_str()),
            ("820367", "T12", "2026", "1FMEE2BP0TLA00000")
        );
        assert_eq!(ro.mode, Some(3.0));
        assert!(ro.waiting_parts && !ro.waiting_customer);
        assert_eq!(ro.dash.promised, Some(json!("2026-11-14T17:00:00")));
        assert_eq!(ro.dash.customer, Some(json!("Smith")));
        assert_eq!(ro.dash.advisor, Some(json!("0123")));
        assert_eq!(ro.dash.advisor_name, "Tony B");
        assert!(ro.dash.waiter);
        assert_eq!(d.tech_name, "Ashton M");
        assert!(d.field_names.unwrap().iter().any(|f| f.starts_with("Vehicle{")));
    }

    #[test]
    fn unassigned_ros_in_dispatch() {
        let data = json!({ "DashboardModes": [
            { "Mode": 2, "ROs": [
                { "RONumber": 1, "TechnicianID": null, "CurrentMode": 2, "Vehicle": { "VIN": "a" } },
                { "RONumber": 2, "TechnicianID": 0, "CurrentMode": "2", "Vehicle": { "VIN": "b" } },
                { "RONumber": 3, "TechnicianID": 12, "CurrentMode": 2, "Vehicle": { "VIN": "c" } },
                { "RONumber": 4, "CurrentMode": 3, "Vehicle": { "VIN": "d" } },
                { "RONumber": 5, "TechnicianID": 77, "CurrentMode": 2, "Vehicle": { "VIN": "e" } }
            ]}
        ]});
        let d = extract(&data, "77", &[]);
        let ros: Vec<&str> = d.dispatch.iter().map(|r| r.ro.as_str()).collect();
        assert_eq!(ros, ["1", "2"]);
        assert_eq!(d.mine.len(), 1);
        assert_eq!(d.mine[0].ro, "5");
    }

    #[test]
    fn nobody_else_s_ros() {
        assert!(extract(&sample(), "5", &[]).mine.is_empty());
        assert_eq!(extract(&sample(), "12", &[]).mine[0].vin, "X");
    }
}
