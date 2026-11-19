//! Connected Vehicle: what the vehicle reports to Ford through its modem, asked the way PTS's Connected Vehicle page
//! and OASIS ask (the Electron app's `manual/connected-vehicle/` and `loadConnectedVehicle`):
//!
//!   the status         [`Ford::connected_vehicle`]: `POST /ConnectedVehicle/getCVGroupApiData`, built from the values
//!                      OASIS writes into its page ([`CvVars`], read with OASIS): odometer, fuel, oil life, 12V battery,
//!                      tire pressures, the DTCs and alerts it's reporting now, and its last 60 days or so of reports
//!   older reports      [`Ford::cv_history`] (60 days at a time, as Ford's page asks)
//!   module names       [`Ford::cv_modules`] (726 → BCM)
//!   what DTCs mean     [`Ford::cv_describe`]: their titles, failure types and status bytes
//!   TSBs by DTC        [`Ford::cv_articles`]: the TSBs / GSBs / SSMs Ford lists next to each DTC
//!   over-the-air       [`Ford::cv_ota`]: the updates sent to the vehicle (with release notes), what's lined up next
//!   subscriptions      [`Ford::cv_subscriptions`]: connectivity, BlueCruise…, what's active and what can be started
//!
//! Each request after the status needs what the status worked out ([`CvContext`]). Ford sends distances in km and
//! tire pressures in kPa; they're shown in miles and psi in the USA. Nothing here is kept on disk, and the log notes
//! the fields Ford sent, never their values.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde_json::{Value, json};
use workbench_net::{Http, Method};

use crate::vehicle::enc;
use crate::{Error, Ford, Result};

pub const KM_PER_MILE: f64 = 1.60934;
const PSI_PER_KPA: f64 = 0.145038;

/// A time Ford reported (UTC)
pub type When = Option<DateTime<Utc>>;

// =====================================================================================================================
// What OASIS writes into its page for the connected vehicle request
// =====================================================================================================================

/// The values OASIS's page sets for its connected vehicle request (`var cvPACode = '…'` and so on)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CvVars {
    pub country: String,
    pub vin: String,
    pub pa_code: String,
    pub flavor: String,
    pub master_id: String,
    pub lang: String,
    pub start_date: String,
    pub end_date: String,
    pub prog_start: String,
    pub region: String,
    /// "G" (gas), "D", "E"…
    pub fuel_type: String,
    /// The SYNC modem OASIS lists
    pub modem: String,
}

/// `var name = "value"` (or 'value') in a page's scripts
pub fn page_var(page: &str, name: &str) -> String {
    let mut rest = page;
    while let Some(i) = rest.find("var") {
        let after = &rest[i + 3..];
        rest = after;
        // "var" on its own, then the name, then = and a quoted value
        if !after.starts_with(char::is_whitespace) {
            continue;
        }
        let t = after.trim_start();
        let Some(t) = t.strip_prefix(name) else { continue };
        let t = t.trim_start();
        let Some(t) = t.strip_prefix('=') else { continue };
        let t = t.trim_start();
        let Some(q) = t.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            continue;
        };
        if let Some(end) = t[1..].find(q) {
            return t[1..1 + end].trim().to_string();
        }
    }
    String::new()
}

/// The value a page's script stores: `localStorage.setItem('CVFuelType', 'G')`
fn set_item(page: &str, key: &str) -> String {
    for quote in ['\'', '"'] {
        let needle = format!("setItem({quote}{key}{quote}");
        let Some(i) = page.find(&needle).or_else(|| page.find(&format!("setItem( {quote}{key}{quote}"))) else {
            continue;
        };
        let after = &page[i + needle.len()..];
        let Some(comma) = after.find(',') else { continue };
        let t = after[comma + 1..].trim_start();
        if let Some(q) = t.chars().next().filter(|c| *c == '"' || *c == '\'') {
            if let Some(end) = t[1..].find(q) {
                return t[1..1 + end].trim().to_string();
            }
        }
    }
    String::new()
}

/// Reads the values from OASIS's page (`modem`: the SYNC modem from its vehicle information)
pub fn read_vars(page: &str, modem: &str) -> CvVars {
    CvVars {
        country: page_var(page, "countryCode"),
        vin: page_var(page, "currentVin"),
        pa_code: page_var(page, "cvPACode"),
        flavor: page_var(page, "flavor"),
        master_id: page_var(page, "masterID"),
        lang: page_var(page, "langISO"),
        start_date: page_var(page, "startDate"),
        end_date: page_var(page, "endDate"),
        prog_start: page_var(page, "ProgStartDate"),
        region: page_var(page, "cvRegion"),
        fuel_type: set_item(page, "CVFuelType"),
        modem: modem.trim().to_string(),
    }
}

// =====================================================================================================================
// Reading Ford's values
// =====================================================================================================================

/// Text of a value (a string or a number)
fn s(v: &Value) -> String {
    match v {
        Value::String(t) => t.trim().to_string(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// A number: Ford's `{ value, updateTime }`, or a plain one (-1000: none)
pub fn num(v: &Value) -> Option<f64> {
    let v = if v.is_object() { &v["value"] } else { v };
    let n = match v {
        Value::Number(n) => n.as_f64(),
        Value::String(t) if !t.trim().is_empty() => t.trim().parse().ok(),
        _ => None,
    }?;
    (n != -1000.0 && n.is_finite()).then_some(n)
}

fn pct(v: &Value) -> Option<String> {
    num(v).filter(|n| (0.0..=100.0).contains(n)).map(|n| format!("{}%", n.floor()))
}

/// Ford's times: "2026-03-02 14:05:00" (UTC), ISO, or seconds / milliseconds since 1970
pub fn parse_time(v: &Value) -> When {
    let t = match v {
        Value::Number(n) => return n.as_f64().and_then(epoch),
        Value::String(t) => t.trim(),
        _ => return None,
    };
    if t.is_empty() {
        return None;
    }
    if t.len() >= 9 && t.len() <= 13 && t.bytes().all(|b| b.is_ascii_digit()) {
        return t.parse().ok().and_then(epoch);
    }
    if let Ok(d) = DateTime::parse_from_rfc3339(t) {
        return Some(d.with_timezone(&Utc));
    }
    let plain = t.replacen(' ', "T", 1);
    let plain = plain.trim_end_matches('Z');
    for f in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M"] {
        if let Ok(d) = NaiveDateTime::parse_from_str(plain, f) {
            return Some(Utc.from_utc_datetime(&d));
        }
    }
    // with an offset Ford wrote without the colon ("+0000")
    if let Ok(d) = DateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S%.f%z") {
        return Some(d.with_timezone(&Utc));
    }
    for f in ["%Y-%m-%d", "%m/%d/%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(t, f) {
            return d.and_hms_opt(0, 0, 0).map(|d| Utc.from_utc_datetime(&d));
        }
    }
    None
}

fn epoch(n: f64) -> When {
    let ms = if n < 1e11 { n * 1000.0 } else { n };
    Utc.timestamp_millis_opt(ms as i64).single()
}

/// LOW_PRESSURE → Low pressure (other wording kept)
pub fn words(v: &str) -> String {
    let t = v.trim();
    let caps = t.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        && t.as_bytes()
            .windows(2)
            .any(|w| w[0].is_ascii_uppercase() && w[1].is_ascii_uppercase());
    if !caps {
        return t.to_string();
    }
    let low = t.to_lowercase().replace('_', " ");
    let mut c = low.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

fn yes(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Null => None,
        _ => {
            let t = s(v).to_ascii_lowercase();
            if t.is_empty() {
                None
            } else {
                Some(matches!(t.as_str(), "y" | "yes" | "true" | "on"))
            }
        }
    }
}

/// A distance (Ford's km) as the vehicle's country shows it: "48,213 mi" or "77,592 km"
pub fn dist(km: f64, miles: bool) -> String {
    if miles {
        format!("{} mi", crate::oasis::thousands((km / KM_PER_MILE).floor() as i64))
    } else {
        format!("{} km", crate::oasis::thousands(km.floor() as i64))
    }
}

fn list(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}

// =====================================================================================================================
// The status
// =====================================================================================================================

/// What the requests after the status need (the same values Ford's own page keeps)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CvContext {
    pub vin: String,
    pub country: String,
    pub pa_code: String,
    pub lang: String,
    pub user_country: String,
    pub user_state: String,
}

