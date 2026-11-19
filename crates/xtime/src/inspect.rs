//! An RO's inspection form, read and changed the way Xtime's RO page does it (the Electron app's `inspect.js`):
//! the form's categories and items (Green / Yellow / Red, a value, sub-items), the forms the store has and which one
//! this RO uses; saving an item (a Red one then goes to Xtime's HandleRedInspectionFormItem, as Xtime's page does
//! when the store uses "common ops"); switching the RO to another form. No customer information is read.
//!
//! Where the form comes from: the RO's details page names its scripts (InspectionFormData: the form, AvailableForms,
//! StoreData) and holds the RO's answers (InspectionItem_ROSpecific); …/InspectionItems has the latest answers.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{Value, json};
use workbench_net::Method;

use crate::employee::encode;
use crate::json::{get, text};
use crate::{Error, Result, Xtime, paths};

#[derive(Debug, Clone, Default, Serialize)]
pub struct InspectionForm {
    pub id: Value,
    pub name: String,
    /// The forms the store has (to switch the RO to another)
    pub forms: Vec<FormChoice>,
    pub categories: Vec<Category>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FormChoice {
    pub id: Value,
    pub name: String,
}

impl std::fmt::Display for FormChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Category {
    pub id: Value,
    pub name: String,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Item {
    /// Xtime's item ID (what's saved: IIID)
    pub id: Value,
    pub name: String,
    pub required: bool,
    /// Has a box for a value (a measurement, a note)
    pub show_value: bool,
    /// "Green", "Yellow", "Red", or "" / "Clear" for none (an item not answered yet shows its default colour)
    pub color: String,
    /// The value, or the sub-items picked ("Worn, Cracked")
    pub value: String,
    pub answered: bool,
    /// The services Xtime offers for it (their SGIDs: Add Service, on a Yellow or Red item)
    pub services: Vec<String>,
    /// Sub-items to pick: any colour, and those that set it Red / Yellow / Green
    pub sub_items: Vec<String>,
    pub sub_red: Vec<String>,
    pub sub_yellow: Vec<String>,
    pub sub_green: Vec<String>,
}

impl Item {
    /// The item's ID as text (for keeping what's being saved or typed)
    pub fn key(&self) -> String {
        text(Some(&self.id))
    }

    /// The value's parts ("Worn, Cracked" → Worn, Cracked)
    pub fn picked(&self) -> Vec<String> {
        split_list(&self.value)
    }
}

impl InspectionForm {
    pub fn item(&self, key: &str) -> Option<&Item> {
        self.categories.iter().flat_map(|c| &c.items).find(|i| i.key() == key)
    }
    pub fn item_mut(&mut self, key: &str) -> Option<&mut Item> {
        self.categories.iter_mut().flat_map(|c| &mut c.items).find(|i| i.key() == key)
    }

