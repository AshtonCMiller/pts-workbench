//! Service lines, the way Xtime's Add Service window does it (the Electron app's `inspect.js`: catalog, opcodes,
//! addLine, editLine, deleteLine): the store's services, labor types and dispatch codes; its CDK op codes; adding a
//! line, changing one already on the RO, deleting one.
//!
//! What's typed is a [`LineInput`]; [`new_line`] and [`edited_line`] turn it into the line Xtime's window would send
//! (tested without Xtime).

use serde::Serialize;
use serde_json::{Value, json};
use workbench_net::Method;

use crate::employee::encode;
use crate::inspect::unwrap_data;
use crate::json::{get, text};
use crate::{Error, Result, Xtime};

/// What the Add Service window offers
#[derive(Debug, Clone, Default, Serialize)]
pub struct Catalog {
    pub services: Vec<CatService>,
    pub pay_types: Vec<PayType>,
    pub dispatch_codes: Vec<DispatchCode>,
    /// A new line's colour unless picked ("Red")
    pub default_color: String,
    /// An internal RO (its default labor type is the internal one)
    pub internal: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CatService {
    pub sgcid: Value,
    pub sgid: Value,
    pub name: String,
    pub op_code: String,
    pub price: f64,
    pub labor: f64,
}

impl CatService {
    /// "* Used Car Check" → "Used Car Check": what Details, Concern and Correction start as
    pub fn plain_name(&self) -> String {
        let t: &[char] = &[' ', '*', '•', '-', '–', '—', '"', '“', '”', '\''];
        let s = self.name.trim_start_matches(t).trim_end_matches(['"', '“', '”', '\'']);
        let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
        if s.is_empty() { self.name.clone() } else { s }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PayType {
    pub id: Value,
    /// "C", "W", "I"…
    pub code: String,
    /// 1 customer, 2 internal…
    pub pay_type_id: Option<i64>,
    pub is_default: bool,
}

impl std::fmt::Display for PayType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.code)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DispatchCode {
    pub code: String,
    pub description: String,
    pub is_default: bool,
    /// Its estimated duration (hours)
    pub duration: Option<f64>,
}

impl std::fmt::Display for DispatchCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.description.is_empty() {
            f.write_str(&self.code)
        } else {
            write!(f, "{} · {}", self.code, self.description)
        }
    }
}

/// One of the store's CDK op codes (Xtime's Op Code box)
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct OpCode {
    pub id: String,
    pub description: String,
    pub complaint: String,
    pub dispatch: String,
    pub duration: Option<f64>,
    pub hours: Option<f64>,
    pub sell: Option<f64>,
}

impl Catalog {
    /// The labor type a new line starts with: the store's default for this kind of RO (customer or internal)
    pub fn default_pay(&self) -> Option<&PayType> {
        let kind = if self.internal { 2 } else { 1 };
        self.pay_types
            .iter()
            .find(|p| p.is_default && p.pay_type_id == Some(kind))
            .or_else(|| self.pay_types.iter().find(|p| p.is_default))
    }
    pub fn default_dispatch(&self) -> Option<&DispatchCode> {
        self.dispatch_codes
            .iter()
            .find(|c| c.is_default)
            .or_else(|| self.dispatch_codes.first())
    }
}

/// What's filled in in the window. Numbers are as typed: empty means "the service's own", as in Xtime.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LineInput {
    pub sgcid: Value,
    pub sgid: Value,
    /// "Yellow" or "Red"
    pub color: String,
    pub pay_type_id: Option<Value>,
    pub hours: String,
    pub labor: String,
    pub parts: String,
    pub total: String,
    pub dispatch_code: String,
    pub duration: String,
    /// The CDK op code
    pub op_code_dms: String,
    pub details: String,
    pub complaint: String,
    pub cause: String,
    pub correction: String,
    pub tech_note: String,
}

