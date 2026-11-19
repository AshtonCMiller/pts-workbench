//! The vehicles you have open, kept between starts: `workspaces.json` in the app's folder, the Electron app's own
//! file, read and written in its format. So the vehicles open in the Electron app open here after the move, and
//! going back to it (or a computer still on it) finds the same ones: a vehicle it had keeps everything it saved about
//! it (its tabs and procedures); one opened here is written so the Electron app sets up its usual tabs for it. This
//! app's own place on each (which tab) is added as `rs`, which the Electron app passes over. VINs stay in this file,
//! on this computer.
//!
//! Which ROs are open also goes in the hours (`openROs`: the RO number and year / make / model only, never the VIN),
//! the field the Electron app's sync carries between your computers (see `sync.js` there).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{Value, json};
use xtime::dashboard::AssignedRo;

use crate::workspace::{ManualTab, Tab, Workspace};

fn file() -> PathBuf {
    crate::settings::app_dir().join("workspaces.json")
}

/// A vehicle as it was saved
#[derive(Debug, Clone, PartialEq)]
pub struct SavedVehicle {
    pub ro: String,
    pub vin: String,
    /// "2023 Ford Bronco Sport"
    pub ymm: String,
    pub tab: Tab,
    pub manual: ManualTab,
    /// Labor Times: the tray and the last search (in the Electron app's Labor Times tab's state when it has one)
    pub labor: ford::labor::Saved,
}

impl SavedVehicle {
    /// The RO as much as it's known (the rest comes with your ROs from Xtime)
    pub fn assigned(&self) -> AssignedRo {
        let mut words = self.ymm.split_whitespace();
        let first = words.next().unwrap_or("");
        let (year, make) = if first.len() == 4 && first.chars().all(|c| c.is_ascii_digit()) {
            (first.to_string(), words.next().unwrap_or("").to_string())
        } else {
            (String::new(), first.to_string())
        };
        AssignedRo {
            ro: self.ro.clone(),
            tag: String::new(),
            year,
            make,
            model: words.collect::<Vec<_>>().join(" "),
            vin: self.vin.clone(),
            mode: None,
            waiting_parts: false,
            waiting_customer: false,
            waiting_warranty: false,
            dash: Default::default(),
        }
    }
}

/// What was open: the vehicles, and the one showing
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Saved {
    pub list: Vec<SavedVehicle>,
    pub active: Option<String>,
}

// The Electron app's names for the tabs
fn tab_kind(t: Tab) -> &'static str {
    match t {
        Tab::Maintenance => "maint",
        Tab::XtimeView => "xview",
        Tab::XtimeSite => "xtime",
        Tab::ServiceManual => "service",
        Tab::PtsSite => "pts",
    }
}

fn manual_kind(m: ManualTab) -> &'static str {
    match m {
        ManualTab::Oasis | ManualTab::Doc(_) => "oasisview",
        ManualTab::ConnectedVehicle => "cvview",
        ManualTab::Diagnostics => "diag",
        ManualTab::History => "vhist",
        ManualTab::Software => "vsw",
        ManualTab::Tsbs => "tsb",
        ManualTab::Workshop => "manual",
        ManualTab::Wiring => "wiring",
        ManualTab::Labor => "labor",
    }
}

fn manual_of(kind: &str) -> Option<ManualTab> {
    Some(match kind {
        "oasisview" | "oasis" => ManualTab::Oasis,
        "cvview" | "cvd" | "ota" => ManualTab::ConnectedVehicle,
        "diag" => ManualTab::Diagnostics,
        "vhist" => ManualTab::History,
        "vsw" => ManualTab::Software,
        "tsb" => ManualTab::Tsbs,
        "manual" | "pced" => ManualTab::Workshop,
        "wiring" => ManualTab::Wiring,
        "labor" => ManualTab::Labor,
        _ => return None,
    })
}