    /// Answered Red, answered Yellow, and required items still empty
    pub fn counts(&self) -> (usize, usize, usize) {
        let all: Vec<&Item> = self.categories.iter().flat_map(|c| &c.items).collect();
        let red = all.iter().filter(|i| i.answered && i.color == "Red").count();
        let yellow = all.iter().filter(|i| i.answered && i.color == "Yellow").count();
        let missing = all.iter().filter(|i| i.required && i.value.trim().is_empty()).count();
        (red, yellow, missing)
    }
}

fn split_list(s: &str) -> Vec<String> {
    s.split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect()
}

/// What pressing something on an item does
#[derive(Debug, Clone, PartialEq)]
pub enum Press {
    /// A colour (pressing the one it already has clears it)
    Color(String),
    /// A sub-item, with the colour it sets (its item's own colour for the plain ones)
    Sub(String, String),
    /// The value box, typed and kept
    Value(String),
    /// The value box's Clear
    ClearValue,
}

/// Xtime's saveItem, step for step: the colour and value the item gets, or nothing when there's nothing to save.
/// The same colour twice clears it; a sub-item adds to the value (once) and keeps the "worse" colour (Green < Yellow
/// < Red); Clear empties the value.
pub fn next_answer(item: &Item, press: &Press) -> Option<(String, String)> {
    match press {
        Press::Color(c) => {
            let color = if *c == item.color { "Clear".to_string() } else { c.clone() };
            Some((color, item.value.clone()))
        }
        Press::Sub(label, c) => {
            if item.picked().iter().any(|p| p == label) {
                return None; // picked already
            }
            let color = match (item.color.as_str(), c.as_str()) {
                ("Red", "Green" | "Yellow") => "Red".to_string(),
                ("Yellow", "Green") => "Yellow".to_string(),
                _ => c.clone(),
            };
            let value = if label.trim().is_empty() {
                item.value.clone()
            } else if item.value.trim().is_empty() {
                label.clone()
            } else {
                format!("{}, {label}", item.value)
            };
            Some((color, value))
        }
        Press::Value(v) => (*v != item.value).then(|| (item.color.clone(), v.clone())),
        Press::ClearValue => Some(("Clear".to_string(), String::new())),
    }
}

// ---------- reading Xtime's pages ----------

/// "var Name = {…};" (a script Xtime writes its data into) → the value
pub fn var_value(src: &str, name: &str) -> Option<Value> {
    let at = if name.is_empty() {
        0
    } else {
        let mut from = 0;
        loop {
            let i = src[from..].find(name)? + from;
            let before_ok = i == 0 || !src[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_');
            let rest = src[i + name.len()..].trim_start();
            if before_ok && rest.starts_with('=') && rest[1..].trim_start().starts_with(['{', '[']) {
                break i;
            }
            from = i + name.len();
        }
    };
    let start = src[at..].find(['{', '['])? + at;
    let bytes = src.as_bytes();
    let (mut depth, mut in_str, mut esc) = (0i32, false, false);
    for (i, &c) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'[' | b'{' => depth += 1,
            b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&src[start..=i]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

/// A script the RO's page loads, by part of its name: its address
pub fn script_src(html: &str, name: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find("<script") {
        let tag_start = from + i;
        let tag_end = lower[tag_start..].find('>').map(|e| tag_start + e)?;
        let tag = &html[tag_start..tag_end];
        if let Some(s) = tag.to_ascii_lowercase().find("src=") {
            let q = tag[s + 4..].chars().next()?;
            if q == '"' || q == '\'' {
                let rest = &tag[s + 5..];
                if let Some(end) = rest.find(q) {
                    let src = &rest[..end];
                    if has_name(src, name) {
                        return Some(src.replace("&amp;", "&"));
                    }
                }
            }
        }
        from = tag_end;
    }
    None
}

/// The address names this script (and not one whose name only starts the same: AvailableServices ≠ AvailableServicesX)
fn has_name(src: &str, name: &str) -> bool {
    let mut from = 0;
    while let Some(i) = src[from..].find(name) {
        let after = src[from + i + name.len()..].chars().next();
        if !after.is_some_and(|c| c.is_ascii_alphabetic()) {
            return true;
        }
        from += i + name.len();
    }
    false
}

/// `{ Data: x }` → x (Xtime wraps most answers)
pub fn unwrap_data(d: Value) -> Value {
    match d {
        Value::Object(mut m) if m.contains_key("Data") => m.remove("Data").unwrap_or(Value::Null),
        other => other,
    }
}

fn order(v: &Value) -> f64 {
    get(v, "DisplayOrder").and_then(Value::as_f64).unwrap_or(0.0)
}

fn active(v: &Value) -> bool {
    get(v, "Active") != Some(&Value::Bool(false))
}

/// The form from its script's data, the RO's answers, and the store's forms
pub fn parse_form(form: &Value, answers: &[Value], forms: &[Value]) -> Result<InspectionForm> {
    let cats = get(form, "Categories")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Format("Couldn't read the inspection form from Xtime".into()))?;
    let by_item: HashMap<String, &Value> = answers
        .iter()
        .map(|a| {
            let id = ["ItemID", "IIID", "ID"].iter().find_map(|k| get(a, k).filter(|v| !v.is_null()));
            (text(id), a)
        })
        .collect();
    let s = |v: &Value, k: &str| text(get(v, k));
    let mut cats: Vec<&Value> = cats.iter().filter(|c| c.is_object() && active(c)).collect();
    cats.sort_by(|a, b| order(a).total_cmp(&order(b)));
    let categories = cats
        .into_iter()
        .map(|c| {
            let mut items: Vec<&Value> = get(c, "Items")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter(|i| i.is_object() && active(i)).collect())
                .unwrap_or_default();
            items.sort_by(|a, b| order(a).total_cmp(&order(b)));
            Category {
                id: get(c, "ID").cloned().unwrap_or(Value::Null),
                name: s(c, "Name"),
                items: items
                    .into_iter()
                    .map(|i| {
                        let ans = by_item.get(&text(get(i, "ID")));
                        Item {
                            id: get(i, "ID").cloned().unwrap_or(Value::Null),
                            name: s(i, "Name"),
                            required: get(i, "Required") == Some(&Value::Bool(true)),
                            show_value: get(i, "ShowValueField") == Some(&Value::Bool(true)),
                            // as Xtime's page: an item not answered yet shows its default colour and no value
                            color: match ans {
                                Some(a) => s(a, "Color"),
                                None => s(i, "DefaultColor"),
                            },
                            value: ans.map(|a| s(a, "Value")).unwrap_or_default(),
                            answered: ans.is_some(),
                            services: get(i, "SelectedServices")
                                .and_then(Value::as_array)
                                .map(|a| a.iter().map(|s| text(Some(s))).filter(|s| !s.is_empty()).collect())
                                .unwrap_or_default(),
                            sub_items: split_list(&s(i, "SubItems")),
                            sub_red: split_list(&s(i, "SubItemsRed")),
                            sub_yellow: split_list(&s(i, "SubItemsYellow")),
                            sub_green: split_list(&s(i, "SubItemsGreen")),
                        }
                    })
                    .collect(),
            }
        })
        .filter(|c: &Category| !c.items.is_empty())
        .collect();
    Ok(InspectionForm {
        id: get(form, "ID").or_else(|| get(form, "IFID")).cloned().unwrap_or(Value::Null),
        name: s(form, "Name"),
        forms: forms
            .iter()
            .map(|f| FormChoice {
                id: get(f, "Key").cloned().unwrap_or(Value::Null),
                name: s(f, "Value"),
            })
            .collect(),
        categories,
    })
}

// ---------- asking Xtime ----------

/// What a Red item did on top of being saved
#[derive(Debug, Clone, PartialEq)]
pub enum RedService {
    /// Not a Red item, or the store doesn't add services for them
    None,
    /// Xtime added the item's service to the RO
    Added,
    /// Xtime didn't (why)
    NotAdded(String),
}

/// The RO's details page and the scripts it names
pub(crate) struct Details {
    pub html: String,
    pub form: Option<String>,
    pub forms: Option<String>,
    pub store: Option<String>,
    pub services: Option<String>,
    pub pay: Option<String>,
    pub dispatch: Option<String>,
}

impl Xtime {
    pub(crate) fn api(store: &str, ro: &str) -> String {
        format!("/api/Stores/{}/RepairOrders/{}", encode(store.trim()), encode(ro))
    }