impl LineInput {
    /// What's missing or wrong before it can go to Xtime (as the window checks)
    pub fn problem(&self) -> Option<&'static str> {
        if self.sgcid.is_null() {
            return Some("Pick a service from the list.");
        }
        if self.op_code_dms.trim().is_empty() {
            return Some("Pick a CDK op code (type to search).");
        }
        if self.details.trim().is_empty() {
            return Some("Details are required.");
        }
        if !self.duration.trim().is_empty() && !self.duration.trim().parse::<f64>().is_ok_and(|d| d > 0.0) {
            return Some("Estimated duration should be a number of hours, like 0.5.");
        }
        for v in [&self.hours, &self.labor, &self.parts, &self.total] {
            if !v.trim().is_empty() && v.trim().trim_start_matches('$').replace(',', "").parse::<f64>().is_err() {
                return Some("Hours and prices should be numbers.");
            }
        }
        None
    }
}

/// As Xtime: an empty number is -1 ("use the service's own")
fn num(v: &str) -> f64 {
    v.trim().trim_start_matches('$').replace(',', "").parse::<f64>().unwrap_or(-1.0)
}

fn duration(v: &str) -> Option<f64> {
    v.trim().parse::<f64>().ok().filter(|d| *d > 0.0)
}

fn find_pay<'a>(pay: &'a [Value], id: &Value) -> Option<&'a Value> {
    pay.iter().find(|p| text(get(p, "ID")) == text(Some(id)))
}

fn is_default(v: &Value) -> bool {
    get(v, "IsDefault") == Some(&Value::Bool(true))
}

/// A new line (ID 0), as Xtime's EditFormService.saveLine sends it. `pay`: the store's labor types, `store`: its
/// data, `codes`: its dispatch codes (as Xtime has them).
pub fn new_line(a: &LineInput, pay: &[Value], store: &Value, codes: &[Value]) -> Result<Value> {
    if a.sgcid.is_null() {
        return Err(Error::Format("Pick a service first".into()));
    }
    // dispatch code and duration: as Xtime's Add Service, the store's default code and its duration unless picked
    let fallback = codes.iter().find(|c| is_default(c));
    let dispatch = if a.dispatch_code.trim().is_empty() {
        fallback.map(|c| text(get(c, "Code"))).unwrap_or_default()
    } else {
        a.dispatch_code.trim().to_string()
    };
    let info = codes.iter().find(|c| text(get(c, "Code")) == dispatch).or(fallback);
    let dur = duration(&a.duration)
        .map(Value::from)
        .or_else(|| info.and_then(|c| get(c, "EstimatedDuration")).cloned())
        .or_else(|| fallback.and_then(|c| get(c, "EstimatedDuration")).cloned())
        .unwrap_or(Value::Null);
    let pay_code = a
        .pay_type_id
        .as_ref()
        .and_then(|id| find_pay(pay, id))
        .or_else(|| pay.iter().find(|p| is_default(p)))
        .cloned()
        .unwrap_or(Value::Null);
    let labor = get(store, "Labor");
    let color = if a.color.is_empty() {
        get(store, "DefaultLineColor").and_then(Value::as_str).unwrap_or("Red").to_string()
    } else {
        a.color.clone()
    };
    let opt = |v: Option<&Value>| v.cloned().unwrap_or(Value::Null);
    Ok(obj(vec![
        ("ID", Value::from(0)),
        ("SGCID", a.sgcid.clone()),
        ("SGID", a.sgid.clone()),
        ("Color", Value::from(color)),
        ("PayTypeCode", pay_code),
        ("Hours", Value::from(num(&a.hours))),
        ("PriceLabor", Value::from(num(&a.labor))),
        ("PriceParts", Value::from(num(&a.parts))),
        ("Price", Value::from(num(&a.total))),
        ("ServiceGuideSpecialItemName", Value::from(a.details.clone())),
        ("Complaint", Value::from(a.complaint.clone())),
        ("Cause", Value::from(a.cause.clone())),
        ("Correction", Value::from(a.correction.clone())),
        ("TechnicianNote", Value::from(a.tech_note.clone())),
        ("IsGbb", Value::from(false)),
        ("ServiceType", Value::Null),
        (
            "DispatchCode",
            if dispatch.is_empty() { Value::Null } else { Value::from(dispatch) },
        ),
        ("EstimatedDuration", dur),
        (
            "OpCodeDms",
            if a.op_code_dms.trim().is_empty() {
                Value::Null
            } else {
                Value::from(a.op_code_dms.trim())
            },
        ),
        (
            "EnableInternalLinePrice",
            opt(labor.and_then(|l| get(l, "InternalLinePriceDefaultValue"))),
        ),
        (
            "EnableWarrantyLinePrice",
            opt(labor.and_then(|l| get(l, "WarrantyLinePriceDefaultValue"))),
        ),
    ]))
}