impl CvContext {
    /// Distances in miles (the USA)
    pub fn miles(&self) -> bool {
        self.country.is_empty() || self.country == "USA"
    }
}

/// One label / value line ("Oil life", "62%"), and when the vehicle reported it
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reading {
    pub label: String,
    pub value: Option<String>,
    pub when: When,
    /// Worth a look (automatic updates off…)
    pub warn: bool,
}

fn reading(label: &str, value: Option<String>, when: When) -> Reading {
    Reading {
        label: label.into(),
        value: value.filter(|v| !v.is_empty()),
        when,
        warn: false,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tire {
    pub label: String,
    /// psi in the USA, else kPa
    pub value: Option<i64>,
    pub low: bool,
    /// Ford's word for it ("Normal", "Low pressure")
    pub status: String,
    pub when: When,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tires {
    /// "psi" or "kPa"
    pub unit: String,
    pub front: Vec<Tire>,
    /// The rear tires (the inner ones too, on a dually)
    pub rear: Vec<Tire>,
    pub front_recommended: Option<i64>,
    pub rear_recommended: Option<i64>,
    /// The system's state, when it isn't normal
    pub system: Option<String>,
}

/// One DTC the vehicle reported
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dtc {
    /// "U2100"
    pub code: String,
    /// Its failure type ("00")
    pub ft: String,
    /// Its status byte ("2F")
    pub sb: String,
    /// The module's address ("726"), as Ford sent it
    pub ecu: String,
    /// The module's name, when Ford sent it with the DTC
    pub ecu_name: String,
    /// What it means, when Ford sent it with the DTC
    pub desc: String,
}

impl Dtc {
    /// What Diagnostics searches: "U2100:00"
    pub fn key(&self) -> String {
        if self.ft.is_empty() {
            self.code.clone()
        } else {
            format!("{}:{}", self.code, self.ft)
        }
    }

    /// "U2100:00:2F"
    pub fn full(&self) -> String {
        [self.code.as_str(), self.ft.as_str(), self.sb.as_str()]
            .into_iter()
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>()
            .join(":")
    }
}

fn read_dtc(x: &Value) -> Option<Dtc> {
    let code = s(&x["dtcCode"]).to_uppercase();
    if code.is_empty() {
        return None;
    }
    let first = |keys: &[&str]| keys.iter().map(|k| s(&x[*k])).find(|v| !v.is_empty()).unwrap_or_default();
    Some(Dtc {
        code,
        ft: s(&x["failureType"]).to_uppercase(),
        sb: first(&["dtcsStatus", "statusByte", "dtcStatus", "status"]).to_uppercase(),
        ecu: first(&["ecuid", "ecuId"]).to_uppercase(),
        ecu_name: s(&x["ecuName"]),
        desc: first(&["dtcTitle", "dtcDescription", "description"]),
    })
}

/// A health alert the vehicle sent (a warning light, a message…)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Alert {
    /// "New alert", "Clear alert"
    pub kind: String,
    pub what: String,
    /// What the customer's told to do
    pub action: String,
    pub clear: bool,
    pub urgent: bool,
}

fn read_alert(a: &Value) -> Alert {
    let t = s(&a["alertType"]);
    let low = t.to_ascii_lowercase();
    let what = ["messageDesc", "alertName", "title"]
        .iter()
        .map(|k| s(&a[*k]))
        .find(|v| !v.is_empty())
        .or_else(|| Some(words(&s(&a["symptomKey"]))).filter(|v| !v.is_empty()))
        .unwrap_or_else(|| s(&a["wilCode"]));
    let urgency = s(&a["urgency"]).to_ascii_lowercase();
    Alert {
        kind: if low.contains("clear") {
            "Clear alert".into()
        } else if low.contains("new") {
            "New alert".into()
        } else if t.is_empty() {
            "Alert".into()
        } else {
            words(&t)
        },
        what,
        action: words(&s(&a["customerActionKey"])),
        clear: low.contains("clear"),
        urgent: ["high", "immediate", "critical"].iter().any(|w| urgency.contains(w)),
    }
}

/// One report the vehicle sent: its DTCs and alerts, odometer and 12V battery
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub when: When,
    pub odometer_km: Option<f64>,
    pub volts: Option<f64>,
    /// The 12V battery's charge (%)
    pub soc: Option<f64>,
    /// Its software changed (an over-the-air update)
    pub ota_change: bool,
    pub dtcs: Vec<Dtc>,
    pub alerts: Vec<Alert>,
}

impl Report {
    /// Has DTCs or alerts
    pub fn notable(&self) -> bool {
        !self.dtcs.is_empty() || !self.alerts.is_empty()
    }
}

/// The health history Ford sends: `vhDataList[].vhDataOnEventTime[]`, newest first
pub fn read_history(list_: &Value) -> Vec<Report> {
    let mut out: Vec<Report> = Vec::new();
    for day in list(list_) {
        for ev in list(&day["vhDataOnEventTime"]) {
            let when = parse_time(&ev["eventTime"]).or_else(|| parse_time(&day["eventDate"]));
            let ota = &ev["otaChange"];
            out.push(Report {
                when,
                odometer_km: num(&ev["odometer"]),
                volts: num(&ev["batteryVoltage"]),
                soc: num(&ev["batterySoc"]),
                ota_change: yes(ota).unwrap_or(!ota.is_null() && ota.as_array().map_or(true, |a| !a.is_empty())),
                dtcs: list(&ev["dtcs"]).iter().filter_map(read_dtc).collect(),
                alerts: list(&ev["alerts"]).iter().filter(|a| !a.is_null()).map(read_alert).collect(),
            });
        }
    }
    sort_reports(&mut out);
    out
}

/// Newest first
pub fn sort_reports(list: &mut [Report]) {
    list.sort_by(|a, b| b.when.cmp(&a.when));
}

/// Adds reports not had yet (by their time and odometer); newest first after
pub fn merge_reports(into: &mut Vec<Report>, more: Vec<Report>) {
    for r in more {
        if !into.iter().any(|x| x.when == r.when && x.odometer_km == r.odometer_km) {
            into.push(r);
        }
    }
    sort_reports(into);
}

/// What the status request answered
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CvStatus {
    pub ctx: CvContext,
    /// Why there may be little or nothing (the dealership's terms, the customer's privacy settings…)
    pub notes: Vec<String>,
    /// Odometer, fuel, oil life, 12V battery (and high voltage battery, DEF when it has them)
    pub readings: Vec<Reading>,
    /// The newest of those reports
    pub latest: When,
    pub tires: Option<Tires>,
    /// The DTCs and alerts it's reporting now
    pub now: Option<Report>,
    /// The reports Ford sent with the status (the last 60 days or so), and from when
    pub history: Vec<Report>,
    pub history_from: Option<NaiveDate>,
    pub odometer_km: Option<f64>,
    pub odometer_when: When,
    /// Every value Ford sent in the status report (in case something above is missing)
    pub everything: Vec<Reading>,
    /// The vehicle's own over-the-air settings
    pub ota_settings: Vec<Reading>,
}

impl CvStatus {
    pub fn miles(&self) -> bool {
        self.ctx.miles()
    }

    /// The odometer in miles (for warranty coverage)
    pub fn odometer_miles(&self) -> Option<f64> {
        self.odometer_km.map(|km| (km / KM_PER_MILE).floor())
    }

    /// Ford had something to show
    pub fn has_data(&self) -> bool {
        !self.readings.is_empty() || self.now.is_some() || self.tires.is_some()
    }
}

fn tires(m: &Value, vs: &Value, miles: bool) -> Option<Tires> {
    let t = if vs["TPMS"].is_object() { &vs["TPMS"] } else { &m["TPMS"] };
    let conv = |kpa: Option<f64>| -> Option<i64> {
        let k = kpa.filter(|k| *k >= 0.0)?;
        Some(if miles {
            (if k > 80.0 { k * PSI_PER_KPA } else { k }).round() as i64
        } else {
            k.round() as i64
        })
    };
    const SPOTS: [(&str, &str, &str); 6] = [
        ("Left front", "leftFrontTire", "FRONT_LEFT"),
        ("Right front", "rightFrontTire", "FRONT_RIGHT"),
        ("Left rear", "outerLeftRearTire", "REAR_LEFT"),
        ("Right rear", "outerRightRearTire", "REAR_RIGHT"),
        ("Inner left rear", "innerLeftRearTire", "INNER_REAR_LEFT"),
        ("Inner right rear", "innerRightRearTire", "INNER_REAR_RIGHT"),
    ];
    let from_list = list(&m["tirePressure"]);
    let all: Vec<Tire> = SPOTS
        .iter()
        .enumerate()
        .map(|(i, (label, field, wheel))| {
            let listed = from_list.iter().find(|x| {
                let w = s(&x["vehicleWheel"]);
                let w = if w.is_empty() { s(&x["wheel"]) } else { w };
                w.eq_ignore_ascii_case(wheel)
            });
            let p = &t[format!("{field}Pressure")];
            let kpa = num(p).or_else(|| listed.and_then(num));
            let status = s(&t[format!("{field}Status")]["value"]);
            let when = parse_time(&p["timestamp"]).or_else(|| listed.and_then(|l| parse_time(&l["updateTime"])));
            (
                i,
                Tire {
                    label: label.to_string(),
                    value: conv(kpa),
                    low: status.to_ascii_lowercase().contains("low"),
                    status: words(&status),
                    when,
                },
            )
        })
        // the inner rear tires only on duallys
        .filter(|(i, x)| *i < 4 || x.value.is_some())
        .map(|(_, x)| x)
        .collect();
    if !all.iter().any(|x| x.value.is_some()) {
        return None;
    }
    let mut sys = s(&t["tirePressureSystemStatus"]["value"]);
    if sys.is_empty() {
        sys = s(&m["tirePressureSystemStatus"]["value"]);
    }
    if sys.is_empty() {
        sys = s(&m["tirePressureSystemStatus"][0]["value"]);
    }
    let low = sys.to_ascii_lowercase();
    let mut all = all;
    let rear = all.split_off(2);
    Some(Tires {
        unit: if miles { "psi" } else { "kPa" }.into(),
        front: all,
        rear,
        front_recommended: conv(num(&t["recommendedFrontTirePressure"])),
        rear_recommended: conv(num(&t["recommendedRearTirePressure"])),
        system: (!sys.is_empty() && !low.contains("normal") && low != "ok").then(|| words(&sys)),
    })
}

/// "batteryStateOfCharge" → "Battery state of charge"
fn label_of(k: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for c in k.chars() {
        if (c.is_ascii_uppercase() || c.is_ascii_digit()) && prev_lower {
            out.push(' ');
        }
        prev_lower = c.is_ascii_lowercase();
        out.push(if out.is_empty() {
            c.to_ascii_uppercase()
        } else {
            c.to_ascii_lowercase()
        });
    }
    out
}

/// Reads Ford's answer to the status request
pub fn read_status(json: &Value, ctx: CvContext) -> CvStatus {
    let miles = ctx.miles();
    let mut st = CvStatus { ctx, ..Default::default() };
    let cap = &json["capabilityData"];
    let no = |v: &Value| !s(v).is_empty() && !s(v).eq_ignore_ascii_case("yes");
    if no(&cap["dealerAgreed"]) {
        st.notes
            .push("Your dealership hasn’t accepted the connected vehicle terms in PTS.".into());
    }
    if no(&cap["privacyResponse"]["auth"]) {
        st.notes
            .push("The customer's privacy settings don't allow connected vehicle data.".into());
    }
    if no(&cap["cvasPassed"]) {
        st.notes
            .push("This vehicle isn’t authorized to send data (modem activation or Ford app settings).".into());
    }
    let data = &json["connectedVehicleData"];
    let m = &data["tmcData"]["metrics"];
    let vs = &data["vstateData"]["vehiclestatus"];
    let when = |x: &Value| parse_time(&x["updateTime"]).or_else(|| parse_time(&x["timestamp"]));
    if m.is_object() || vs.is_object() {
        for x in [&m["odometer"], &m["fuelLevel"], &m["batteryStateOfCharge"], &m["oilLifeRemaining"]] {
            let d = when(x);
            if d > st.latest {
                st.latest = d;
            }
        }
        st.odometer_km = num(&m["odometer"]).or_else(|| num(&vs["odometer"]));
        st.odometer_when = when(&m["odometer"]).or_else(|| when(&vs["odometer"]));
        let fuel = pct(&m["fuelLevel"]).or_else(|| pct(&vs["fuel"]["fuelLevel"]));
        let range = num(&m["fuelRange"])
            .or_else(|| num(&vs["fuel"]["distanceToEmpty"]))
            .map(|k| dist(k, miles));
        let volts = num(&m["batteryVoltage"]).or_else(|| num(&vs["battery"]["batteryStatusActual"]));
        let join = |a: Option<String>, b: Option<String>| {
            let v: Vec<String> = [a, b].into_iter().flatten().collect();
            (!v.is_empty()).then(|| v.join(" · "))
        };
        st.readings = vec![
            reading("Odometer", st.odometer_km.map(|k| dist(k, miles)), st.odometer_when),
            reading("Fuel", join(fuel, range.map(|r| format!("{r} range"))), when(&m["fuelLevel"])),
            reading(
                "Oil life",
                pct(&m["oilLifeRemaining"]).or_else(|| pct(&vs["oil"]["oilLifeActual"])),
                when(&m["oilLifeRemaining"]),
            ),
            reading(
                "12V battery",
                join(pct(&m["batteryStateOfCharge"]), volts.map(|v| format!("{v:.1} V"))),
                when(&m["batteryStateOfCharge"]),
            ),
        ];
        if !m["xevBatteryStateOfCharge"].is_null() {
            let x = &m["xevBatteryStateOfCharge"];
            st.readings.push(reading("High voltage battery", pct(x), when(x)));
        }
        if !m["xevBatteryRange"].is_null() {
            let x = &m["xevBatteryRange"];
            st.readings.push(reading("Electric range", num(x).map(|k| dist(k, miles)), when(x)));
        }
        if !m["dieselExhaustFluidLevel"].is_null() {
            let x = &m["dieselExhaustFluidLevel"];
            st.readings.push(reading("Diesel exhaust fluid", pct(x), when(x)));
        }
        st.tires = tires(m, vs, miles);
    }
    // DTCs and alerts in the latest report
    let diag = &data["diagCurrentData"];
    if diag.is_object() {
        let dtcs = if diag["Metadata"]["dtcs"].is_array() {
            &diag["Metadata"]["dtcs"]
        } else {
            &diag["dtcs"]
        };
        st.now = Some(Report {
            when: parse_time(&diag["eventTime"]),
            dtcs: list(dtcs).iter().filter_map(read_dtc).collect(),
            alerts: list(&diag["vhAlerts"])
                .iter()
                .flat_map(|g| list(&g["alerts"]).iter())
                .filter(|a| !a.is_null())
                .map(read_alert)
                .collect(),
            ..Default::default()
        });
    }
    // the reports Ford sent with it
    let h = &data["diagHistoryData"];
    st.history = read_history(&h["vhDataList"]);
    if !st.history.is_empty() {
        st.history_from = parse_time(&h["fromDate"])
            .map(|d| d.date_naive())
            .or_else(|| Some(Utc::now().date_naive() - chrono::Duration::days(60)));
    }
    // everything in the status report
    if let Some(o) = m.as_object() {
        for (k, x) in o {
            if !x.is_object() || x.get("value").is_none() {
                continue;
            }
            let v = &x["value"];
            let value = match v {
                Value::Object(_) | Value::Array(_) => v.to_string(),
                _ => words(&s(v)),
            };
            st.everything.push(reading(&label_of(k), Some(value), when(x)));
        }
    }
    // the vehicle's own over-the-air settings
    let cfg = &m["configurations"];
    let auto = &cfg["automaticSoftwareUpdateOptInSetting"];
    let sched = &cfg["automaticSoftwareUpdateScheduleSetting"];
    if auto.is_object() {
        let v = &auto["value"];
        let on = match v {
            Value::Bool(b) => Some(*b),
            Value::Null => None,
            _ => {
                let t = s(v).to_ascii_lowercase().replace('_', "");
                Some(matches!(t.as_str(), "on" | "true" | "yes" | "optin" | "enabled" | "enable"))
            }
        };
        let mut r = reading(
            "Automatic software updates",
            on.map(|o| if o { "On" } else { "Off" }.to_string()),
            when(auto),
        );
        r.warn = on == Some(false);
        st.ota_settings.push(r);
    }
    if sched.is_object() {
        let kind = s(&sched["value"]["scheduleType"]).to_ascii_lowercase();
        st.ota_settings.push(reading(
            "Recurring schedule set",
            Some(if kind.contains("recurring") { "Yes" } else { "No" }.into()),
            when(sched),
        ));
    }
    st
}

// =====================================================================================================================
// Modules, DTC descriptions, TSBs by DTC
// =====================================================================================================================

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Module {
    /// "BCM"
    pub name: String,
    /// "Body Control Module"
    pub desc: String,
}

/// Module names by address ("726" → BCM)
pub type Modules = HashMap<String, Module>;

pub fn read_modules(json: &Value) -> Modules {
    let mut out = Modules::new();
    for m in list(&json["moduleDetails"]) {
        let addr = s(&m["moduleAddr"]).to_uppercase();
        let addr: String = addr.chars().skip(3).take(3).collect();
        let name = s(&m["moduleAcronym"]);
        if addr.is_empty() || name.is_empty() {
            continue;
        }
        let desc = Some(s(&m["moduleDesc"]))
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| s(&m["moduleDescription"]));
        out.insert(addr, Module { name, desc });
    }
    out
}

