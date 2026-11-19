//! The VIN in PTS (the Electron app's `manual.js`, steps 1 and 2):
//!
//! 1. `/VehicleId/DecodeVehicle?vin=…` → PTS's vehicle IDs, year, name and option codes ([`Vehicle`])
//! 2. `/publication/Book/workshop/{id}/{vin}` → the book's settings for that vehicle ([`Book`]: its `params` and the
//!    addresses of its other pages, `bases`). OASIS and the manual's pages are asked with these.
//!
//! Only what the app needs is kept: no warranty, owner or dealer details.

use std::collections::HashMap;

use serde_json::Value;
use workbench_net::{Http, Method};

use crate::{Error, Ford, Result};

/// What PTS says about a VIN
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Vehicle {
    pub vin: String,
    /// The book's vehicle IDs to try, numbers first (PTS's "MasterId" is the one that usually works)
    pub ids: Vec<String>,
    pub year: String,
    pub display_name: String,
    pub market_name: String,
    pub build_date: String,
    pub product_type: String,
    pub vehicle_line: String,
    /// The vehicle's option codes (engine, transmission…), for showing only what applies to it
    pub features: Vec<String>,
    pub minor_features: Vec<String>,
}

/// A book (the Workshop Manual, or PC/ED) for a vehicle: its settings, as Ford's book page sets them
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Book {
    pub vin: String,
    /// The book page's own address (Ford's pages are asked from it)
    pub url: String,
    /// "workshop" or "pced"
    pub kind: String,
    pub title: String,
    pub vehicle: Vehicle,
    /// The page's `var params = { … }`
    pub params: HashMap<String, String>,
    /// The addresses of the book's other pages (`var …Base = '…'`)
    pub bases: HashMap<String, String>,
}

impl Book {
    pub fn param(&self, k: &str) -> &str {
        self.params.get(k).map(String::as_str).unwrap_or("")
    }
}

fn text_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// PTS's answer to DecodeVehicle; none when it didn't know the VIN
pub fn read_decode(d: &Value, vin: &str) -> Option<Vehicle> {
    let info = d.get("DecodedInfo").cloned().unwrap_or(Value::Null);
    let year = text_of(info.get("Year"));
    if year.is_empty() || year == "0" {
        return None;
    }
    let mut ids: Vec<String> = [
        info.get("MasterId"),
        d.get("VehicleID"),
        d.get("VehicleId"),
        info.get("VehicleId"),
        info.get("vid"),
    ]
    .into_iter()
    .map(text_of)
    .filter(|x| !x.is_empty() && x != "0")
    .collect();
    let mut seen = Vec::new();
    ids.retain(|x| {
        let new = !seen.contains(x);
        seen.push(x.clone());
        new
    });
    // numbers first (the line codes like "CHD" rarely work)
    ids.sort_by_key(|x| !x.bytes().all(|b| b.is_ascii_digit()));
    let up = |k: &str| text_of(d.get(k)).to_uppercase();
    let features = [
        "ProductType",
        "VehicleLine",
        "Version",
        "BodyStyle",
        "Engine",
        "FuelType",
        "Transmission",
        "TransmissionInd",
        "Emissions",
        "TearTag",
        "Drive",
        "AxleRatio",
        "AirConditioning",
        "Tire",
        "Territory",
        "Paint",
        "IntColor",
        "IntFabric",
    ]
    .iter()
    .map(|k| up(k))
    .filter(|v| !v.is_empty())
    .collect();
    let minor_features = match d.get("MinorFeatures") {
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| text_of(Some(v)).to_uppercase())
            .filter(|v| !v.is_empty())
            .collect(),
        other => text_of(other)
            .split(|c: char| c == '|' || c == ',' || c.is_whitespace())
            .map(|v| v.trim().to_uppercase())
            .filter(|v| !v.is_empty())
            .collect(),
    };
    let display = text_of(info.get("DefaultDisplayName"));
    let market = text_of(info.get("MarketName"));
    Some(Vehicle {
        vin: {
            let v = text_of(d.get("Vin"));
            if v.is_empty() { vin.to_uppercase() } else { v.to_uppercase() }
        },
        ids,
        year,
        display_name: if display.is_empty() { market.clone() } else { display.clone() },
        market_name: if market.is_empty() { display } else { market },
        build_date: text_of(d.get("BuildDate")),
        product_type: text_of(d.get("ProductType")),
        vehicle_line: text_of(d.get("VehicleLine")),
        features,
        minor_features,
    })
}

