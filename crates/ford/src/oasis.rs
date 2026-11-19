//! OASIS for a VIN: Ford's server-built OASIS page, read for what the app shows (the Electron app's `manual/oasis/`):
//! the vehicle's information (grouped), field service actions (recalls), warning messages, and warranty coverage,
//! each coverage checked against the warranty start date and the odometer ([`check`]).

use chrono::{Months, NaiveDate};

use crate::html::{Doc, El, clean};
use crate::vehicle::enc;
use crate::{Error, Ford, Result};

/// One OASIS page, read
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Oasis {
    pub vin: String,
    /// "F-150 · Lariat"
    pub title: String,
    /// When OASIS made the page ("11/15/2026 2:05 PM"), as it says
    pub stamp: String,
    /// The vehicle's information, grouped (Vehicle, Powertrain, Paint…; anything new in Other)
    pub groups: Vec<Group>,
    pub fsa: Fsa,
    pub warnings: Warnings,
    pub warranty: Warranty,
    /// What its page sets for the connected vehicle request (crate::connected)
    pub cv: crate::connected::CvVars,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Group {
    pub title: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Field {
    /// "Paint Code"
    pub label: String,
    pub value: String,
    /// A code (shown in a fixed-width font)
    pub code: bool,
}

/// Field service actions (recalls)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fsa {
    /// OASIS had the section
    pub found: bool,
    pub title: String,
    pub items: Vec<FsaItem>,
    /// Ford's notices above the list (their text, and the address of what they open)
    pub notices: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FsaItem {
    /// "24S10"
    pub number: String,
    pub description: String,
    /// The FSA's documents on Ford's site
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Warnings {
    pub found: bool,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Warranty {
    pub start: Option<NaiveDate>,
    pub start_text: String,
    pub build: Option<NaiveDate>,
    pub build_text: String,
    pub months_in_service: Option<u32>,
    pub coverages: Vec<Coverage>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Coverage {
    /// "Bumper to Bumper"
    pub kind: String,
    /// "3 years / 36,000 Miles"
    pub description: String,
    pub info: String,
}

/// How the vehicle information is grouped (by Ford's labels); anything else lands in "Other"
const GROUPS: &[(&str, &[&str])] = &[
    (
        "Vehicle",
        &[
            "VEHICLE DESCRIPTION",
            "VERSION/SERIES",
            "BODY STYLE",
            "DRIVE TYPE",
            "FUEL TYPE",
            "GROSS VEHICLE WEIGHT",
        ],
    ),
    (
        "Powertrain",
        &["ENGINE", "ENGINE CALIBRATION", "TRANSMISSION", "AXLE RATIO", "AXLE CODE"],
    ),
    ("Paint, wheels & tires", &["PAINT COLOR", "PAINT CODE", "WHEEL SIZE", "TIRE"]),
    ("Electronics", &["SYNC VERSION", "RADIO", "MODEM", "VHR ACTIVATED"]),
    ("Sale", &["SOLD TO FLEET", "RETAIL SALES TYPE"]),
];
const CODES: &[&str] = &["PAINT CODE", "ENGINE CALIBRATION", "AXLE CODE", "RETAIL SALES TYPE"];

/// "GROSS VEHICLE WEIGHT" → "Gross Vehicle Weight" (GVW, SYNC, VHR kept)
pub fn nice(k: &str) -> String {
    let mut out = String::new();
    let mut up = true;
    for c in k.to_lowercase().chars() {
        out.push(if up { c.to_ascii_uppercase() } else { c });
        up = c.is_whitespace() || c == '/' || c == '(';
    }
    out.replace("Gvw", "GVW").replace("Sync", "SYNC").replace("Vhr", "VHR")
}

/// A label's value: the next label after it (Ford's "label: value" pairs); `tags`: what else counts as a value
fn pairs(panel: Option<El<'_>>, tags: &[&str]) -> Vec<(String, String)> {
    let Some(panel) = panel else { return Vec::new() };
    panel
        .find_all(|e| e.tag() == "label" && e.has_class("label"))
        .into_iter()
        .filter_map(|lab| {
            let k = lab.text().trim_end_matches(':').trim().to_string();
            if k.is_empty() {
                return None;
            }
            let mut v = lab.next_element();
            while let Some(e) = v {
                if tags.contains(&e.tag()) {
                    break;
                }
                v = e.next_element();
            }
            Some((k, v.map(|e| e.text()).unwrap_or_default()))
        })
        .collect()
}

/// A section's heading ("NO OPEN FIELD SERVICE ACTIONS"), without Ford's arrows
fn heading(doc: &Doc, panel: &str) -> String {
    doc.root()
        .find(|e| e.tag() == "label" && e.attr("data-results") == Some(panel))
        .map(|e| e.text().trim_start_matches(['▴', '▾', ' ']).to_string())
        .unwrap_or_default()
}

/// Reads Ford's OASIS page
pub fn read(page: &str) -> Oasis {
    let doc = Doc::parse(page);
    let mut o = Oasis::default();
    let title = doc.title();
    o.vin = ["OASIS RESULTS:", "OASIS RESULT:"]
        .iter()
        .find_map(|p| title.to_uppercase().starts_with(p).then(|| title[p.len()..].trim().to_string()))
        .unwrap_or(title);

    // when OASIS made the page: the last column of the main panel's heading
    if let Some(head) = doc
        .root()
        .find(|e| e.has_class("panel-primary"))
        .and_then(|p| p.find(|e| e.has_class("panel-heading")))
    {
        if let Some(strong) = head.find_all(|e| e.has_class("col-md-4")).last().and_then(|c| c.first("strong")) {
            let t = strong.text();
            let cut = t.to_uppercase().find("LOCAL TIME").unwrap_or(t.len());
            o.stamp = t[..cut].trim().to_string();
        }
    }

    // the vehicle's information
    let info = pairs(doc.by_id("vehicleinfo-panel"), &["label"]);
    let get = |k: &str| {
        info.iter()
            .find(|(l, _)| l.eq_ignore_ascii_case(k))
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let (desc, series) = (get("VEHICLE DESCRIPTION"), get("VERSION/SERIES"));
    let series = series.trim_end_matches(" VERSION").trim_end_matches(" version").to_string();
    o.title = [desc, series].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
    let mut used = vec![false; info.len()];
    let field = |(k, v): &(String, String)| Field {
        label: nice(k),
        value: v.clone(),
        code: CODES.contains(&k.to_uppercase().as_str()),
    };
    for (title, keys) in GROUPS {
        let mut fields = Vec::new();
        for key in keys.iter() {
            if let Some(i) = info.iter().position(|(k, _)| k.eq_ignore_ascii_case(key)) {
                used[i] = true;
                fields.push(field(&info[i]));
            }
        }
        if !fields.is_empty() {
            o.groups.push(Group {
                title: title.to_string(),
                fields,
            });
        }
    }
    let rest: Vec<Field> = info.iter().zip(&used).filter(|(_, u)| !**u).map(|(p, _)| field(p)).collect();
    if !rest.is_empty() {
        o.groups.push(Group {
            title: "Other".into(),
            fields: rest,
        });
    }

    o.fsa = read_fsa(&doc);
    o.warnings = read_warnings(&doc);
    o.warranty = read_warranty(&doc);
    o.cv = crate::connected::read_vars(page, &get("MODEM"));
    o
}

fn read_fsa(doc: &Doc) -> Fsa {
    let Some(panel) = doc.by_id("fsa-panel") else {
        return Fsa::default();
    };
    let mut f = Fsa {
        found: true,
        title: heading(doc, "fsa-panel"),
        ..Default::default()
    };
    for li in panel.all("li") {
        let a = li
            .find(|e| e.tag() == "a" && e.attr("data-fsanum").is_some())
            .or_else(|| li.first("a"));
        let number = clean(
            a.and_then(|a| a.attr("data-fsanum"))
                .map(str::to_string)
                .unwrap_or_else(|| a.map(|a| a.text()).unwrap_or_default())
                .as_str(),
        );
        let labels = clean(&li.all("label").iter().map(|l| l.text()).collect::<Vec<_>>().join(" "));
        let description = if labels.is_empty() {
            li.text().replacen(&number, "", 1).trim().to_string()
        } else {
            labels
        };
        if !number.is_empty() || !description.is_empty() {
            f.items.push(FsaItem {
                number,
                description,
                url: a.and_then(|a| a.attr("href")).unwrap_or("").to_string(),
            });
        }
    }
    // Ford's notices above the list
    for a in panel.find_all(|e| e.tag() == "a" && e.attr("href").is_some()) {
        if a.closest(|e| e.tag() == "li").is_some() || a.closest(|e| e.has_class("row")).is_none() {
            continue;
        }
        let text = a.closest(|e| e.tag() == "label").map(|l| l.text()).unwrap_or_else(|| a.text());
        let text = ["NOTICE –", "NOTICE -", "NOTICE—"]
            .iter()
            .find_map(|p| text.to_uppercase().starts_with(p).then(|| text[p.len()..].trim().to_string()))
            .unwrap_or(text);
        if !text.is_empty() {
            f.notices.push((text, a.attr("href").unwrap_or("").to_string()));
        }
    }
    f
}

fn read_warnings(doc: &Doc) -> Warnings {
    let body = doc.by_id("warnmsg-panel").and_then(|p| p.find(|e| e.has_class("panel-body")));
    let mut w = Warnings {
        found: body.is_some(),
        lines: Vec::new(),
    };
    if let Some(body) = body {
        let is_item =
            |e: &El| matches!(e.tag(), "li" | "p" | "label") || (e.tag() == "div" && e.parent().is_some_and(|p| p.has_class("row")));
        let items = body.find_all(is_item);
        let pieces: Vec<El> = if items.is_empty() { vec![body] } else { items.clone() };
        for n in pieces {
            // only the innermost pieces
            if !items.is_empty() && n.find(|e| matches!(e.tag(), "li" | "p" | "label")).is_some() {
                continue;
            }
            let t = n.text();
            if !t.is_empty() && !w.lines.contains(&t) {
                w.lines.push(t);
            }
        }
    }
    // OASIS hides its "imported" notice unless it applies
    if doc.by_id("husmDiv").is_some_and(|h| !h.hidden()) {
        let msg = doc.by_id("husmMessage").map(|e| e.text()).unwrap_or_default();
        let more = doc.by_id("husmInformation").map(|e| e.text()).unwrap_or_default();
        if !msg.is_empty() {
            w.lines
                .push([msg, more].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" — "));
        }
    }
    w
}

fn read_warranty(doc: &Doc) -> Warranty {
    let general = pairs(doc.by_id("gwi-panel"), &["label", "span", "p", "div"]);
    let get = |k: &str| {
        general
            .iter()
            .find(|(l, _)| l.eq_ignore_ascii_case(k))
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let months = doc
        .root()
        .find(|e| e.attr("data-results") == Some("gwi-panel"))
        .and_then(|e| e.closest(|p| p.has_class("panel-heading")))
        .map(|h| h.text())
        .and_then(|t| {
            let up = t.to_uppercase();
            let at = up.find("MONTHS IN SERVICE:")? + "MONTHS IN SERVICE:".len();
            up[at..].trim_start().split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
        });
    let mut coverages: Vec<Coverage> = Vec::new();
    for (k, v) in pairs(doc.by_id("warantycov-panel"), &["label", "span", "p", "div"]) {
        match k.to_uppercase().as_str() {
            "COVERAGE TYPE" => coverages.push(Coverage {
                kind: v,
                ..Default::default()
            }),
            "COVERAGE DESCRIPTION" => {
                if let Some(c) = coverages.last_mut() {
                    c.description = v;
                }
            }
            "ADDITIONAL INFO" => {
                if let Some(c) = coverages.last_mut() {
                    c.info = v;
                }
            }
            _ => {}
        }
    }
    let (start_text, build_text) = (get("WARRANTY START DATE"), get("BUILD DATE"));
    Warranty {
        start: parse_date(&start_text),
        start_text,
        build: parse_date(&build_text),
        build_text,
        months_in_service: months,
        coverages,
    }
}

/// "15-NOV-2026", "15 Nov 2026", "2026-11-15", "11/15/2026"
pub fn parse_date(t: &str) -> Option<NaiveDate> {
    let t = t.trim();
    let parts: Vec<&str> = t
        .split(|c: char| c == '-' || c == '/' || c == ',' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .collect();
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let month_word = |w: &str| {
        let w = w.to_lowercase();
        (w.len() >= 3)
            .then(|| MONTHS.iter().position(|m| w.starts_with(m)))
            .flatten()
            .map(|m| m as u32 + 1)
    };
    let num = |w: &str| w.parse::<i32>().ok();
    match parts.as_slice() {
        // 15-NOV-2026
        [d, m, y, ..] if month_word(m).is_some() && num(d).is_some() && y.len() == 4 => {
            NaiveDate::from_ymd_opt(num(y)?, month_word(m)?, num(d)? as u32)
        }
        // Nov 15, 2026
        [m, d, y, ..] if month_word(m).is_some() && num(d).is_some() && y.len() == 4 => {
            NaiveDate::from_ymd_opt(num(y)?, month_word(m)?, num(d)? as u32)
        }
        // 2026-11-15
        [y, m, d, ..] if y.len() == 4 => NaiveDate::from_ymd_opt(num(y)?, num(m)? as u32, num(d.get(..2).unwrap_or(d))? as u32),
        // 11/15/2026
        [m, d, y, ..] if y.len() >= 4 => NaiveDate::from_ymd_opt(num(&y[..4])?, num(m)? as u32, num(d)? as u32),
        _ => None,
    }
}

const KM_PER_MILE: f64 = 1.609_344;

/// A coverage's limits: months, miles, or unlimited miles ("5 years / 60,000 Miles", "36 Months/36,000 Miles",
/// "Unlimited Miles", "100,000 km")
pub fn limits(text: &str) -> (Option<u32>, Option<f64>, bool) {
    let t = text.to_lowercase();
    let words: Vec<String> = t
        .replace('/', " ")
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != ',').to_string())
        .collect();
    let mut months = None;
    let mut miles = None;
    let mut unlimited = false;
    for (i, w) in words.iter().enumerate() {
        // the number may be stuck to its unit ("36months") or before it
        let (n, unit) = {
            let split = w.find(|c: char| c.is_alphabetic()).unwrap_or(w.len());
            if split > 0 && split < w.len() {
                (w[..split].replace(',', "").parse::<f64>().ok(), w[split..].to_string())
            } else if split == w.len() {
                (
                    w.replace(',', "").parse::<f64>().ok(),
                    words.get(i + 1).cloned().unwrap_or_default(),
                )
            } else {
                (None, String::new())
            }
        };
        if w == "unlimited" && words.get(i + 1).is_some_and(|u| u.starts_with("mi") || u.starts_with("k")) {
            unlimited = true;
        }
        let Some(n) = n else { continue };
        if months.is_none() && (unit.starts_with("year") || unit.starts_with("yr")) {
            months = Some((n * 12.0).round() as u32);
        } else if months.is_none() && (unit.starts_with("month") || unit.starts_with("mo")) {
            months = Some(n.round() as u32);
        } else if miles.is_none() && (unit.starts_with("mile") || unit == "mi") {
            miles = Some(n);
        } else if miles.is_none() && (unit.starts_with("kilomet") || unit == "km") {
            miles = Some(n / KM_PER_MILE);
        }
    }
    (months, miles, unlimited)
}

/// Where a coverage stands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    Covered,
    Expired,
    /// In time, but the mileage isn't known
    CheckMiles,
    /// Its terms couldn't be read
    SeeTerms,
}

impl Standing {
    pub fn label(self) -> &'static str {
        match self {
            Standing::Covered => "Covered",
            Standing::Expired => "Expired",
            Standing::CheckMiles => "Check miles",
            Standing::SeeTerms => "See terms",
        }
    }
}

/// "Nov 15, 2026"
pub fn nice_date(d: NaiveDate) -> String {
    d.format("%b %-d, %Y").to_string()
}

/// 12345 → "12,345"
pub fn thousands(n: i64) -> String {
    let s = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// Checks a coverage against the warranty start (`start`: the build date when there's none), the odometer (miles)
/// and today: where it stands, and why ("1,234 days left (until …) · 12,000 mi left")
pub fn check(c: &Coverage, start: Option<NaiveDate>, odometer: Option<f64>, today: NaiveDate) -> (Standing, String) {
    let (months, miles, unlimited) = limits(&c.description);
    let ends = start.zip(months).and_then(|(s, m)| s.checked_add_months(Months::new(m)));
    let time_out = ends.is_some_and(|e| today > e);
    let days_left = ends.map(|e| (e - today).num_days());
    let miles_left = odometer.zip(miles).map(|(o, m)| (m - o).round() as i64);
    let miles_out = miles_left.is_some_and(|l| l < 0);
    if months.is_none() && miles.is_none() && !unlimited {
        return (Standing::SeeTerms, "Check the description".into());
    }
    if time_out || miles_out {
        let mut why = Vec::new();
        if let (true, Some(e)) = (time_out, ends) {
            why.push(format!("Out by time (ended {})", nice_date(e)));
        }
        if let (true, Some(l)) = (miles_out, miles_left) {
            why.push(format!("Out by mileage ({} mi over)", thousands(-l)));
        }
        return (Standing::Expired, why.join(" · "));
    }
    if miles.is_some() && odometer.is_none() {
        let until = ends.map(|e| format!(" until {}", nice_date(e))).unwrap_or_default();
        return (Standing::CheckMiles, format!("In time{until}, mileage unknown: enter the odometer"));
    }
    let mut parts = Vec::new();
    if let (Some(e), Some(d)) = (ends, days_left) {
        parts.push(format!("{} days left (until {})", thousands(d), nice_date(e)));
    }
    if let Some(l) = miles_left {
        parts.push(format!("{} mi left", thousands(l)));
    } else if unlimited {
        parts.push("unlimited miles".into());
    }
    let text = if parts.is_empty() {
        "Covered".to_string()
    } else {
        parts.join(" · ")
    };
    (Standing::Covered, text)
}

impl Ford {
    /// OASIS for a VIN (Ford's page, read)
    pub async fn oasis(&self, vin: &str) -> Result<Oasis> {
        let book = self.book(vin).await?;
        let p = |k: &str| book.param(k).to_string();
        let year = if book.vehicle.year.is_empty() {
            p("modelYear")
        } else {
            book.vehicle.year.clone()
        };
        let country = Some(p("country")).filter(|c| !c.is_empty()).unwrap_or_else(|| "USA".into());
        let region = Some(p("marketGroup")).filter(|c| !c.is_empty()).unwrap_or_else(|| "NA".into());
        let url = self.url(&format!(
            "/oasis/Index?vin={}&ModelYear={}&DefaultDisplayName={}&OasisEnv=RELEASE&country={}&region={}&v=1238",
            enc(&book.vin),
            enc(&year),
            enc(&book.vehicle.market_name),
            enc(&country),
            enc(&region)
        ))?;
        let page = self.text(url).await?;
        let up = page.to_uppercase();
        if !(up.contains("VEHICLEINFO-PANEL") || up.contains("OASIS RESULT")) {
            return Err(Error::Format("Ford sent back something other than OASIS".into()));
        }
        let mut o = read(&page);
        if o.vin.is_empty() || o.vin.contains(' ') {
            o.vin = book.vin.clone();
        }
        Ok(o)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<html><head><title>OASIS RESULTS: 1FTFW1E85MFA00000</title></head><body>
      <div class="panel panel-primary"><div class="panel-heading"><div class="row">
        <div class="col-md-4"><strong>OASIS</strong></div><div class="col-md-4"><strong>11/15/2026 2:05 PM LOCAL TIME</strong></div>
      </div></div></div>
      <div id="vehicleinfo-panel">
        <label class="label">VEHICLE DESCRIPTION:</label> <label>F-150</label>
        <label class="label">VERSION/SERIES:</label> <label>LARIAT VERSION</label>
        <label class="label">PAINT CODE:</label> <label>UM</label>
        <label class="label">SYNC VERSION:</label> <label>4</label>
        <label class="label">NEW THING:</label> <label>yes</label>
      </div>
      <div class="panel-heading"><label data-results="fsa-panel">▾ OPEN FIELD SERVICE ACTIONS</label></div>
      <div id="fsa-panel"><div class="row"><label>NOTICE – <a href="/notice1">Software repair steps</a></label></div>
        <ul><li><a data-fsanum="24S10" href="/fsa/24S10">24S10</a><label>Rear axle bolt</label></li>
            <li><a href="/fsa/23C01">23C01</a> Backup camera</li></ul></div>
      <div id="warnmsg-panel"><div class="panel-body"><ul><li>Vehicle sold as is</li><li>Vehicle sold as is</li><li>Odometer replaced</li></ul></div></div>
      <div id="husmDiv" style="display:none"><span id="husmMessage">Imported</span></div>
      <div class="panel-heading"><label data-results="gwi-panel">GENERAL</label> MONTHS IN SERVICE: 44</div>
      <div id="gwi-panel"><label class="label">WARRANTY START DATE:</label><span>04-MAR-2021</span>
        <label class="label">BUILD DATE:</label><span>2021-02-01</span></div>
      <div id="warantycov-panel">
        <label class="label">COVERAGE TYPE:</label><span>Bumper to Bumper</span>
        <label class="label">COVERAGE DESCRIPTION:</label><span>3 years / 36,000 Miles</span>
        <label class="label">COVERAGE TYPE:</label><span>Powertrain</span>
        <label class="label">COVERAGE DESCRIPTION:</label><span>5 Years/60,000 miles</span>
        <label class="label">ADDITIONAL INFO:</label><span>Transferable</span>
      </div></body></html>"#;

    #[test]
    fn reads_oasis() {
        let o = read(PAGE);
        assert_eq!(o.vin, "1FTFW1E85MFA00000");
        assert_eq!(o.title, "F-150 · LARIAT");
        assert_eq!(o.stamp, "11/15/2026 2:05 PM");
        let titles: Vec<&str> = o.groups.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, ["Vehicle", "Paint, wheels & tires", "Electronics", "Other"]);
        assert_eq!(
            o.groups[1].fields[0],
            Field {
                label: "Paint Code".into(),
                value: "UM".into(),
                code: true
            }
        );
        assert_eq!(o.groups[2].fields[0].label, "SYNC Version");
        assert_eq!(o.groups[3].fields[0].label, "New Thing");

        assert!(o.fsa.found);
        assert_eq!(o.fsa.title, "OPEN FIELD SERVICE ACTIONS");
        assert_eq!(o.fsa.items.len(), 2);
        assert_eq!(
            (o.fsa.items[0].number.as_str(), o.fsa.items[0].description.as_str()),
            ("24S10", "Rear axle bolt")
        );
        assert_eq!(
            (o.fsa.items[1].number.as_str(), o.fsa.items[1].description.as_str()),
            ("23C01", "Backup camera")
        );
        assert_eq!(o.fsa.notices, [("Software repair steps".to_string(), "/notice1".to_string())]);

        assert_eq!(o.warnings.lines, ["Vehicle sold as is", "Odometer replaced"]);
        let w = &o.warranty;
        assert_eq!(w.start, NaiveDate::from_ymd_opt(2021, 3, 4));
        assert_eq!(w.build, NaiveDate::from_ymd_opt(2021, 2, 1));
        assert_eq!(w.months_in_service, Some(44));
        assert_eq!(w.coverages.len(), 2);
        assert_eq!(w.coverages[1].info, "Transferable");
    }

    #[test]
    fn checks_coverage() {
        assert_eq!(limits("3 years / 36,000 Miles"), (Some(36), Some(36000.0), false));
        assert_eq!(limits("36 Months/36,000 Miles"), (Some(36), Some(36000.0), false));
        assert_eq!(limits("8 years / unlimited miles"), (Some(96), None, true));
        assert_eq!(limits("See dealer"), (None, None, false));
        let (_, km, _) = limits("100,000 km");
        assert!((km.unwrap() - 62137.1).abs() < 1.0);

        let today = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        let start = NaiveDate::from_ymd_opt(2021, 3, 4);
        let c = |d: &str| Coverage {
            description: d.into(),
            ..Default::default()
        };
        let (s, t) = check(&c("3 years / 36,000 Miles"), start, Some(80000.0), today);
        assert_eq!(s, Standing::Expired);
        assert_eq!(t, "Out by time (ended Mar 4, 2024) · Out by mileage (44,000 mi over)");
        let (s, t) = check(&c("8 years / 100,000 miles"), start, Some(80000.0), today);
        assert_eq!(s, Standing::Covered);
        assert_eq!(t, "840 days left (until Mar 4, 2029) · 20,000 mi left");
        let (s, _) = check(&c("8 years / 100,000 miles"), start, None, today);
        assert_eq!(s, Standing::CheckMiles);
        assert_eq!(check(&c("whatever"), start, None, today).0, Standing::SeeTerms);
        assert_eq!(parse_date("Nov 15, 2026"), NaiveDate::from_ymd_opt(2026, 11, 15));
        assert_eq!(parse_date("11/15/2026"), NaiveDate::from_ymd_opt(2026, 11, 15));
        assert_eq!(parse_date(""), None);
        assert_eq!(thousands(-1234567), "-1,234,567");
    }
}