/// A DTC's module: its name from Ford's list (by its address, or the address's last three), else what came with it
pub fn module_of(modules: &Modules, d: &Dtc) -> (String, String) {
    let id = d.ecu.to_uppercase();
    let tail: String = id.chars().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect();
    match modules.get(&id).or_else(|| modules.get(&tail)) {
        Some(m) => (m.name.clone(), m.desc.clone()),
        None if !d.ecu_name.is_empty() => (d.ecu_name.clone(), String::new()),
        None => (id.clone(), if id.is_empty() { String::new() } else { format!("Module {id}") }),
    }
}

/// What DTCs mean: their titles (by code), failure types and status bytes
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Descriptions {
    pub titles: HashMap<String, String>,
    pub ft: HashMap<String, String>,
    pub sb: HashMap<String, String>,
}

impl Descriptions {
    pub fn merge(&mut self, more: Descriptions) {
        self.titles.extend(more.titles);
        self.ft.extend(more.ft);
        self.sb.extend(more.sb);
    }

    /// What a DTC means: what came with it, else Ford's title
    pub fn title(&self, d: &Dtc) -> String {
        if !d.desc.is_empty() {
            return d.desc.clone();
        }
        self.titles.get(&d.code).cloned().unwrap_or_default()
    }

    /// What its failure type and status byte mean (for its tooltip)
    pub fn detail(&self, d: &Dtc) -> String {
        let mut out = Vec::new();
        if !d.ft.is_empty() {
            match self.ft.get(&d.ft).filter(|t| !t.is_empty()) {
                Some(t) => out.push(format!("Failure type {}: {t}", d.ft)),
                None => out.push(format!("Failure type {}", d.ft)),
            }
        }
        if !d.sb.is_empty() {
            match self.sb.get(&d.sb).filter(|t| !t.is_empty()) {
                Some(t) => out.push(format!("Status byte {}: {t}", d.sb)),
                None => out.push(format!("Status byte {}", d.sb)),
            }
        }
        out.join("\n")
    }
}