    /// A page or script as text
    async fn get_text(&self, path: &str) -> Result<String> {
        let res = self.send(self.request(Method::GET, path)?).await?;
        if matches!(res.status.as_u16(), 401 | 403) || (res.redirected && res.url.path().to_ascii_lowercase().contains("login")) {
            return Err(Error::SessionExpired);
        }
        if !res.ok() {
            return Err(Error::Status(res.status.as_u16()));
        }
        Ok(res.body)
    }

    pub(crate) async fn details(&self, ro: &str) -> Result<Details> {
        let html = self.get_text(&paths::RO_DETAILS.replace("{RO}", &encode(ro))).await?;
        let src = |name: &str| script_src(&html, name);
        Ok(Details {
            form: src("InspectionFormData"),
            forms: src("AvailableForms"),
            store: src("StoreData"),
            services: src("AvailableServices"),
            pay: src("AvailablePayTypeCodes"),
            dispatch: src("DispatchCodes"),
            html,
        })
    }

    /// A script's data (`var Name = …`), kept once read (unless `fresh`)
    pub(crate) async fn script_var(&self, src: Option<&str>, name: &str, fresh: bool) -> Option<Value> {
        let src = src?;
        if !fresh {
            if let Some(v) = self.scripts.lock().expect("scripts lock").get(src) {
                return Some(v.clone());
            }
        }
        let text = self.get_text(src).await.ok()?;
        let v = var_value(&text, name)?;
        self.scripts.lock().expect("scripts lock").insert(src.to_string(), v.clone());
        Some(v)
    }

    /// The RO's inspection form, with its answers
    pub async fn inspection(&self, store: &str, ro: &str) -> Result<InspectionForm> {
        let d = self.details(ro).await?;
        if d.form.is_none() {
            return Err(Error::Format("Xtime's RO page didn't name its inspection form".into()));
        }
        // the form's own address changes with the form: read it fresh each time
        let form = self
            .script_var(d.form.as_deref(), "InspectionFormData", true)
            .await
            .ok_or_else(|| Error::Format("Couldn't read the inspection form from Xtime".into()))?;
        let forms = self.script_var(d.forms.as_deref(), "AvailableForms", false).await;
        // the RO's answers: the latest from Xtime, or the ones written into its page
        let answers = match self
            .ro_call(Method::GET, &format!("{}/InspectionItems", Self::api(store, ro)), None)
            .await
            .map(unwrap_data)
        {
            Ok(Value::Array(a)) => a,
            _ => var_value(&d.html, "InspectionItem_ROSpecific")
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default(),
        };
        let forms = forms.and_then(|f| f.as_array().cloned()).unwrap_or_default();
        parse_form(&form, &answers, &forms)
    }