/// One entry of the file, read: this app's own place on it, or else where the Electron app was on it
fn read_entry(e: &Value) -> Option<SavedVehicle> {
    let s = |v: &Value| v.as_str().map(|x| x.trim().to_string()).unwrap_or_default();
    let ro = match &e["ro"] {
        Value::Number(n) => n.to_string(),
        v => s(v),
    };
    let vin = s(&e["vin"]).to_uppercase();
    if ro.is_empty() || vin.is_empty() {
        return None;
    }
    let (mut tab, mut manual) = (Tab::XtimeView, ManualTab::Oasis);
    if let Some(rs) = e.get("rs").filter(|v| v.is_object()) {
        tab = match s(&rs["tab"]).as_str() {
            "service" => Tab::ServiceManual,
            "xtime" => Tab::XtimeSite,
            "pts" => Tab::PtsSite,
            "maint" => Tab::Maintenance,
            _ => Tab::XtimeView,
        };
        manual = manual_of(&s(&rs["manual"])).unwrap_or(ManualTab::Oasis);
    } else if let Some(tabs) = e["tabs"].as_array() {
        // the Electron app's: the tab it showed, and the Service Manual's tab it was on
        let kind_at = |i: &Value| i.as_i64().and_then(|i| tabs.get(usize::try_from(i).ok()?)).map(|t| s(&t["kind"]));
        let active = kind_at(&e["activeIndex"]).unwrap_or_default();
        let svc = kind_at(&e["svcIndex"]);
        if let Some(m) = svc.as_deref().and_then(manual_of).or_else(|| manual_of(&active)) {
            manual = m;
        }
        if active == "service" || manual_of(&active).is_some() {
            tab = Tab::ServiceManual;
        } else if active == "xtime" {
            tab = Tab::XtimeSite;
        } else if active == "pts" {
            tab = Tab::PtsSite;
        }
    }
    Some(SavedVehicle {
        ro,
        vin,
        ymm: s(&e["ymm"]),
        tab,
        manual,
        labor: read_labor(e),
    })
}

/// The Electron app's Labor Times tab's state (both apps keep it there), else this app's own
fn read_labor(e: &Value) -> ford::labor::Saved {
    let tab = e["tabs"]
        .as_array()
        .and_then(|t| t.iter().find(|t| t["kind"].as_str() == Some("labor")))
        .map(|t| t["state"].clone());
    let state = tab.unwrap_or_else(|| e["rs"]["labor"].clone());
    serde_json::from_value(state).unwrap_or_default()
}

/// What was open last time (nothing when there's no file, or it can't be read)
pub fn load() -> Saved {
    let Some(root) = std::fs::read_to_string(file())
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        return Saved::default();
    };
    parse(&root)
}

fn parse(root: &Value) -> Saved {
    let mut list: Vec<SavedVehicle> = Vec::new();
    for e in root["list"].as_array().into_iter().flatten() {
        if let Some(v) = read_entry(e) {
            if !list.iter().any(|x| x.ro == v.ro) {
                list.push(v);
            }
        }
    }
    let active = root["activeRO"]
        .as_str()
        .map(str::to_string)
        .filter(|a| list.iter().any(|v| &v.ro == a));
    Saved { list, active }
}

/// The file with these vehicles open (`root`: the file as it is, kept as far as it goes)
fn update(mut root: Value, open: &[&Workspace], selected: Option<&str>) -> Value {
    if !root.is_object() {
        root = json!({});
    }
    let old: Vec<Value> = root["list"].as_array().cloned().unwrap_or_default();
    let list: Vec<Value> = open
        .iter()
        .map(|ws| {
            let mut e = old
                .iter()
                .find(|e| e["ro"].as_str() == Some(ws.ro.ro.as_str()) || e["ro"].as_i64().map(|n| n.to_string()) == Some(ws.ro.ro.clone()))
                .cloned()
                .filter(Value::is_object)
                // new here: no tabs, as from before the Electron app's present layout, so it sets up its usual ones
                .unwrap_or_else(|| json!({ "tabs": [], "activeIndex": 0, "svcIndex": -1, "layout": 2 }));
            e["ro"] = json!(ws.ro.ro);
            e["vin"] = json!(ws.ro.vin);
            let ymm = ws.vehicle();
            if !ymm.is_empty() {
                e["ymm"] = json!(ymm);
            }
            e["rs"] = json!({ "tab": tab_kind(ws.tab), "manual": manual_kind(ws.manual) });
            // Labor Times: in the Electron app's tab when it has one (so it's the same both ways), else here
            let labor = serde_json::to_value(&ws.labor.saved).unwrap_or(Value::Null);
            let electron = e["tabs"]
                .as_array_mut()
                .and_then(|t| t.iter_mut().find(|t| t["kind"].as_str() == Some("labor")))
                .filter(|t| t.is_object());
            match electron {
                Some(t) => t["state"] = labor,
                None => e["rs"]["labor"] = labor,
            }
            e
        })
        .collect();
    root["list"] = Value::Array(list);
    root["activeRO"] = selected.map_or(Value::Null, |s| json!(s));
    root
}

/// Keeps which vehicles are open, and the one showing (best effort: whole or not at all)
pub fn save(open: &[&Workspace], selected: Option<&str>) -> std::io::Result<()> {
    let f = file();
    let root = std::fs::read_to_string(&f)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .unwrap_or_else(|| json!({}));
    let next = update(root, open, selected);
    if let Some(dir) = f.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = f.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&next).map_err(std::io::Error::other)?)?;
    std::fs::rename(tmp, f)
}