pub fn read_descriptions(json: &Value, asked: &[Dtc]) -> Descriptions {
    let mut d = Descriptions::default();
    for t in list(&json["dtsGGDSRes"]["lstDTCData"]) {
        let (code, title) = (s(&t["dtcCode"]).to_uppercase(), s(&t["dtcTitle"]));
        if !code.is_empty() && !title.is_empty() {
            d.titles.insert(code, title);
        }
    }
    // none from Ford: not asked again
    for a in asked {
        d.titles.entry(a.code.clone()).or_default();
    }
    for f in list(&json["failureTypeRes"]["lstDTCFTInfo"]) {
        let c = s(&f["strFailureTypeCode"]).to_uppercase();
        if !c.is_empty() {
            d.ft.insert(c, s(&f["strFailureTypeDesc"]));
        }
    }
    for b in list(&json["statusByteRes"]["lstDTCSBInfo"]) {
        let c = s(&b["strStatusByteCode"]).to_uppercase();
        if !c.is_empty() {
            d.sb.insert(c, s(&b["strStatusByteDesc"]));
        }
    }
    d
}

/// A TSB / GSB / SSM Ford lists next to a DTC
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CvArticle {
    /// "26-2414"
    pub id: String,
    /// "TSB", "GSB", "SSM"
    pub kind: String,
    pub title: String,
    pub ft: String,
    pub module: String,
    /// Where it is on Ford's site
    pub url: String,
}

/// Ford's keys: "B1081", "B1081:07", "B10DA (PCM)", "B156D:89 (TCU)"
pub type Articles = HashMap<String, Vec<CvArticle>>;

fn bulletin_url(id: &str, ctx: &CvContext) -> String {
    let country = if ctx.country.is_empty() { "USA" } else { ctx.country.as_str() };
    let market = match country {
        "USA" | "PRI" | "VIR" => "US",
        "CAN" => "CA",
        c => &c[..c.len().min(2)],
    };
    format!(
        "{}/vdirsnet/TSB_SSM/DisplayArticle.aspx?content=/vdirsnet/TSB_SSM/DisplayArticle.aspx&id={}&lang={}&country={}&market={}",
        crate::paths::PTS,
        enc(id),
        enc(if ctx.lang.is_empty() { "EN-US" } else { &ctx.lang }),
        enc(country),
        enc(market),
    )
}

pub fn read_articles(json: &Value, ctx: &CvContext) -> Articles {
    let mut out = Articles::new();
    let Some(o) = json["TSBSSMGSBList"].as_object() else {
        return out;
    };
    let none = |v: Option<&str>| {
        let v = v.unwrap_or("").trim();
        if v.eq_ignore_ascii_case("null") {
            String::new()
        } else {
            v.to_string()
        }
    };
    for (key, items) in o {
        let list: Vec<CvArticle> = list(items)
            .iter()
            .filter_map(|t| {
                let t = s(t);
                let mut p = t.split('|');
                let id = none(p.next());
                if id.is_empty() {
                    return None;
                }
                let kind = Some(none(p.next()).to_uppercase())
                    .filter(|k| !k.is_empty())
                    .unwrap_or_else(|| "TSB".into());
                let url = bulletin_url(&id, ctx);
                Some(CvArticle {
                    id,
                    kind,
                    title: none(p.next()),
                    ft: none(p.next()),
                    module: none(p.next()),
                    url,
                })
            })
            .collect();
        out.insert(key.trim().to_uppercase(), list);
    }
    out
}

/// The articles for a DTC: those keyed by its code, or by its failure type or module when they match
pub fn articles_for<'a>(arts: &'a Articles, d: &Dtc, module: &str) -> Vec<&'a CvArticle> {
    let mut keys = vec![d.code.clone()];
    if !d.ft.is_empty() {
        keys.push(format!("{}:{}", d.code, d.ft));
    }
    if !module.is_empty() {
        keys.push(format!("{} ({})", d.code, module.to_uppercase()));
        if !d.ft.is_empty() {
            keys.push(format!("{}:{} ({})", d.code, d.ft, module.to_uppercase()));
        }
    }
    let mut out: Vec<&CvArticle> = Vec::new();
    for k in keys {
        for a in arts.get(&k).into_iter().flatten() {
            if !out.iter().any(|x| x.id == a.id) {
                out.push(a);
            }
        }
    }
    out
}

// =====================================================================================================================
// Over-the-air updates
// =====================================================================================================================

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StepClass {
    Ok,
    Bad,
    /// Cancelled or expired
    Gone,
    #[default]
    Pending,
}