/// The book page's settings: `var params = { key: 'value', … };` and `var …Base = '…';`
pub fn read_book_page(page: &str) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut params = HashMap::new();
    if let Some(start) = page.find("var params = {") {
        let block = &page[start..];
        let block = &block[..block.find("};").unwrap_or(block.len().min(20_000))];
        for line in block.lines() {
            let line = line.trim();
            let Some((key, rest)) = line.split_once(':') else { continue };
            let key = key.trim();
            if key.is_empty()
                || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                || key.starts_with(|c: char| c.is_ascii_digit())
            {
                continue;
            }
            let rest = rest.trim().trim_end_matches(',').trim_end();
            let Some(q) = rest.chars().next().filter(|c| *c == '\'' || *c == '"') else {
                continue;
            };
            if rest.len() >= 2 && rest.ends_with(q) {
                params.insert(key.to_string(), rest[1..rest.len() - 1].to_string());
            }
        }
    }
    let mut bases = HashMap::new();
    let mut rest = page;
    while let Some(at) = rest.find("var ") {
        rest = &rest[at + 4..];
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if !(name.ends_with("Base") || name.ends_with("BaseUrl")) {
            continue;
        }
        let after = rest[name.len()..].trim_start();
        let Some(after) = after.strip_prefix('=') else { continue };
        let Some(after) = after.trim_start().strip_prefix('\'') else {
            continue;
        };
        if let Some(end) = after.find('\'') {
            bases.insert(name, after[..end].to_string());
        }
    }
    (params, bases)
}