/// The open ROs as the hours keep them for sync: RO number → year / make / model
pub fn open_ros(open: &[&Workspace]) -> BTreeMap<String, hours::OpenRo> {
    open.iter()
        .map(|ws| (ws.ro.ro.clone(), hours::OpenRo { ymm: ws.vehicle() }))
        .collect()
}

/// What's kept changes with: which vehicles, their VINs and tabs, and the one showing
pub fn snapshot(open: &[Workspace], selected: Option<&str>) -> String {
    let mut s = selected.unwrap_or("").to_string();
    for ws in open {
        s.push_str(&format!(
            "|{}:{}:{}:{}:{}:{}",
            ws.ro.ro,
            ws.ro.vin,
            ws.vehicle(),
            tab_kind(ws.tab),
            manual_kind(ws.manual),
            serde_json::to_string(&ws.labor.saved).unwrap_or_default()
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    // trimmed from an Electron app's workspaces.json
    const ELECTRON: &str = r#"{"activeRO":"820367","hidden":["1"],"xtimeName":"Ashton",
      "list":[{"ro":"820367","vin":"3fmcr9d97prd31969","ymm":"2023 Ford Bronco Sport",
        "tabs":[{"kind":"xtime","title":"Xtime"},{"kind":"xview","title":"Xtime View"},{"kind":"pts","title":"PTS"},
                {"kind":"oasisview","title":"OASIS"},{"kind":"tsb","title":"TSBs"},{"kind":"manual","title":"Workshop Manual","state":{"open":[1]}}],
        "activeIndex":5,"svcIndex":5,"layout":7},
        {"ro":820400,"vin":"1FTFW1E50PFA00000","ymm":"2023 Ford F-150","tabs":[{"kind":"xview"}],"activeIndex":0,"svcIndex":-1,"layout":7},
        {"ro":"","vin":"x"}]}"#;

    #[test]
    fn reads_the_electron_apps_vehicles() {
        let s = parse(&serde_json::from_str(ELECTRON).unwrap());
        assert_eq!(s.list.len(), 2);
        assert_eq!(s.active.as_deref(), Some("820367"));
        let v = &s.list[0];
        assert_eq!(
            (v.vin.as_str(), v.tab, v.manual),
            ("3FMCR9D97PRD31969", Tab::ServiceManual, ManualTab::Workshop)
        );
        let a = v.assigned();
        assert_eq!(
            (a.year.as_str(), a.make.as_str(), a.model.as_str()),
            ("2023", "Ford", "Bronco Sport")
        );
        assert_eq!((s.list[1].ro.as_str(), s.list[1].tab), ("820400", Tab::XtimeView));
    }

    #[test]
    fn writes_them_back_keeping_the_electron_apps_part() {
        let root: Value = serde_json::from_str(ELECTRON).unwrap();
        let saved = parse(&root);
        let mut open: Vec<Workspace> = saved.list.iter().map(|v| Workspace::new(v.assigned())).collect();
        open[0].tab = Tab::ServiceManual;
        open[0].manual = ManualTab::Tsbs;
        open.remove(1); // closed
        let mut extra = saved.list[0].clone();
        extra.ro = "830000".into();
        extra.ymm = "2024 Ford Maverick".into();
        open.push(Workspace::new(extra.assigned())); // opened here
        let refs: Vec<&Workspace> = open.iter().collect();
        let out = update(root, &refs, Some("830000"));
        assert_eq!(out["activeRO"], "830000");
        assert_eq!(out["hidden"], json!(["1"])); // kept
        assert_eq!(out["xtimeName"], "Ashton");
        let list = out["list"].as_array().unwrap();
        assert_eq!(list.len(), 2);
        // the Electron app's tabs (and procedures) are kept; this app's place is added
        assert_eq!(list[0]["tabs"].as_array().unwrap().len(), 6);
        assert_eq!(list[0]["tabs"][5]["state"], json!({"open": [1]}));
        assert_eq!(list[0]["rs"], json!({"tab": "service", "manual": "tsb"}));
        // new here: set up by the Electron app as from an older layout
        assert_eq!(list[1]["layout"], 2);
        assert_eq!(list[1]["ymm"], "2024 Ford Maverick");
        // and read back as it was left
        let again = parse(&out);
        assert_eq!((again.list[0].tab, again.list[0].manual), (Tab::ServiceManual, ManualTab::Tsbs));
        assert_eq!(again.active.as_deref(), Some("830000"));
    }
}