/// One step Ford recorded for an update (repeats folded into one)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Step {
    pub when: When,
    pub text: String,
    /// Ford's code for it ("OTAM_S1010")
    pub code: String,
    pub class: StepClass,
    /// How many times in a row it was recorded, and the first time
    pub count: usize,
    pub since: When,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReleaseNotes {
    pub header: String,
    pub text: String,
    pub disclaimer: String,
    /// (heading, text)
    pub groups: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OtaUpdate {
    /// Its software ID
    pub id: String,
    pub status: String,
    pub class: StepClass,
    pub when: When,
    pub created: When,
    pub expires: When,
    /// "Recall", "Feature"…
    pub kind: String,
    pub groups: String,
    pub inhibit: Option<bool>,
    pub wifi: Option<bool>,
    pub power_up: String,
    pub recall: String,
    /// The modules it changes, and their software ("from → to")
    pub modules: Vec<(String, String)>,
    pub notes: Option<ReleaseNotes>,
    /// Newest first
    pub steps: Vec<Step>,
    /// Still waiting on the vehicle
    pub in_process: bool,
    /// Where its release notes are (asked after)
    notes_url: String,
}

/// The next update lined up
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Queued {
    pub sequence: String,
    pub modules: String,
    pub software: String,
    pub qualified: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ota {
    /// Newest first
    pub updates: Vec<OtaUpdate>,
    pub updates_failed: String,
    pub sequence: Vec<Queued>,
    pub sequence_failed: String,
}

fn step_text(st: &Value) -> String {
    let t = s(&st["translatedVehicleStatus"]);
    let tidy = |v: String| {
        let simple = v.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if simple && !v.is_empty() {
            let v = v.replace('_', " ");
            let mut c = v.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        } else {
            v
        }
    };
    let up = t.to_ascii_uppercase();
    if !t.is_empty() && !up.starts_with("OTAM_") && !up.starts_with("DCM_") {
        return tidy(t);
    }
    let w = words(&s(&st["deploymentStatus"]));
    if !w.is_empty() {
        return tidy(w);
    }
    if !t.is_empty() {
        return t;
    }
    s(&st["rawVehicleStatus"])
}

fn step_class(st: &Value) -> StepClass {
    let d = s(&st["deploymentStatus"]).to_ascii_lowercase();
    let raw = s(&st["rawVehicleStatus"]);
    let tr = s(&st["translatedVehicleStatus"]).to_ascii_lowercase();
    if d == "success" || raw == "OTAM_S1010" {
        StepClass::Ok
    } else if d == "failure" || raw == "OTAM_E1016" {
        StepClass::Bad
    } else if ["cancel", "expired"].iter().any(|w| d.contains(w) || tr.contains(w)) {
        StepClass::Gone
    } else {
        StepClass::Pending
    }
}

fn step_time(st: &Value) -> When {
    parse_time(&st["timestamp"]).or_else(|| parse_time(&st["vehicleTimeStamp"]))
}

/// A step that finishes an update
const DONE: [&str; 10] = [
    "success",
    "failure",
    "failed",
    "expired",
    "cancel",
    "canceled",
    "cancelled",
    "rejected",
    "complete",
    "completed",
];

pub fn read_update(d: &Value, now: DateTime<Utc>) -> OtaUpdate {
    static NULL: Value = Value::Null;
    let history = list(&d["statusHistory"]);
    let last = history.first().unwrap_or(&NULL);
    // Ford records the same step again every few minutes while waiting: each change once, with how many times
    let mut steps: Vec<Step> = Vec::new();
    for st in history {
        let (text, code) = (step_text(st), s(&st["rawVehicleStatus"]));
        match steps.last_mut() {
            Some(l) if l.text == text && l.code == code => {
                l.count += 1;
                l.since = step_time(st);
            }
            _ => steps.push(Step {
                when: step_time(st),
                text,
                code,
                class: step_class(st),
                count: 1,
                since: step_time(st),
            }),
        }
    }
    let det = &d["updateDetails"];
    let mut modules = Vec::new();
    let mut groups: Vec<String> = Vec::new();
    for g in list(&det["groups"]) {
        let gid = s(&g["groupId"]);
        if !gid.is_empty() && !groups.contains(&gid) {
            groups.push(gid);
        }
        for m in list(&g["modules"]) {
            let name = Some(s(&m["moduleInformation"]["name"]))
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| s(&m["name"]));
            let sw: Vec<String> = list(&m["softwares"])
                .iter()
                .map(|x| {
                    [s(&x["fromSoftwarePartNumber"]), s(&x["toSoftwarePartNumber"])]
                        .into_iter()
                        .filter(|p| !p.is_empty())
                        .collect::<Vec<_>>()
                        .join(" → ")
                })
                .filter(|x| !x.is_empty())
                .collect();
            modules.push((name, sw.join(" · ")));
        }
    }
    let last_status = s(&last["deploymentStatus"]).to_ascii_lowercase();
    let tr = s(&last["translatedVehicleStatus"]).to_ascii_lowercase();
    let expires = parse_time(&d["deploymentExpirationTime"]);
    let finished = (!history.is_empty() && DONE.contains(&last_status.as_str())) || tr.contains("expired") || tr.contains("cancel");
    let created = parse_time(&d["creationDate"]);
    let id = Some(s(&d["oemCorrelationId"]))
        .filter(|x| !x.is_empty())
        .unwrap_or_else(|| s(&d["deploymentId"]));
    OtaUpdate {
        id,
        status: if history.is_empty() { String::new() } else { step_text(last) },
        class: step_class(last),
        when: step_time(last).or(created),
        created,
        expires,
        kind: words(&s(&d["deploymentType"])),
        groups: groups.join(", "),
        inhibit: yes(&d["inhibitRequired"]),
        wifi: yes(&det["wifiRequired"]),
        power_up: s(&d["fordPowerUp"]),
        recall: s(&d["recallNumber"]),
        modules,
        notes: None,
        steps,
        in_process: !finished && !expires.is_some_and(|e| e < now),
        notes_url: if s(&d["oemCorrelationId"]).is_empty() {
            String::new()
        } else {
            s(&det["CDNReleaseNotesUrl"])
        },
    }
}

/// Release notes in the vehicle's language (Ford keeps them in each)
pub fn read_notes(json: &Value, country: &str) -> Option<ReleaseNotes> {
    let langs = list(json);
    let want = match country {
        "CAN" => "en-ca",
        "GBR" => "en-gb",
        "IRL" => "en-ie",
        _ => "en-us",
    };
    let code = |x: &Value| s(&x["LanguageCodeMobileApp"]).to_ascii_lowercase();
    let n = langs
        .iter()
        .find(|x| code(x) == want)
        .or_else(|| langs.iter().find(|x| code(x).starts_with("en")))
        .or(langs.first())?;
    let tidy = |v: &Value| s(v).replace(['*', '\r'], "").trim().to_string();
    let groups: Vec<(String, String)> = list(&n["groupContent"])
        .iter()
        .map(|g| (tidy(&g["groupHeader"]), tidy(&g["groupText"])))
        .collect();
    let text = Some(tidy(&n["Text"])).filter(|t| !t.is_empty()).unwrap_or_else(|| {
        if groups.is_empty() {
            tidy(&n["consentText"])
        } else {
            String::new()
        }
    });
    Some(ReleaseNotes {
        header: tidy(&n["manifestHeader"]),
        text,
        disclaimer: tidy(&n["manifestDisclaimer"]),
        groups,
    })
}

/// What's lined up next, as Ford's page filters it
pub fn read_sequence(json: &Value) -> Vec<Queued> {
    list(&json["result"])
        .iter()
        .filter(|x| {
            let st = s(&x["status"]).to_ascii_lowercase();
            let act = s(&x["activationStatus"]).to_ascii_lowercase();
            let plan = s(&x["servicePlanParameters"]).to_ascii_lowercase();
            matches!(st.as_str(), "qualified" | "non-qualified")
                && matches!(act.as_str(), "active" | "inactive")
                && matches!(plan.as_str(), "service + ota" | "ota only" | "service > ota")
        })
        .map(|x| {
            // "PCM (7E0), BCM (726)" → "PCM, BCM"
            let mut mods = String::new();
            let mut depth = 0;
            for c in s(&x["ecuNodes"]).chars() {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ if depth == 0 => mods.push(c),
                    _ => {}
                }
            }
            let mods: Vec<&str> = mods.split(',').map(str::trim).filter(|m| !m.is_empty()).collect();
            let fpu = s(&x["fpu"]);
            Queued {
                sequence: s(&x["sequenceId"]),
                modules: mods.join(", "),
                software: if fpu.is_empty() { s(&x["softwareUpdate"]) } else { fpu },
                qualified: s(&x["status"]).eq_ignore_ascii_case("qualified"),
            }
        })
        .collect()
}