/// An object from its fields, in order
fn obj(fields: Vec<(&str, Value)>) -> Value {
    Value::Object(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// A line already on the RO (as Xtime has it) with the changes: what Xtime's window sends when saving an edit
pub fn edited_line(old: &Value, a: &LineInput, pay: &[Value], codes: &[Value]) -> Result<Value> {
    let mut line = old.clone();
    let Some(m) = line.as_object_mut() else {
        return Err(Error::Format("That line isn't readable".into()));
    };
    if !a.sgcid.is_null() {
        m.insert("SGCID".into(), a.sgcid.clone());
        if !a.sgid.is_null() {
            m.insert("SGID".into(), a.sgid.clone());
        }
    }
    if !a.color.is_empty() {
        m.insert("Color".into(), Value::from(a.color.clone()));
    }
    // the labor type: the one picked, or the line's own (as the store has it now)
    let pay_id = a
        .pay_type_id
        .clone()
        .or_else(|| m.get("PayTypeCode").and_then(|p| get(p, "ID")).cloned());
    if let Some(p) = pay_id.as_ref().and_then(|id| find_pay(pay, id)) {
        m.insert("PayTypeCode".into(), p.clone());
    }
    m.insert("Hours".into(), Value::from(num(&a.hours)));
    m.insert("PriceLabor".into(), Value::from(num(&a.labor)));
    m.insert("PriceParts".into(), Value::from(num(&a.parts)));
    m.insert("Price".into(), Value::from(num(&a.total)));
    m.insert("ServiceGuideSpecialItemName".into(), Value::from(a.details.clone()));
    m.insert("Complaint".into(), Value::from(a.complaint.clone()));
    m.insert("Cause".into(), Value::from(a.cause.clone()));
    m.insert("Correction".into(), Value::from(a.correction.clone()));
    m.insert("TechnicianNote".into(), Value::from(a.tech_note.clone()));
    if !a.dispatch_code.trim().is_empty() {
        m.insert("DispatchCode".into(), Value::from(a.dispatch_code.trim()));
    }
    if !a.op_code_dms.trim().is_empty() {
        m.insert("OpCodeDms".into(), Value::from(a.op_code_dms.trim()));
    }
    match duration(&a.duration) {
        Some(d) => {
            m.insert("EstimatedDuration".into(), Value::from(d));
        }
        None if m.get("EstimatedDuration").is_none_or(|v| v.is_null() || v.as_f64() == Some(0.0)) => {
            let dc = m.get("DispatchCode").map(|v| text(Some(v))).unwrap_or_default();
            let d = codes
                .iter()
                .find(|c| text(get(c, "Code")) == dc)
                .or_else(|| codes.iter().find(|c| is_default(c)))
                .and_then(|c| get(c, "EstimatedDuration"))
                .cloned()
                .unwrap_or(Value::Null);
            m.insert("EstimatedDuration".into(), d);
        }
        None => {}
    }
    // as Xtime: the address only wants the service type's code
    let st = match (m.get("ServiceTypeFull"), m.get("ServiceType")) {
        (Some(full), _) if full.is_object() => get(full, "ServiceTypeCode").cloned().unwrap_or(Value::Null),
        (_, Some(t)) if t.is_object() => get(t, "ServiceTypeCode").cloned().unwrap_or(Value::Null),
        (_, Some(t)) => t.clone(),
        _ => Value::Null,
    };
    m.insert("ServiceType".into(), st);
    if m.get("SGCID").is_none_or(Value::is_null) {
        return Err(Error::Format("Pick a service first".into()));
    }
    Ok(line)
}

fn number(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// The catalog from the store's scripts' data
pub fn parse_catalog(services: &[Value], pay: &[Value], store: &Value, codes: &[Value], internal: bool) -> Catalog {
    Catalog {
        services: services
            .iter()
            .filter(|s| get(s, "SGCID").is_some_and(|v| !v.is_null()))
            .map(|s| CatService {
                sgcid: get(s, "SGCID").cloned().unwrap_or(Value::Null),
                sgid: get(s, "SGID").cloned().unwrap_or(Value::Null),
                name: text(get(s, "ServiceName")),
                op_code: text(get(s, "OpCode")),
                price: number(get(s, "Price")).unwrap_or(0.0),
                labor: number(get(s, "PriceLabor")).unwrap_or(0.0),
            })
            .collect(),
        pay_types: pay
            .iter()
            .map(|p| PayType {
                id: get(p, "ID").cloned().unwrap_or(Value::Null),
                code: text(get(p, "Code")),
                pay_type_id: get(p, "PayTypeID").and_then(Value::as_i64),
                is_default: is_default(p),
            })
            .collect(),
        dispatch_codes: codes
            .iter()
            .filter(|c| get(c, "Code").is_some_and(|v| !v.is_null() && text(Some(v)) != ""))
            .map(|c| DispatchCode {
                code: text(get(c, "Code")),
                description: text(get(c, "Description")).trim().to_string(),
                is_default: is_default(c),
                duration: number(get(c, "EstimatedDuration")),
            })
            .collect(),
        default_color: get(store, "DefaultLineColor")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("Red")
            .to_string(),
        internal,
    }
}

// ---------- asking Xtime ----------

/// The store's data for lines: its labor types, dispatch codes and store data (as Xtime's scripts have them)
struct LineData {
    pay: Vec<Value>,
    codes: Vec<Value>,
    store: Value,
    services: Vec<Value>,
}

impl Xtime {
    async fn line_data(&self, ro: &str) -> Result<LineData> {
        let d = self.details(ro).await?;
        let arr = |v: Option<Value>| v.and_then(|v| v.as_array().cloned()).unwrap_or_default();
        let services = arr(self.script_var(d.services.as_deref(), "AvailableServices", false).await);
        let pay = arr(self.script_var(d.pay.as_deref(), "AvailablePayTypeCodes", false).await);
        let store = self.script_var(d.store.as_deref(), "StoreData", false).await.unwrap_or(Value::Null);
        let mut codes = arr(self.script_var(d.dispatch.as_deref(), "DispatchCodes", false).await);
        if codes.is_empty() {
            codes = crate::inspect::var_value(&d.html, "DispatchCodes")
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
        }
        Ok(LineData {
            pay,
            codes,
            store,
            services,
        })
    }

    async fn ro_data(&self, store: &str, ro: &str) -> Result<Value> {
        Ok(unwrap_data(
            self.ro_call(
                Method::GET,
                &format!("/api/Stores/{}/RepairOrder/{}", encode(store.trim()), encode(ro)),
                None,
            )
            .await?,
        ))
    }

    /// What the Add Service window offers for this RO
    pub async fn catalog(&self, store: &str, ro: &str) -> Result<Catalog> {
        let d = self.line_data(ro).await?;
        let r = self.ro_data(store, ro).await.unwrap_or(Value::Null);
        let c = parse_catalog(
            &d.services,
            &d.pay,
            &d.store,
            &d.codes,
            get(&r, "IsInternal") == Some(&Value::Bool(true)),
        );
        if c.services.is_empty() {
            return Err(Error::Format("Couldn't read the store's services from Xtime".into()));
        }
        Ok(c)
    }

    /// The store's CDK op codes matching what's typed (code or description)
    pub async fn op_codes(&self, store: &str, search: &str) -> Result<Vec<OpCode>> {
        let q = search.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let data = self
            .ro_call(
                Method::GET,
                &format!("/api/Stores/{}/OpCodes/?SearchString={}", encode(store.trim()), encode(q)),
                None,
            )
            .await?;
        let list = match unwrap_data(data) {
            Value::Array(a) => a,
            _ => Vec::new(),
        };
        Ok(list
            .iter()
            .filter(|o| get(o, "Id").is_some_and(|v| !v.is_null()))
            .take(60)
            .map(|o| OpCode {
                id: text(get(o, "Id")),
                description: text(get(o, "Description")).trim().to_string(),
                complaint: text(get(o, "Complaint")).trim().to_string(),
                dispatch: text(get(o, "DispatchCode")),
                duration: number(get(o, "EstimatedDuration")),
                hours: number(get(o, "FlatHours")),
                sell: number(get(o, "FlatSellRate")),
            })
            .collect())
    }

    /// Adds a service line to the RO
    pub async fn add_line(&self, store: &str, ro: &str, a: &LineInput) -> Result<()> {
        let d = self.line_data(ro).await?;
        let line = new_line(a, &d.pay, &d.store, &d.codes)?;
        self.ro_call(
            Method::PUT,
            &format!("{}/Lines/0?format=json", Self::api(store, ro)),
            Some(json!({ "Data": line })),
        )
        .await?;
        Ok(())
    }

    /// Saves changes to a line already on the RO (the line as Xtime has it now, with the changes)
    pub async fn edit_line(&self, store: &str, ro: &str, line_id: &Value, a: &LineInput) -> Result<()> {
        let d = self.line_data(ro).await?;
        let r = self.ro_data(store, ro).await?;
        let lines = ["RepairOrderLines", "Lines"]
            .iter()
            .find_map(|k| get(&r, k).and_then(Value::as_array))
            .cloned()
            .unwrap_or_default();
        let old = lines
            .iter()
            .find(|l| text(get(l, "ID")) == text(Some(line_id)))
            .ok_or_else(|| Error::Format("That line isn't on the RO anymore (refresh)".into()))?;
        let line = edited_line(old, a, &d.pay, &d.codes)?;
        self.ro_call(
            Method::PUT,
            &format!("{}/Lines/{}?format=json", Self::api(store, ro), encode(&text(Some(line_id)))),
            Some(json!({ "Data": line })),
        )
        .await?;
        Ok(())
    }

    /// Deletes a line from the RO (Xtime's Delete Service)
    pub async fn delete_line(&self, store: &str, ro: &str, line_id: &Value) -> Result<()> {
        self.ro_call(
            Method::DELETE,
            &format!("{}/Lines/{}?format=json", Self::api(store, ro), encode(&text(Some(line_id)))),
            None,
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pay() -> Vec<Value> {
        vec![
            json!({ "ID": 1, "Code": "C", "PayTypeID": 1, "IsDefault": true }),
            json!({ "ID": 2, "Code": "I", "PayTypeID": 2, "IsDefault": true }),
            json!({ "ID": 3, "Code": "W", "PayTypeID": 3, "IsDefault": false }),
        ]
    }
    fn codes() -> Vec<Value> {
        vec![
            json!({ "Code": "A", "Description": "Advisor", "IsDefault": false, "EstimatedDuration": 0.5 }),
            json!({ "Code": "T", "Description": "Tech", "IsDefault": true, "EstimatedDuration": 1.0 }),
        ]
    }
    fn input() -> LineInput {
        LineInput {
            sgcid: json!(55),
            sgid: json!(9),
            color: "Yellow".into(),
            op_code_dms: "BRK".into(),
            details: "Front brakes".into(),
            labor: "120".into(),
            ..Default::default()
        }
    }

    #[test]
    fn builds_a_new_line_like_xtime() {
        let store = json!({ "DefaultLineColor": "Red", "Labor": { "InternalLinePriceDefaultValue": true } });
        let l = new_line(&input(), &pay(), &store, &codes()).unwrap();
        assert_eq!(l["ID"], json!(0));
        assert_eq!(l["Color"], json!("Yellow"));
        assert_eq!(l["PayTypeCode"]["Code"], json!("C")); // the store's default
        assert_eq!((l["Hours"].as_f64(), l["PriceLabor"].as_f64()), (Some(-1.0), Some(120.0))); // empty: the service's own
        assert_eq!(l["DispatchCode"], json!("T"));
        assert_eq!(l["EstimatedDuration"], json!(1.0));
        assert_eq!(l["OpCodeDms"], json!("BRK"));
        assert_eq!(l["EnableInternalLinePrice"], json!(true));
        let mut a = input();
        a.dispatch_code = "A".into();
        a.pay_type_id = Some(json!(3));
        let l = new_line(&a, &pay(), &store, &codes()).unwrap();
        assert_eq!(
            (l["DispatchCode"].clone(), l["EstimatedDuration"].clone()),
            (json!("A"), json!(0.5))
        );
        assert_eq!(l["PayTypeCode"]["Code"], json!("W"));
        assert!(new_line(&LineInput::default(), &pay(), &store, &codes()).is_err());
    }

    #[test]
    fn edits_a_line_keeping_the_rest() {
        let old = json!({ "ID": 77, "SGCID": 55, "SGID": 9, "Color": "Red", "PayTypeCode": { "ID": 2, "Code": "I" },
            "DispatchCode": "A", "EstimatedDuration": 0, "ServiceTypeFull": { "ServiceTypeCode": "MAINT" }, "Keep": "me" });
        let mut a = input();
        a.sgcid = Value::Null; // the service isn't changed
        a.color = String::new();
        let l = edited_line(&old, &a, &pay(), &codes()).unwrap();
        assert_eq!(l["Keep"], json!("me"));
        assert_eq!(l["Color"], json!("Red"));
        assert_eq!(l["PayTypeCode"]["Code"], json!("I")); // its own, as the store has it
        assert_eq!(l["EstimatedDuration"], json!(0.5)); // its dispatch code's
        assert_eq!(l["ServiceType"], json!("MAINT"));
        assert_eq!(l["ServiceGuideSpecialItemName"], json!("Front brakes"));
    }

    #[test]
    fn checks_what_s_filled_in() {
        assert_eq!(input().problem(), None);
        assert_eq!(
            LineInput {
                op_code_dms: "".into(),
                ..input()
            }
            .problem(),
            Some("Pick a CDK op code (type to search).")
        );
        assert_eq!(
            LineInput {
                duration: "x".into(),
                ..input()
            }
            .problem()
            .map(|p| p.starts_with("Estimated")),
            Some(true)
        );
        assert_eq!(
            LineInput {
                total: "$1,234.50".into(),
                ..input()
            }
            .problem(),
            None
        );
        let svc = CatService {
            name: "* Used Car  Check".into(),
            ..Default::default()
        };
        assert_eq!(svc.plain_name(), "Used Car Check");
    }

    #[test]
    fn reads_the_catalog() {
        let services = vec![
            json!({ "SGCID": 55, "SGID": 9, "ServiceName": "Brakes", "OpCode": "BRK", "Price": "149.95" }),
            json!({ "ServiceName": "x" }),
        ];
        let c = parse_catalog(&services, &pay(), &json!({}), &codes(), true);
        assert_eq!(c.services.len(), 1);
        assert_eq!(c.services[0].price, 149.95);
        assert_eq!(c.default_pay().map(|p| p.code.as_str()), Some("I")); // an internal RO
        assert_eq!(c.default_dispatch().map(|d| d.to_string()), Some("T · Tech".into()));
        assert_eq!(c.default_color, "Red");
    }
}