/// Form-style percent encoding (encodeURIComponent)
pub(crate) fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl Ford {
    /// What PTS says about a VIN
    pub async fn decode(&self, vin: &str) -> Result<Vehicle> {
        let url = self.url(&format!("/VehicleId/DecodeVehicle?vin={}", enc(vin)))?;
        let req = Http::request(Method::GET, url)
            .header("Accept", "application/json, text/javascript, */*; q=0.01")
            .header("X-Requested-With", "XMLHttpRequest");
        let res = self.fetch(req).await?;
        let d: Value = match serde_json::from_str(&res.body) {
            Ok(d) => d,
            Err(_) if crate::looks_like_sign_in(&res.body) || res.body.to_ascii_lowercase().contains("sign in") => {
                self.log("   signed out: the VIN lookup sent back a sign-in page");
                return Err(Error::NeedsLogin);
            }
            Err(_) => return Err(Error::Format("PTS didn't recognise this VIN".into())),
        };
        read_decode(&d, vin).ok_or_else(|| Error::Format("PTS didn't recognise this VIN".into()))
    }

    /// The Workshop Manual's settings for a VIN (read once, then remembered)
    pub async fn book(&self, vin: &str) -> Result<Book> {
        let vin = vin.trim().to_uppercase();
        if let Some(b) = self.books.lock().expect("books lock").get(&vin) {
            return Ok(b.clone());
        }
        // decoded once (the same VIN decodes the same; Ford takes a couple of seconds over it)
        let known = self.vehicles.lock().expect("vehicles lock").get(&vin).cloned();
        let v = match known {
            Some(v) => v,
            None => {
                let v = self.decode(&vin).await?;
                self.vehicles.lock().expect("vehicles lock").insert(vin.clone(), v.clone());
                v
            }
        };
        // PTS hasn't shown a vehicle in this profile yet (no PTSSession cookie): its book pages can only say
        // "Unavailable", so don't ask; PTS gets set up first
        if !self
            .http
            .cookie_sizes(&self.url("/publication/Book/workshop/")?)
            .iter()
            .any(|(n, _)| n == "PTSSession")
        {
            self.log("   no PTSSession cookie yet: PTS needs setting up first");
            return Err(Error::NotSetUp);
        }
        if v.ids.is_empty() {
            return Err(Error::Format("PTS didn't return a vehicle ID for this VIN".into()));
        }
        let mut last = Error::Format("No manual for this vehicle".into());
        for id in &v.ids {
            let url = self.url(&format!(
                "/publication/Book/workshop/{}/{}?model={}&modelyear={}",
                enc(id),
                enc(&vin),
                enc(&v.display_name),
                enc(&v.year)
            ))?;
            let page_url = url.to_string();
            match self.text(url).await {
                Ok(page) => {
                    let (params, bases) = read_book_page(&page);
                    if params.get("vehicleId").is_none_or(|x| x.is_empty()) || !bases.contains_key("publicationBookTreeAndCoverBase") {
                        // Ford's "Unavailable" page. In a profile PTS's own vehicle page hasn't set up yet it says
                        // "Object reference not set…" (or nothing useful): PTS needs setting up (the app does that)
                        let said = crate::error_page_says(&page);
                        self.log(&format!("   Ford's page said: {}", said.chars().take(240).collect::<String>()));
                        let sent = self.http.cookie_sizes(&self.url("/publication/Book/workshop/")?);
                        let missing = ["PTSSession", "FeaturesAttributes"]
                            .iter()
                            .any(|n| !sent.iter().any(|(c, _)| c == n));
                        last = if said.to_ascii_lowercase().contains("object reference not set") || missing {
                            Error::NotSetUp
                        } else {
                            Error::Format("No manual for this vehicle".into())
                        };
                        continue;
                    }
                    let book = Book {
                        vin: vin.clone(),
                        url: page_url.clone(),
                        kind: "workshop".into(),
                        title: params
                            .get("bookTitle")
                            .cloned()
                            .filter(|t| !t.is_empty())
                            .unwrap_or_else(|| format!("{} {}", v.year, v.market_name)),
                        vehicle: v.clone(),
                        params,
                        bases,
                    };
                    self.books.lock().expect("books lock").insert(vin.clone(), book.clone());
                    return Ok(book);
                }
                Err(Error::NeedsLogin) => return Err(Error::NeedsLogin),
                // (a vehicle ID that isn't a book's: what the others said counts more)
                Err(e) if matches!(last, Error::NotSetUp) => self.log(&format!("   vehicle ID {id}: {e}")),
                Err(e) => last = e,
            }
        }
        Err(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_vin_decode() {
        let d: Value = serde_json::from_str(
            r#"{"Vin":"1ftfw1e85mfa00000","VehicleID":"CHD","BuildDate":"2021-03-04","ProductType":"T","VehicleLine":"F15",
                "Engine":"99X","Transmission":"44T","MinorFeatures":"AB1|CD2, EF3",
                "DecodedInfo":{"Year":2021,"MasterId":12345,"MarketName":"F-150","DefaultDisplayName":"F-150 Lariat"}}"#,
        )
        .unwrap();
        let v = read_decode(&d, "x").unwrap();
        assert_eq!(v.vin, "1FTFW1E85MFA00000");
        assert_eq!(v.ids, ["12345", "CHD"]);
        assert_eq!(
            (v.year.as_str(), v.display_name.as_str(), v.market_name.as_str()),
            ("2021", "F-150 Lariat", "F-150")
        );
        assert_eq!(v.features, ["T", "F15", "99X", "44T"]);
        assert_eq!(v.minor_features, ["AB1", "CD2", "EF3"]);
        assert!(read_decode(&serde_json::from_str(r#"{"DecodedInfo":{"Year":0}}"#).unwrap(), "x").is_none());
    }

    #[test]
    fn reads_the_book_page() {
        let page = r#"<script>
            var params = {
                vehicleId: '12345',
                bookTitle: "2021 F-150 Workshop Manual",
                country : 'USA',
                weird: 'it's',
                count: 5,
            };
            var publicationBookTreeAndCoverBase = '/publication/TreeAndCover';
            var procBaseUrl = '/publication/proc';
            var other = 'x';
        </script>"#;
        let (params, bases) = read_book_page(page);
        assert_eq!(params["vehicleId"], "12345");
        assert_eq!(params["bookTitle"], "2021 F-150 Workshop Manual");
        assert_eq!(params["country"], "USA");
        assert_eq!(params["weird"], "it's");
        assert!(!params.contains_key("count"));
        assert_eq!(bases["publicationBookTreeAndCoverBase"], "/publication/TreeAndCover");
        assert_eq!(bases["procBaseUrl"], "/publication/proc");
        assert!(!bases.contains_key("other"));
        assert_eq!(enc("F-150 Lariat/x"), "F-150%20Lariat%2Fx");
    }
}