// =====================================================================================================================
// Subscriptions
// =====================================================================================================================

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Product {
    pub name: String,
    /// "Active", "Pending", "Trial available", "Expired"
    pub status: String,
    pub active: bool,
    pub trial: bool,
    /// "Free trial", "Free", "Paid"
    pub kind: String,
    pub expires: When,
    pub expiring_soon: bool,
    pub renew_anytime: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Subscriptions {
    pub products: Vec<Product>,
    /// Ford doesn't see a modem, the modem isn't authorized, its service didn't answer…
    pub notes: Vec<String>,
    /// Ford has no subscription information for the vehicle
    pub none: bool,
    pub asked: When,
}

fn plan_kind(t: &str) -> String {
    match t {
        "AUTO" => "Free trial".into(),
        "FREE" => "Free".into(),
        "PAID" => "Paid".into(),
        t => words(t),
    }
}

pub fn read_subscriptions(json: &Value) -> Subscriptions {
    let mut out = Subscriptions::default();
    let code = |v: &Value| v["errorCode"].as_i64();
    if code(&json["vs"]) == Some(620) || code(&json["productCatalog"]) == Some(620) {
        out.notes
            .push("Ford’s subscription service didn’t answer in time. Try again in a moment.".into());
        return out;
    }
    if code(json) == Some(800) {
        out.none = true;
        return out;
    }
    let catalog = list(&json["productCatalog"]["offerCatalogProducts"]);
    let name_of = |sku: &str| -> String {
        for p in catalog {
            let full = s(&p["catalogProduct"]["product"]["sku"]);
            let short: Vec<&str> = full.split('-').take(3).collect();
            if short.join("-") == sku {
                return s(&p["catalogProduct"]["product"]["name"]);
            }
        }
        sku.to_string()
    };
    for p in list(&json["vs"]["subscriptionProducts"]) {
        let active = list(&p["activeSubscriptions"]);
        for x in active {
            let name = Some(s(&x["productName"]))
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| name_of(&s(&p["productSku"])));
            out.products.push(Product {
                name,
                status: if s(&x["status"]) == "PENDING" { "Pending" } else { "Active" }.into(),
                active: true,
                trial: false,
                kind: plan_kind(&s(&x["productType"])),
                expires: parse_time(&x["expirationDate"]),
                expiring_soon: yes(&x["dueForExpiration"]).unwrap_or(false),
                renew_anytime: yes(&x["renewAnytimeProduct"]).unwrap_or(false),
            });
        }
        let avail = &p["availableProduct"];
        if avail.is_object() && active.is_empty() {
            let plan = &avail["ratePlanList"][0];
            let t = s(&plan["productType"]);
            let trial = t == "FREE" || t == "AUTO";
            out.products.push(Product {
                name: name_of(&s(&p["productSku"])),
                status: if trial { "Trial available" } else { "Expired" }.into(),
                active: false,
                trial,
                kind: plan_kind(&t),
                expires: if trial { parse_time(&plan["endDate"]) } else { None },
                expiring_soon: false,
                renew_anytime: false,
            });
        }
    }
    let sec = &json["vs"]["security"];
    if sec["modemAvailable"] == Value::Bool(false) {
        out.notes.push("Ford doesn’t see a modem in this vehicle.".into());
    } else if sec["modemAuthorized"] == Value::Bool(false) {
        out.notes.push(
            "The vehicle’s modem isn’t authorized yet (the customer turns on connected services in the vehicle or the Ford app).".into(),
        );
    }
    out.asked = parse_time(&json["vs"]["lastRequested"]);
    out
}

// =====================================================================================================================
// Asking Ford
// =====================================================================================================================

/// What a JSON answer holds, for the log (its fields, never their values)
fn fields(v: &Value) -> String {
    match v {
        Value::Array(a) => format!("a list of {}", a.len()),
        Value::Object(o) => format!("fields {}", o.keys().take(14).cloned().collect::<Vec<_>>().join(", ")),
        _ => "something else".into(),
    }
}

impl Ford {
    /// Reads a JSON answer (a sign-in page instead counts as signed out)
    async fn cv_answer(&self, req: workbench_net::Request, what: &str) -> Result<Value> {
        let req = req
            .header("Accept", "application/json, */*")
            .header("X-Requested-With", "XMLHttpRequest");
        let res = self.fetch(req).await?;
        match serde_json::from_str::<Value>(&res.body) {
            Ok(v) => {
                self.log(&format!("   {} characters: {}", res.body.len(), fields(&v)));
                Ok(v)
            }
            Err(_) if crate::looks_like_sign_in(&res.body) => Err(Error::NeedsLogin),
            Err(_) => {
                self.log(&format!("   {what}: not JSON ({} characters)", res.body.len()));
                Err(Error::Format(format!("Ford didn't send {what}")))
            }
        }
    }

    async fn cv_get(&self, path: &str, what: &str) -> Result<Value> {
        let url = self.url(path)?;
        self.cv_answer(Http::request(Method::GET, url), what).await
    }

    /// The vehicle's connected vehicle status: the same request OASIS makes, with the values OASIS writes into its
    /// page (`vars`: from OASIS already read; none: OASIS is read first)
    pub async fn connected_vehicle(&self, vin: &str, vars: Option<CvVars>) -> Result<CvStatus> {
        let book = self.book(vin).await?;
        let v = match vars {
            Some(v) => v,
            None => self.oasis(vin).await?.cv,
        };
        let or = |a: &str, b: &str| if a.is_empty() { b.to_string() } else { a.to_string() };
        let country = or(&v.country, &or(book.param("country"), "USA"));
        let vehicle_vin = or(&v.vin, &book.vin);
        let pa_code = or(&v.pa_code, &or(book.param("pacode"), self.dealer_code()));
        if pa_code.is_empty() {
            return Err(Error::Format(
                "OASIS didn't include the dealer code connected vehicle data needs".into(),
            ));
        }
        let flavor = or(&v.flavor, "DEALERS");
        let year: u32 = or(&book.vehicle.year, book.param("modelYear")).parse().unwrap_or(0);
        let lang = or(&v.lang, "EN-US");
        let pa_code = if flavor == "FLEET" { "4FORD".to_string() } else { pa_code };
        let body = json!({
            "vin": vehicle_vin,
            "country": country,
            "masterID": or(&v.master_id, book.param("vehicleId")).parse::<i64>().unwrap_or(0),
            "language": lang,
            "isDetailed": true,
            "includeAllECU": true,
            "fromDate": v.start_date.clone(),
            "toDate": v.end_date.clone(),
            "paCode": pa_code,
            "progFromDate": or(&v.prog_start, &v.start_date),
            "progEndDate": v.end_date.clone(),
            "ignoreMetaData": true,
            "flavor": flavor,
            "fuelType": or(&v.fuel_type, "G"),
            "region": or(&v.region, &or(book.param("marketGroup"), "NA")),
            "syncModemVal": if v.modem.is_empty() { Value::Null } else { json!(v.modem.clone()) },
            "userRole": "DEALER",
            "featureCode": "",
            "productType": if book.vehicle.product_type.is_empty() { Value::Null } else { json!(book.vehicle.product_type) },
            "vehicleLine": if book.vehicle.vehicle_line.is_empty() { Value::Null } else { json!(book.vehicle.vehicle_line) },
            "modelYear": if year == 0 { String::new() } else { year.to_string() },
            "optionalParameter": json!({ "battery12VVinCheck": if year >= 2025 { "true" } else { "false" } }).to_string(),
        });
        let url = self.url(&format!(
            "/ConnectedVehicle/getCVGroupApiData?vin={}&countryCode={}",
            enc(&vehicle_vin),
            enc(&country)
        ))?;
        let req = Http::request(Method::POST, url)
            .header("Content-Type", "application/json")
            .body(body.to_string());
        let json = self.cv_answer(req, "connected vehicle data").await?;
        let user = &json["capabilityData"]["userDetails"];
        let ctx = CvContext {
            vin: vehicle_vin,
            user_country: Some(s(&user["userCountry"]))
                .filter(|c| !c.is_empty())
                .unwrap_or_else(|| country.clone()),
            user_state: Some(s(&user["userstate"])).filter(|c| !c.is_empty()).unwrap_or_else(|| "MI".into()),
            country,
            pa_code,
            lang,
        };
        Ok(read_status(&json, ctx))
    }

    /// Health reports between two dates (Ford's page asks for 60 days at a time)
    pub async fn cv_history(&self, ctx: &CvContext, from: NaiveDate, to: NaiveDate) -> Result<Vec<Report>> {
        let json = self
            .cv_get(
                &format!(
                    "/ConnectedVehicle/getDiagnostichistory?vin={}&pacode={}&countryCode={}&fromDate={}&toDate={}&isDetailed=true&dealerCheckRequired=false&includeAllEcu=true",
                    enc(&ctx.vin),
                    enc(&ctx.pa_code),
                    enc(&ctx.country),
                    from.format("%Y-%m-%d"),
                    to.format("%Y-%m-%d"),
                ),
                "the vehicle's health history",
            )
            .await?;
        Ok(read_history(&json["vhDataList"]))
    }