    /// Saves an item's colour and value, as Xtime's saveItem; a Red one then asks Xtime to add its service (stores
    /// using "common ops", for ROs opened since then), as Xtime's page does
    pub async fn save_item(&self, store: &str, ro: &str, item_id: &Value, color: &str, value: &str) -> Result<RedService> {
        let saved = self
            .ro_call(
                Method::PUT,
                &format!("{}/InspectionItems/?format=json", Self::api(store, ro)),
                Some(json!({ "IIID": item_id, "Color": color, "Value": value })),
            )
            .await?;
        if color != "Red" {
            return Ok(RedService::None);
        }
        Ok(match self.red_service(store, ro, item_id, unwrap_data(saved)).await {
            Ok(r) => r,
            Err(e) => RedService::NotAdded(e.to_string()),
        })
    }

    async fn red_service(&self, store: &str, ro: &str, item_id: &Value, asr: Value) -> Result<RedService> {
        let d = self.details(ro).await?;
        let store_data = self.script_var(d.store.as_deref(), "StoreData", false).await.unwrap_or(Value::Null);
        let r = unwrap_data(
            self.ro_call(
                Method::GET,
                &format!("/api/Stores/{}/RepairOrder/{}", encode(store.trim()), encode(ro)),
                None,
            )
            .await?,
        );
        let since = get(&store_data, "IntegrationXtime")
            .and_then(|i| get(i, "CommonOpsEnableDate"))
            .and_then(xtime_date);
        let created = get(&r, "CreateDate").and_then(xtime_date);
        match (since, created) {
            (Some(s), Some(c)) if c >= s => {}
            _ => return Ok(RedService::None),
        }
        let vehicle = get(&r, "Vehicle").cloned().unwrap_or(Value::Null);
        let make = get(&vehicle, "MakeNormalized")
            .filter(|m| m.as_str().is_some_and(|s| !s.is_empty()))
            .or_else(|| get(&vehicle, "Make"))
            .cloned()
            .unwrap_or(Value::Null);
        let tech = get(&r, "Technician")
            .filter(|t| t.is_object())
            .and_then(|t| get(t, "ID"))
            .or_else(|| get(&r, "TechnicianID"))
            .cloned()
            .unwrap_or(Value::Null);
        let store_id = get(&store_data, "ID")
            .cloned()
            .unwrap_or_else(|| store.trim().parse::<i64>().map(Value::from).unwrap_or(Value::Null));
        let body = json!({
            "StoreId": store_id,
            "RoNumber": get(&r, "RONumber").cloned().unwrap_or(Value::Null),
            "IsInternal": get(&r, "IsInternal").cloned().unwrap_or(Value::Null),
            "IiId": item_id,
            "AsrIiId": asr,
            "Make": make,
            "Vin": get(&vehicle, "VIN").cloned().unwrap_or(Value::Null),
            "RoTechnician": tech,
        });
        match self
            .ro_call(Method::POST, "/api/ServiceStudio/HandleRedInspectionFormItem", Some(body))
            .await
        {
            Ok(_) => Ok(RedService::Added),
            Err(e) => Ok(RedService::NotAdded(e.to_string())),
        }
    }