    /// Module names by address (Ford's list)
    pub async fn cv_modules(&self, ctx: &CvContext) -> Result<Modules> {
        let json = self
            .cv_get(
                &format!(
                    "/ConnectedVehicle/getMMLModuleAcronym?moduleAddr=ALL&lang={}&countryCode={}",
                    enc(if ctx.lang.is_empty() { "EN-US" } else { &ctx.lang }),
                    enc(&ctx.country)
                ),
                "module names",
            )
            .await?;
        Ok(read_modules(&json))
    }

    /// What these DTCs mean (their titles, failure types and status bytes)
    pub async fn cv_describe(&self, ctx: &CvContext, dtcs: &[Dtc]) -> Result<Descriptions> {
        if dtcs.is_empty() {
            return Ok(Descriptions::default());
        }
        let uniq = |f: &dyn Fn(&Dtc) -> &str| {
            let mut v: Vec<String> = Vec::new();
            for d in dtcs {
                let x = f(d).to_uppercase();
                if !x.is_empty() && !v.contains(&x) {
                    v.push(x);
                }
            }
            v.join("|")
        };
        let body = json!({
            "lang": if ctx.lang.is_empty() { "EN-US" } else { ctx.lang.as_str() },
            "Spec": "GGDS",
            "DispID": uniq(&|d| d.code.as_str()),
            "moduleAddr": uniq(&|d| d.ecu.as_str()),
            "failTypes": uniq(&|d| d.ft.as_str()),
            "statusBytes": uniq(&|d| d.sb.as_str()),
            "key": "",
            "fullStatusByteDesc": "yes",
            "progKeys": "",
        });
        let url = self.url(&format!(
            "/ConnectedVehicle/getDTCDataDetailDescription?countryCode={}",
            enc(&ctx.country)
        ))?;
        let req = Http::request(Method::POST, url)
            .header("Content-Type", "application/json")
            .body(body.to_string());
        let json = self.cv_answer(req, "DTC descriptions").await?;
        Ok(read_descriptions(&json, dtcs))
    }

    /// The TSBs, GSBs and SSMs Ford lists by DTC for this vehicle
    pub async fn cv_articles(&self, ctx: &CvContext) -> Result<Articles> {
        let country = if matches!(ctx.country.as_str(), "PRI" | "VIR") {
            "USA"
        } else {
            ctx.country.as_str()
        };
        let json = self
            .cv_get(
                &format!(
                    "/ConnectedVehicle/getArticlesBySymptomData?vin={}&pacode={}&lang={}&country={}&market={}",
                    enc(&ctx.vin),
                    enc(&ctx.pa_code),
                    enc(if ctx.lang.is_empty() { "EN-US" } else { &ctx.lang }),
                    enc(country),
                    enc(country)
                ),
                "TSBs by DTC",
            )
            .await?;
        Ok(read_articles(&json, ctx))
    }

    /// Over-the-air updates: those sent to the vehicle (with their release notes) and what's lined up next
    pub async fn cv_ota(&self, ctx: &CvContext) -> Result<Ota> {
        let (v, c) = (enc(&ctx.vin), enc(&ctx.country));
        let verbose_path = format!("/ConnectedVehicle/getVerboseData?vin={v}&countryCode={c}");
        let sequence_path = format!("/ConnectedVehicle/getOdomSequenceData?vin={v}");
        let (verbose, sequence) = tokio::join!(
            self.cv_get(&verbose_path, "over-the-air updates"),
            self.cv_get(&sequence_path, "the update sequence"),
        );
        let mut ota = Ota::default();
        let now = Utc::now();
        match verbose {
            Err(Error::NeedsLogin) => return Err(Error::NeedsLogin),
            Err(e) => ota.updates_failed = e.to_string(),
            Ok(json) => {
                ota.updates = list(&json["verboseData"]).iter().map(|d| read_update(d, now)).collect();
                let code = json["errorCode"].as_i64().unwrap_or(0);
                if ota.updates.is_empty() && code != 0 {
                    ota.updates_failed = Some(s(&json["errorMsg"]))
                        .filter(|m| !m.is_empty())
                        .unwrap_or_else(|| format!("Ford error {code}"));
                }
            }
        }
        match sequence {
            Ok(json) => ota.sequence = read_sequence(&json),
            Err(e) => ota.sequence_failed = e.to_string(),
        }
        // release notes (Ford keeps them per update)
        for u in ota.updates.iter_mut().take(12) {
            let url = std::mem::take(&mut u.notes_url);
            if url.is_empty() || url.to_ascii_lowercase().contains("vehicleupdates") {
                continue;
            }
            match self
                .cv_get(
                    &format!("/ConnectedVehicle/getReleaseNoteUrlData?countryCode={c}&url={}", enc(&url)),
                    "release notes",
                )
                .await
            {
                Ok(json) => u.notes = read_notes(&json, &ctx.country),
                Err(Error::NeedsLogin) => return Err(Error::NeedsLogin),
                Err(e) => self.log(&format!("   release notes: {e}")),
            }
        }
        ota.updates.sort_by(|a, b| b.when.cmp(&a.when));
        Ok(ota)
    }

    /// Software subscriptions (connectivity, BlueCruise…): what's active, and what can be started
    pub async fn cv_subscriptions(&self, ctx: &CvContext) -> Result<Subscriptions> {
        let json = self
            .cv_get(
                &format!(
                    "/ConnectedVehicle/getVehicleSubscriptionSummaryDataV1?vin={}&countryCode={}&state={}",
                    enc(&ctx.vin),
                    enc(if ctx.user_country.is_empty() {
                        &ctx.country
                    } else {
                        &ctx.user_country
                    }),
                    enc(&ctx.user_state)
                ),
                "software subscriptions",
            )
            .await?;
        Ok(read_subscriptions(&json))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_oasis_vars() {
        let page = r#"<script>var countryCode = "USA"; var currentVin='1FTFW1E85MFA00000';
            var cvPACode = "01234"; var flavor = 'DEALERS'; var masterID = "12345"; var startDate = "2026-09-01";
            localStorage.setItem('CVFuelType', 'G'); var variable = 'x';</script>"#;
        let v = read_vars(page, " TCU4G ");
        assert_eq!(v.country, "USA");
        assert_eq!(v.vin, "1FTFW1E85MFA00000");
        assert_eq!(v.pa_code, "01234");
        assert_eq!(v.master_id, "12345");
        assert_eq!(v.fuel_type, "G");
        assert_eq!(v.modem, "TCU4G");
        assert_eq!(v.region, "");
    }

    #[test]
    fn reads_times() {
        let t = |v: Value| parse_time(&v).map(|d| d.format("%Y-%m-%d %H:%M").to_string());
        assert_eq!(t(json!("2026-03-02 14:05:00")), Some("2026-03-02 14:05".into()));
        assert_eq!(t(json!("2026-03-02T14:05:00.123Z")), Some("2026-03-02 14:05".into()));
        assert_eq!(t(json!("2026-03-02T09:05:00-05:00")), Some("2026-03-02 14:05".into()));
        assert_eq!(t(json!(1772460300)), Some("2026-03-02 14:05".into()));
        assert_eq!(t(json!("1772460300000")), Some("2026-03-02 14:05".into()));
        assert_eq!(t(json!("2026-03-02")), Some("2026-03-02 00:00".into()));
        assert_eq!(t(json!("")), None);
        assert_eq!(words("LOW_PRESSURE"), "Low pressure");
        assert_eq!(words("Normal"), "Normal");
        assert_eq!(dist(160934.0, true), "100,000 mi");
    }

    #[test]
    fn reads_status() {
        let j = json!({
            "capabilityData": { "dealerAgreed": "Yes", "privacyResponse": { "auth": "No" }, "userDetails": { "userstate": "OH" } },
            "connectedVehicleData": {
                "tmcData": { "metrics": {
                    "odometer": { "value": 77592.4, "updateTime": "2026-11-10 12:00:00" },
                    "fuelLevel": { "value": 55.6, "updateTime": "2026-11-10 12:00:00" },
                    "fuelRange": { "value": 400.0 },
                    "oilLifeRemaining": { "value": 62 },
                    "batteryStateOfCharge": { "value": 88 },
                    "batteryVoltage": { "value": 12.46 },
                    "tirePressure": [ { "vehicleWheel": "FRONT_LEFT", "value": 241.3 }, { "vehicleWheel": "FRONT_RIGHT", "value": 240 },
                                      { "vehicleWheel": "REAR_LEFT", "value": 255 }, { "vehicleWheel": "REAR_RIGHT", "value": -1000 } ],
                    "tirePressureSystemStatus": [ { "value": "LOW_PRESSURE_WARNING" } ],
                    "configurations": { "automaticSoftwareUpdateOptInSetting": { "value": "OFF" } }
                } },
                "diagCurrentData": { "eventTime": "2026-11-10 12:00:00", "Metadata": { "dtcs": [
                    { "dtcCode": "u2100", "failureType": "00", "dtcsStatus": "2F", "ecuid": "726" } ] },
                    "vhAlerts": [ { "alerts": [ { "alertType": "NEW_ALERT", "messageDesc": "Low tire pressure", "urgency": "HIGH" } ] } ] },
                "diagHistoryData": { "fromDate": "2026-09-11", "vhDataList": [
                    { "eventDate": "2026-10-01", "vhDataOnEventTime": [
                        { "eventTime": "2026-10-01 08:00:00", "odometer": 77000, "dtcs": [ { "dtcCode": "B1081" } ], "alerts": [] },
                        { "eventTime": "2026-10-02 08:00:00", "odometer": 77100, "batteryVoltage": 12.1, "otaChange": true } ] } ] }
            }
        });
        let st = read_status(
            &j,
            CvContext {
                country: "USA".into(),
                ..Default::default()
            },
        );
        assert_eq!(st.notes, ["The customer's privacy settings don't allow connected vehicle data."]);
        assert_eq!(st.readings[0].value.as_deref(), Some("48,213 mi"));
        assert_eq!(st.readings[1].value.as_deref(), Some("55% · 248 mi range"));
        assert_eq!(st.readings[3].value.as_deref(), Some("88% · 12.5 V"));
        assert_eq!(st.odometer_miles(), Some(48213.0));
        let t = st.tires.as_ref().unwrap();
        assert_eq!(t.front[0].value, Some(35));
        assert_eq!(t.rear.len(), 2);
        assert_eq!(t.rear[1].value, None);
        assert_eq!(t.system.as_deref(), Some("Low pressure warning"));
        let now = st.now.as_ref().unwrap();
        assert_eq!(now.dtcs[0].full(), "U2100:00:2F");
        assert_eq!(now.dtcs[0].key(), "U2100:00");
        assert!(now.alerts[0].urgent);
        assert_eq!(now.alerts[0].kind, "New alert");
        assert_eq!(st.history.len(), 2);
        assert!(st.history[0].ota_change && !st.history[1].ota_change);
        assert_eq!(st.history[1].dtcs[0].code, "B1081");
        assert_eq!(st.history_from, NaiveDate::from_ymd_opt(2026, 9, 11));
        assert_eq!(st.ota_settings[0].value.as_deref(), Some("Off"));
        assert!(st.ota_settings[0].warn);
        assert!(st.everything.iter().any(|r| r.label == "Oil life remaining"));
    }

    #[test]
    fn reads_the_rest() {
        let ctx = CvContext {
            country: "USA".into(),
            lang: "EN-US".into(),
            ..Default::default()
        };
        let mods = read_modules(&json!({ "moduleDetails": [ { "moduleAddr": "0x0726", "moduleAcronym": "BCM", "moduleDesc": "Body" } ] }));
        let d = Dtc {
            code: "U2100".into(),
            ft: "00".into(),
            ecu: "726".into(),
            ..Default::default()
        };
        assert_eq!(module_of(&mods, &d).0, "BCM");
        let arts = read_articles(
            &json!({ "TSBSSMGSBList": { "U2100": [ "26-2414|tsb|Network fault|null|null" ], "U2100:00 (BCM)": [ "SSM 5|SSM|x||BCM", "26-2414|TSB|dup||" ] } }),
            &ctx,
        );
        let found = articles_for(&arts, &d, "BCM");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].kind, "TSB");
        assert!(found[0].url.contains("id=26-2414&lang=EN-US&country=USA&market=US"));
        let desc = read_descriptions(
            &json!({ "dtsGGDSRes": { "lstDTCData": [ { "dtcCode": "u2100", "dtcTitle": "Initial Configuration Not Complete" } ] },
                     "failureTypeRes": { "lstDTCFTInfo": [ { "strFailureTypeCode": "00", "strFailureTypeDesc": "No Sub Type Information" } ] } }),
            &[
                d.clone(),
                Dtc {
                    code: "B1".into(),
                    ..Default::default()
                },
            ],
        );
        assert_eq!(desc.title(&d), "Initial Configuration Not Complete");
        assert_eq!(desc.titles.get("B1").map(String::as_str), Some(""));
        assert_eq!(desc.detail(&d), "Failure type 00: No Sub Type Information");

        let now = Utc::now();
        let u = read_update(
            &json!({ "oemCorrelationId": "ABC", "deploymentType": "RECALL", "inhibitRequired": "Y",
                "statusHistory": [
                    { "deploymentStatus": "IN_PROGRESS", "translatedVehicleStatus": "OTAM_S0023", "rawVehicleStatus": "OTAM_S0023", "timestamp": "2026-11-10 10:00:00" },
                    { "deploymentStatus": "IN_PROGRESS", "translatedVehicleStatus": "OTAM_S0023", "rawVehicleStatus": "OTAM_S0023", "timestamp": "2026-11-10 09:00:00" },
                    { "deploymentStatus": "QUEUED", "timestamp": "2026-11-09 09:00:00" } ],
                "updateDetails": { "CDNReleaseNotesUrl": "https://x/notes.json", "groups": [ { "groupId": 1, "modules": [
                    { "moduleInformation": { "name": "PCM" }, "softwares": [ { "fromSoftwarePartNumber": "A", "toSoftwarePartNumber": "B" } ] } ] } ] } }),
            now,
        );
        assert_eq!(u.status, "In progress");
        assert!(u.in_process);
        assert_eq!(u.steps.len(), 2);
        assert_eq!(u.steps[0].count, 2);
        assert_eq!(u.modules, [("PCM".to_string(), "A → B".to_string())]);
        assert_eq!(u.kind, "Recall");
        assert_eq!(u.inhibit, Some(true));
        let n = read_notes(
            &json!([ { "LanguageCodeMobileApp": "fr-ca", "manifestHeader": "Non" },
            { "LanguageCodeMobileApp": "en-us", "manifestHeader": "*Better* radio", "Text": "Fixes\r things" } ]),
            "USA",
        )
        .unwrap();
        assert_eq!((n.header.as_str(), n.text.as_str()), ("Better radio", "Fixes things"));
        let q = read_sequence(&json!({ "result": [
            { "sequenceId": 2, "status": "Qualified", "activationStatus": "Active", "servicePlanParameters": "OTA Only", "ecuNodes": "PCM (7E0), BCM (726)", "fpu": "FPU-1" },
            { "sequenceId": 3, "status": "Done", "activationStatus": "Active", "servicePlanParameters": "OTA Only" } ] }));
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].modules, "PCM, BCM");
        assert!(q[0].qualified);

        let subs = read_subscriptions(&json!({ "vs": {
            "security": { "modemAvailable": true, "modemAuthorized": false },
            "subscriptionProducts": [
                { "productSku": "A-B-C", "activeSubscriptions": [ { "status": "ACTIVE", "productType": "PAID", "expirationDate": "2027-01-01" } ] },
                { "productSku": "D-E-F", "availableProduct": { "ratePlanList": [ { "productType": "AUTO", "endDate": "2027-02-01" } ] } } ] },
            "productCatalog": { "offerCatalogProducts": [ { "catalogProduct": { "product": { "sku": "A-B-C-1", "name": "Connectivity" } } } ] } }));
        assert_eq!(subs.products.len(), 2);
        assert_eq!(subs.products[0].name, "Connectivity");
        assert_eq!(subs.products[0].kind, "Paid");
        assert_eq!(subs.products[1].status, "Trial available");
        assert_eq!(subs.products[1].kind, "Free trial");
        assert_eq!(subs.notes.len(), 1);
    }
}