    /// Switches the RO to another of the store's forms (Xtime's form drop-down)
    pub async fn switch_form(&self, store: &str, ro: &str, form_id: &Value) -> Result<()> {
        self.ro_call(
            Method::PUT,
            &format!("{}/InspectionForm/?format=json", Self::api(store, ro)),
            Some(json!({ "Data": { "ID": form_id } })),
        )
        .await?;
        self.scripts.lock().expect("scripts lock").clear(); // another form now
        Ok(())
    }
}

/// Xtime's dates: "/Date(1763190000000)/" or "2026-11-15T09:05:00" → something that sorts in time order
fn xtime_date(v: &Value) -> Option<String> {
    let s = v.as_str()?;
    if let Some(ms) = s.strip_prefix("/Date(") {
        let n: String = ms.chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
        let n: i64 = n.parse().ok()?;
        // as days and time from 1970, written so it sorts with the ISO ones below
        return Some(format!("@{n:020}"));
    }
    // ISO: compare the date and time part; "/Date" ones and ISO ones aren't mixed in one store
    (s.len() >= 10).then(|| s.chars().take(19).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(color: &str, value: &str) -> Item {
        Item {
            id: json!(5),
            color: color.into(),
            value: value.into(),
            ..Default::default()
        }
    }

    #[test]
    fn saves_like_xtime_does() {
        // the same colour twice clears it
        assert_eq!(
            next_answer(&item("Green", ""), &Press::Color("Green".into())),
            Some(("Clear".into(), "".into()))
        );
        assert_eq!(
            next_answer(&item("", "3mm"), &Press::Color("Red".into())),
            Some(("Red".into(), "3mm".into()))
        );
        // a sub-item adds to the value and keeps the worse colour
        assert_eq!(
            next_answer(&item("Red", "Worn"), &Press::Sub("Cracked".into(), "Yellow".into())),
            Some(("Red".into(), "Worn, Cracked".into()))
        );
        assert_eq!(
            next_answer(&item("Green", ""), &Press::Sub("Cracked".into(), "Yellow".into())),
            Some(("Yellow".into(), "Cracked".into()))
        );
        assert_eq!(next_answer(&item("Red", "Worn"), &Press::Sub("Worn".into(), "Red".into())), None); // once
        assert_eq!(next_answer(&item("Yellow", "4"), &Press::Value("4".into())), None); // unchanged
        assert_eq!(
            next_answer(&item("Yellow", "4"), &Press::Value("3".into())),
            Some(("Yellow".into(), "3".into()))
        );
        assert_eq!(
            next_answer(&item("Yellow", "4"), &Press::ClearValue),
            Some(("Clear".into(), "".into()))
        );
    }

    #[test]
    fn reads_scripts_and_their_data() {
        let html = r#"<script src="/Scripts/x.js"></script><script type="text/javascript" src='/JS/InspectionFormData?id=4&amp;v=2'></script>
            <script src="/JS/AvailableServicesSettings"></script><script src="/JS/AvailableServices?s=1"></script>
            <script>var InspectionItem_ROSpecific = [{"ItemID": 7, "Color": "Red", "Value": "2mm; \"thin\""}];</script>"#;
        assert_eq!(
            script_src(html, "InspectionFormData").as_deref(),
            Some("/JS/InspectionFormData?id=4&v=2")
        );
        assert_eq!(script_src(html, "AvailableServices").as_deref(), Some("/JS/AvailableServices?s=1"));
        assert_eq!(script_src(html, "StoreData"), None);
        let a = var_value(html, "InspectionItem_ROSpecific").unwrap();
        assert_eq!(a[0]["Value"], json!("2mm; \"thin\""));
        assert_eq!(
            var_value("var StoreDataX = {}; var StoreData = {\"ID\": 9};", "StoreData"),
            Some(json!({ "ID": 9 }))
        );
    }

    #[test]
    fn reads_the_form_with_its_answers() {
        let form = json!({ "ID": 3, "Name": "Ford MPI", "Categories": [
            { "ID": 2, "Name": "Brakes", "DisplayOrder": 2, "Items": [
                { "ID": 7, "Name": "Front pads", "Required": true, "ShowValueField": true, "DefaultColor": "Green",
                  "SubItemsRed": "Metal to metal", "SubItems": "Noise, Pulsation", "SelectedServices": [{ "x": 1 }], "DisplayOrder": 1 },
                { "ID": 8, "Name": "Old", "Active": false } ] },
            { "ID": 1, "Name": "Under hood", "DisplayOrder": 1, "Items": [ { "ID": 9, "Name": "Wipers", "DefaultColor": "Green" } ] },
            { "ID": 4, "Name": "Empty", "Items": [] } ] });
        let answers = vec![json!({ "ItemID": 7, "Color": "Red", "Value": "2mm" })];
        let forms = vec![json!({ "Key": 3, "Value": "Ford MPI" }), json!({ "Key": 5, "Value": "Express" })];
        let f = parse_form(&form, &answers, &forms).unwrap();
        assert_eq!(
            f.categories.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["Under hood", "Brakes"]
        );
        let pads = f.item("7").unwrap();
        assert_eq!((pads.color.as_str(), pads.value.as_str(), pads.answered), ("Red", "2mm", true));
        assert!(pads.required && pads.show_value && !pads.services.is_empty());
        assert_eq!(pads.sub_items, ["Noise", "Pulsation"]);
        assert_eq!(f.item("9").map(|i| (i.color.as_str(), i.answered)), Some(("Green", false)));
        assert!(f.item("8").is_none());
        assert_eq!(f.counts(), (1, 0, 0));
        assert_eq!(f.forms[1].to_string(), "Express");
    }
}
