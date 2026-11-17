//! Diagnostics: Ford's "Enhanced Search" for a vehicle (the Electron app's `diagSearch`). Type DTCs and/or words;
//! the results come back grouped as Ford's Diagnostics tab groups them: TSB / GSB / SSM, pinpoint tests, workshop
//! procedures, PC/ED, wiring, support videos. The same two requests Ford's tab makes:
//!   1. `/Diagnostics/UnifiedSearch?VIN=…`: the vehicle's article numbers and book codes (written into the page)
//!   2. `/Diagnostics/GetAllSearchResults` (POST): the results, and how many of each kind
//! Ford narrows them to the vehicle's options with the `FeaturesAttributes` cookie PTS sets for a VIN (only used when
//! it's this vehicle's: the same build date).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use workbench_net::{Http, Method};

use crate::doc::ProcLink;
use crate::html::{clean, decode};
use crate::{Error, Ford, Result};

/// The vehicle's side of the search, read from its diagnostics page (kept half an hour)
#[derive(Debug, Clone, PartialEq)]
pub struct DiagContext {
    pub vin: String,
    pub url: String,
    pub articles: Vec<String>,
    pub warranty: Vec<String>,
    pub wsm_books: Vec<String>,
    pub wiring_books: Vec<String>,
    pub pced_books: Vec<String>,
    pub build_date: String,
    pub vehicle_id: String,
    pub model_year: String,
    /// The article language (TSB / GSB / SSM); the Workshop Manual's (pinpoint tests, procedures), wiring's and PC/ED's
    /// (Ford's page sends each kind with its own)
    pub lang: String,
    pub wsm_lang: String,
    pub wiring_lang: String,
    pub pced_lang: String,
    pub country: String,
    pub content_market: String,
    pub iso_lang: String,
}

const KEEP: Duration = Duration::from_secs(30 * 60);

/// Every kind of result Ford's search has (PC/ED only for vehicles with a PC/ED book)
pub const SOURCES: [&str; 8] = ["tsb", "gsb", "ssm", "wsmppt", "wsmproc", "pced", "wiring", "tsv"];

/// What to search for
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiagRequest {
    pub text: String,
    pub dtcs: Vec<String>,
    /// The kinds wanted (none: all)
    pub sources: Vec<String>,
    pub rows: u32,
    pub start: u32,
    pub exact: bool,
}

/// A DTC Ford described for the search
#[derive(Debug, Clone, PartialEq)]
pub struct DtcInfo {
    pub name: String,
    pub description: String,
}

/// One result: its kind, title, what it's about, its date, whether it fits the vehicle's options, and where it goes
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DiagResult {
    /// "tsb", "gsb", "ssm", "wsmppt", "wsmproc", "pced", "wiring", "tsv"
    pub source: String,
    pub title: String,
    pub description: String,
    pub date: String,
    pub applies: bool,
    /// A pinpoint test or procedure: where it is in the manual (the test, for a pinpoint test)
    pub proc: Option<ProcLink>,
    /// A bulletin: its number
    pub article: String,
    /// A bulletin's, video's or wiring page's address
    pub url: String,
}

/// A page of results, and how many of each kind there are
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DiagPage {
    pub counts: BTreeMap<String, u64>,
    pub dtc_info: Vec<DtcInfo>,
    pub results: Vec<DiagResult>,
}

// ---------- what was typed ----------

/// "P0420", "U3000:49", "U3000:49:ABS", "B1A2B|87" (as Ford's search reads them)
fn is_dtc(t: &str) -> bool {
    let up = t.to_ascii_uppercase();
    let parts: Vec<&str> = up.split([':', '|']).collect();
    let code = |p: &str| {
        let b = p.as_bytes();
        (5..=7).contains(&b.len()) && b"PBUC".contains(&b[0]) && b[1..].iter().all(u8::is_ascii_hexdigit)
    };
    let hex2 = |p: &str| p.len() == 2 && p.bytes().all(|c| c.is_ascii_hexdigit());
    let module = |p: &str| (2..=12).contains(&p.len()) && p.bytes().all(|c| c.is_ascii_uppercase());
    match parts.as_slice() {
        [c] => code(c),
        [c, x] => code(c) && (hex2(x) || module(x)),
        [c, x, m] => code(c) && hex2(x) && module(m),
        _ => false,
    }
}

/// What was typed: its DTCs (each once, in capitals, "|" as ":") and the words left
pub fn parse_query(q: &str) -> (Vec<String>, String) {
    let mut dtcs: Vec<String> = Vec::new();
    let mut words: Vec<&str> = Vec::new();
    for tok in q
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter(|t| !t.is_empty())
    {
        if is_dtc(tok) {
            let d = tok.to_ascii_uppercase().replace('|', ":");
            if !dtcs.contains(&d) {
                dtcs.push(d);
            }
        } else {
            words.push(tok);
        }
    }
    (dtcs, words.join(" "))
}

// ---------- the vehicle's page ----------

fn decode_entities(t: &str) -> String {
    t.replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

/// `var Name = "…"` in the page
fn page_str(html: &str, name: &str) -> String {
    let mut rest = html;
    while let Some(at) = rest.find("var ") {
        rest = &rest[at + 4..];
        let r = rest.trim_start();
        let Some(after) = r.strip_prefix(name) else { continue };
        let after = after.trim_start();
        let Some(after) = after.strip_prefix('=') else { continue };
        let after = after.trim_start();
        if let Some(v) = after.strip_prefix('"') {
            if let Some(end) = v.find('"') {
                return decode_entities(&v[..end]);
            }
        }
    }
    String::new()
}

/// `var Name = DeserializeJSON("[&quot;…&quot;]")` (or with '…') in the page, anywhere on a line and over lines too;
/// the page also has commented-out versions of some of these lines, which are passed over (the last one that isn't
/// counts)
fn page_list(html: &str, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = html[from..].find("var") {
        let at = from + i;
        from = at + 3;
        // a whole word: "var" then a space
        if html[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            continue;
        }
        let rest = &html[at + 3..];
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        // commented out ("// var X = …" on its line)
        let line_start = html[..at].rfind('\n').map(|n| n + 1).unwrap_or(0);
        if html[line_start..at].contains("//") {
            continue;
        }
        let Some(after) = rest.trim_start().strip_prefix(name) else {
            continue;
        };
        let Some(after) = after.trim_start().strip_prefix('=') else {
            continue;
        };
        let Some(after) = after.trim_start().strip_prefix("DeserializeJSON(") else {
            continue;
        };
        let after = after.trim_start();
        let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            continue;
        };
        let body = &after[1..];
        let Some(end) = body.find(q) else { continue };
        if let Ok(Value::Array(a)) = serde_json::from_str::<Value>(&decode_entities(&body[..end])) {
            out = a
                .iter()
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect();
        }
    }
    out
}

/// `var VehicleID = 1234`
fn page_number(html: &str, name: &str) -> String {
    let mut rest = html;
    while let Some(at) = rest.find("var ") {
        rest = &rest[at + 4..];
        let Some(after) = rest.trim_start().strip_prefix(name) else {
            continue;
        };
        let Some(after) = after.trim_start().strip_prefix('=') else {
            continue;
        };
        let n: String = after.trim_start().chars().take_while(char::is_ascii_digit).collect();
        if !n.is_empty() {
            return n;
        }
    }
    String::new()
}

/// Strips tags and Ford's entities: plain words
fn plain(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut in_tag = false;
    for c in t.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    clean(&decode(&out))
}

fn s(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// One of Ford's results, as the app opens it (the Electron app's `diagResult`)
pub fn read_result(r: &Value, ctx: &DiagContext) -> DiagResult {
    let mut out = DiagResult {
        source: s(&r["source"]),
        title: plain(&s(&r["title"])),
        description: plain(&s(&r["description"])).chars().take(400).collect(),
        date: [s(&r["effectiveDate"]), s(&r["publicationDate"])]
            .into_iter()
            .find(|d| !d.is_empty())
            .unwrap_or_default(),
        applies: r["qsMatched"] != Value::Bool(false),
        ..Default::default()
    };
    match out.source.as_str() {
        "wsmppt" | "wsmproc" | "pced" => {
            // as Ford's page works it out: GSI books → procUid = uid, searchNumber = procuid; ODY books ("G…" procuid)
            // the other way round
            let (uid, procuid) = (s(&r["uid"]), s(&r["procuid"]));
            let (mut proc_uid, mut search_number) = (uid.clone(), procuid.clone());
            if procuid.to_ascii_lowercase().starts_with('g') {
                proc_uid = procuid.clone();
                search_number = uid.clone();
            }
            if out.source == "pced" && uid.is_empty() {
                proc_uid = procuid.clone();
                if procuid.contains("_s4p") || procuid.contains("_s5p") {
                    search_number = format!("spapced_{procuid}");
                }
            }
            if let Some(bar) = out.title.find(" |") {
                let after = out.title[bar + 2..].to_string();
                out.title = clean(&format!("{} {after}", &out.title[..bar]));
            }
            out.proc = Some(ProcLink {
                search_number,
                proc_uid,
                link_type: "Procedure".into(),
                hash: String::new(),
                ppt: s(&r["pptid"]),
            });
        }
        "tsb" | "gsb" | "ssm" => {
            out.article = s(&r["articleNumber"]);
            out.url = format!(
                "{}/vdirsnet/TSB_SSM/DisplayArticle.aspx?content=/vdirsnet/TSB_SSM/DisplayArticle.aspx&id={}&lang={}&country={}&market={}",
                crate::paths::PTS,
                crate::vehicle::enc(&out.article),
                crate::vehicle::enc(if ctx.iso_lang.is_empty() { "EN-US" } else { &ctx.iso_lang }),
                crate::vehicle::enc(if ctx.country.is_empty() { "USA" } else { &ctx.country }),
                crate::vehicle::enc(if ctx.content_market.is_empty() { "US" } else { &ctx.content_market }),
            );
        }
        "tsv" => {
            let u = s(&r["uid"]);
            if u.to_ascii_lowercase().starts_with("https:") {
                out.url = u;
            }
        }
        "wiring" => {
            // the same wiring addresses Ford's search builds (opened on Ford's site until the Wiring tab is here)
            let (book, conn, kind) = (s(&r["articleNumber"]), s(&r["wiringConnector"]), s(&r["type"]));
            let lang = if ctx.lang.is_empty() { "enusa" } else { ctx.lang.as_str() };
            let vid = crate::vehicle::enc(if ctx.vehicle_id.is_empty() { "1" } else { &ctx.vehicle_id });
            let page = s(&r["wiringPage"]);
            let e = crate::vehicle::enc;
            if kind == "cfc" {
                out.title = format!("{conn}: {}", out.title);
                out.url = format!(
                    "{}/wiring/face/?book={}&vehicleId={vid}&cell=150&page=&item={}&bookType=svg&languageCode={lang}",
                    crate::paths::PTS,
                    e(&book),
                    e(&format!("{}cf{}", book.to_lowercase(), conn.to_lowercase()))
                );
            } else {
                let mut cell = s(&r["wiringCell"]);
                let mut base = "/wiring/page";
                match kind.as_str() {
                    "celttl" => {
                        if cell == "150" {
                            base = "/wiring/connectors";
                        }
                        out.title = format!("{cell} {}", out.title);
                    }
                    "component_search" => out.title = format!("{}: {cell}-{page}", out.title),
                    "conn_search" => out.title = format!("{conn}: {cell}-{page} {}", out.title),
                    k if k.ends_with("_index") => cell = "151".into(),
                    _ => {}
                }
                out.url = format!(
                    "{}{base}?book={}&vehicleId={vid}&cell={}&page={}&bookType=svg&languageCode={lang}&wiringBookCode={}&title={}{}",
                    crate::paths::PTS,
                    e(&book),
                    e(&cell),
                    e(if page.is_empty() { "1" } else { &page }),
                    e(&ctx.wiring_books.join(",")),
                    e(&out.title),
                    if conn.is_empty() {
                        String::new()
                    } else {
                        format!("&item={}", e(&conn))
                    }
                );
            }
            out.url.push_str("&comefrom=enhancesearch");
            out.description.clear();
        }
        _ => {}
    }
    out
}

/// What Ford's search answered: how many of each kind, its DTCs' descriptions, and the results
pub fn read_page(json: &Value, ctx: &DiagContext) -> DiagPage {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for x in json["filter"]["source"].as_array().into_iter().flatten() {
        *counts.entry(s(&x["name"])).or_default() += x["count"].as_u64().or_else(|| s(&x["count"]).parse().ok()).unwrap_or(0);
    }
    let dtc_info = json["filter"]["dtcInfo"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|x| DtcInfo {
            name: s(&x["name"]),
            description: plain(
                &[s(&x["description"]), s(&x["desc"])]
                    .into_iter()
                    .find(|d| !d.is_empty())
                    .unwrap_or_default(),
            ),
        })
        .collect();
    let results = json["data"].as_array().into_iter().flatten().map(|r| read_result(r, ctx)).collect();
    DiagPage { counts, dtc_info, results }
}

/// A date as its year, month and day, however it's written: "20230322", "2023-03-22", "3/22/2023"
fn ymd(s: &str) -> Option<(u32, u32, u32)> {
    let groups: Vec<&str> = s.split(|c: char| !c.is_ascii_digit()).filter(|g| !g.is_empty()).collect();
    let n = |g: &str| g.parse::<u32>().ok();
    match groups.as_slice() {
        [one, ..] if one.len() == 8 => Some((n(&one[..4])?, n(&one[4..6])?, n(&one[6..])?)),
        [y, m, d, ..] if y.len() == 4 => Some((n(y)?, n(m)?, n(d)?)),
        [m, d, y, ..] if y.len() == 4 => Some((n(y)?, n(m)?, n(d)?)),
        _ => None,
    }
}

/// The same build date (written the same way, or not)
fn same_day(a: &str, b: &str) -> bool {
    a.trim() == b.trim() || matches!((ymd(a), ymd(b)), (Some(x), Some(y)) if x == y)
}

/// The vehicle's options from PTS's `FeaturesAttributes` cookie (only when it's this vehicle's: the same build date)
fn feature_attributes(raw: &str, build_date: &str) -> Option<(Vec<String>, Option<String>, String)> {
    let raw = crate::doc::percent_decode(raw);
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    for pair in raw.split('&') {
        let Some((k, v)) = pair.split_once('=') else { continue };
        if !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    let bd = map.get("BuildDate")?.clone();
    if !same_day(&bd, build_date) {
        return None;
    }
    let list = |k: &str| -> Vec<String> {
        map.get(k)
            .map(|v| v.split('|').filter(|x| !x.is_empty()).map(str::to_string).collect())
            .unwrap_or_default()
    };
    let mut codes = list("PrimaryFeatureCodes");
    codes.extend(list("MinorFeatureCodes"));
    codes.sort();
    codes.truncate(300);
    let line = map
        .get("VehicleLine")
        .filter(|v| !v.is_empty())
        .map(|v| format!("{}{}", v.to_lowercase(), map.get("VehicleType").cloned().unwrap_or_default()));
    Some((codes, line, bd))
}

/// Each kind's language, as Ford's page sends them (TSB / GSB / SSM: the article language; videos: always enusa)
fn source_lang<'a>(ctx: &'a DiagContext, kind: &str) -> &'a str {
    match kind {
        "wsmppt" | "wsmproc" => &ctx.wsm_lang,
        "wiring" => &ctx.wiring_lang,
        "pced" => &ctx.pced_lang,
        "tsv" => "enusa",
        _ => &ctx.lang,
    }
}

impl Ford {
    /// This vehicle's options for the search (PTS's `FeaturesAttributes` cookie). PTS keeps the cookie for the
    /// vehicle it identified last, and its own page keeps a copy for each VIN to put back when you switch vehicles:
    /// so does this. The cookie as it is when it's this vehicle's; else this VIN's copy; else the VIN identified again
    /// (as PTS does), and the cookie read then. Values are never logged.
    async fn vehicle_options(&self, vin: &str, build_date: &str) -> Option<(Vec<String>, Option<String>, String)> {
        let now = || self.http.cookie(&self.base, "FeaturesAttributes");
        if let Some(raw) = now() {
            if let Some(f) = feature_attributes(&raw, build_date) {
                self.features
                    .lock()
                    .expect("features lock")
                    .insert(vin.to_string(), (Instant::now(), Some(raw)));
                return Some(f);
            }
        }
        let kept = self.features.lock().expect("features lock").get(vin).cloned();
        match kept {
            Some((_, Some(raw))) => return feature_attributes(&raw, build_date),
            // tried lately, and PTS had none for it
            Some((at, None)) if at.elapsed() < KEEP => return None,
            _ => {}
        }
        // identified again: PTS sets the cookie for this VIN
        let _ = self.decode(vin).await;
        let found = now().filter(|raw| feature_attributes(raw, build_date).is_some());
        self.log(if found.is_some() {
            "   diagnostics: the vehicle's options came with identifying it again"
        } else {
            "   diagnostics: no vehicle options from PTS for this VIN (searched without them)"
        });
        self.features
            .lock()
            .expect("features lock")
            .insert(vin.to_string(), (Instant::now(), found.clone()));
        feature_attributes(&found?, build_date)
    }

    /// The vehicle's side of the search (its diagnostics page), kept half an hour
    pub async fn diag_context(&self, vin: &str) -> Result<DiagContext> {
        let book = self.book(vin).await?;
        if let Some((at, c)) = self.diag.lock().expect("diag lock").get(&book.vin) {
            if at.elapsed() < KEEP {
                return Ok(c.clone());
            }
        }
        let v = &book.vehicle;
        let year = if v.year.is_empty() {
            book.param("modelYear").to_string()
        } else {
            v.year.clone()
        };
        let e = crate::vehicle::enc;
        let url = format!(
            "{}/Diagnostics/UnifiedSearch?VIN={}&VID={}&BuildDate={}&ModelYear={}",
            crate::paths::PTS,
            e(&book.vin),
            e(book.param("vehicleId")),
            e(&v.build_date),
            e(&year)
        );
        let html = self.text(self.url(&url)?).await?;
        if !["GetAllSearchResults", "UnifiedSearchBar", "ArticleNumbers", "var ESVIN"]
            .iter()
            .any(|w| html.contains(w))
        {
            return Err(Error::Format("Ford's diagnostics search didn't load for this vehicle".into()));
        }
        let lang = [page_str(&html, "ArticleLanguage"), page_str(&html, "LanguageOdysseyCode")]
            .into_iter()
            .find(|x| !x.is_empty())
            .unwrap_or_else(|| "ENUSA".into())
            .to_lowercase();
        let or = |a: String, b: &str| if a.is_empty() { b.to_string() } else { a };
        let ctx = DiagContext {
            vin: book.vin.clone(),
            url,
            articles: page_list(&html, "ArticleNumbers"),
            warranty: page_list(&html, "WarrantyArticleNumbers"),
            wsm_books: page_list(&html, "WSMBookCode"),
            wiring_books: page_list(&html, "WiringBookCode"),
            pced_books: page_list(&html, "PCEDBookCodes"),
            build_date: or(page_str(&html, "BuildDate"), &v.build_date),
            vehicle_id: or(page_number(&html, "VehicleID"), book.param("vehicleId")),
            model_year: or(page_str(&html, "ModelYear"), &year),
            wsm_lang: or(page_str(&html, "WSMLanguage").to_lowercase(), &lang),
            wiring_lang: or(page_str(&html, "WiringLanguage").to_lowercase(), &lang),
            pced_lang: or(page_str(&html, "PCEDLanguage").to_lowercase(), &lang),
            lang,
            country: or(
                page_str(&html, "Country"),
                if book.param("country").is_empty() {
                    "USA"
                } else {
                    book.param("country")
                },
            ),
            content_market: or(page_str(&html, "ContentMarket"), "US"),
            iso_lang: or(page_str(&html, "ISOLanguageCode"), "EN-US"),
        };
        self.log(&format!(
            "   diagnostics: {} articles, WSM {}, wiring {}, PC/ED {}",
            ctx.articles.len(),
            ctx.wsm_books.join(","),
            ctx.wiring_books.join(","),
            ctx.pced_books.join(",")
        ));
        self.diag
            .lock()
            .expect("diag lock")
            .insert(book.vin.clone(), (Instant::now(), ctx.clone()));
        Ok(ctx)
    }

    /// Ford's search for a vehicle: a page of results (of the kinds asked for), and how many of each kind there are
    pub async fn diag_search(&self, vin: &str, req: &DiagRequest) -> Result<DiagPage> {
        let ctx = self.diag_context(vin).await?;
        let book = self.book(vin).await?;
        let all: Vec<&str> = SOURCES
            .iter()
            .copied()
            .filter(|t| *t != "pced" || !ctx.pced_books.is_empty())
            .collect();
        let wanted: Vec<&str> = if req.sources.is_empty() {
            all.clone()
        } else {
            all.iter().copied().filter(|t| req.sources.iter().any(|w| w == t)).collect()
        };
        let mut articles: Vec<String> = Vec::new();
        for a in ctx
            .articles
            .iter()
            .chain(&ctx.wsm_books)
            .chain(&ctx.pced_books)
            .chain(&ctx.wiring_books)
            .chain(&ctx.warranty)
        {
            let a = a.replace('/', "");
            if !a.is_empty() && !articles.contains(&a) {
                articles.push(a);
            }
        }
        // the market name ("F-150"), as Ford's page sends it: Ford narrows the Workshop Manual's results (pinpoint
        // tests, procedures) by it
        let name = [&book.vehicle.market_name, &book.vehicle.display_name]
            .into_iter()
            .find(|n| !n.is_empty())
            .cloned()
            .unwrap_or_default();
        let mut params = json!({
            "searchTerm": req.text.trim(),
            "articleNumber": articles,
            "dtc": req.dtcs,
            "source": wanted.iter().map(|t| json!({ "language": source_lang(&ctx, t), "type": t })).collect::<Vec<_>>(),
            "start": req.start,
            "rows": req.rows.max(1),
            "descriptionFlag": true,
            "symptomCodes": [],
            "modelAndYear": [format!("{} {name}", ctx.model_year).trim()],
        });
        if req.exact {
            params["searchType"] = json!("exact");
        }
        if let Some((codes, line, bd)) = self.vehicle_options(&book.vin, &ctx.build_date).await {
            params["featureCodes"] = json!(codes);
            if let Some(l) = line {
                params["vehicleLine"] = json!([l]);
            }
            params["buildDate"] = json!(bd);
        }
        let body = format!("searchRequest={}", crate::vehicle::enc(&params.to_string()));
        let post = Http::request(Method::POST, self.url("/Diagnostics/GetAllSearchResults")?)
            .header("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")
            .header("Accept", "application/json, text/javascript, */*; q=0.01")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", ctx.url.clone())
            .body(body);
        let res = self.fetch(post).await?;
        let json: Value = match serde_json::from_str(&res.body) {
            Ok(v) => v,
            Err(_) if crate::looks_like_sign_in(&res.body) => return Err(Error::NeedsLogin),
            Err(_) => return Err(Error::Format("Ford's diagnostics search didn't answer".into())),
        };
        let page = read_page(&json, &ctx);
        self.log(&format!(
            "   diagnostics search: {} DTCs, {} of {} results",
            req.dtcs.len(),
            page.results.len(),
            page.counts.values().sum::<u64>()
        ));
        Ok(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_was_typed() {
        let (d, t) = parse_query("p0420, U3000:49 no crank B1A2B|87 U0100:00:PCM U0100:ABS liftgate");
        assert_eq!(d, ["P0420", "U3000:49", "B1A2B:87", "U0100:00:PCM", "U0100:ABS"]);
        assert_eq!(t, "no crank liftgate");
        let (d, t) = parse_query("P042 X0420 P04200000");
        assert!(d.is_empty());
        assert_eq!(t, "P042 X0420 P04200000");
    }

    const PAGE: &str = r#"<script>
        var ESVIN = "1FT";
        var ArticleLanguage = "ENUSA";
        //var ArticleNumbers = DeserializeJSON("[&quot;OLD&quot;]");
        var ArticleNumbers = DeserializeJSON("[&quot;S9635&quot;,&quot;TSB/1&quot;]");
        var WSMBookCode = DeserializeJSON('["S9635"]');
        var VehicleID = 6925;
        var BuildDate = "2022-11-03";
    </script>"#;

    #[test]
    fn reads_the_vehicles_page() {
        assert_eq!(page_list(PAGE, "ArticleNumbers"), ["S9635", "TSB/1"]);
        assert_eq!(page_list(PAGE, "WSMBookCode"), ["S9635"]);
        assert!(page_list(PAGE, "PCEDBookCodes").is_empty());
        assert_eq!(page_str(PAGE, "ArticleLanguage"), "ENUSA");
        assert_eq!(page_str(PAGE, "BuildDate"), "2022-11-03");
        assert_eq!(page_number(PAGE, "VehicleID"), "6925");
    }

    #[test]
    fn reads_lists_anywhere_on_a_line() {
        let page = "<script>var ESVIN = \"1FT\"; var WSMBookCode = DeserializeJSON('[\"S9635\"]'); var WiringBookCode = DeserializeJSON(\n  \"[&quot;EQD&quot;]\");\n// var WSMBookCode = DeserializeJSON('[\"OLD\"]');\nvar novar = 1;</script>";
        assert_eq!(page_list(page, "WSMBookCode"), ["S9635"]);
        assert_eq!(page_list(page, "WiringBookCode"), ["EQD"]);
    }

    fn ctx() -> DiagContext {
        DiagContext {
            vin: "1FT".into(),
            url: String::new(),
            articles: vec![],
            warranty: vec![],
            wsm_books: vec![],
            wiring_books: vec!["EQD".into()],
            pced_books: vec![],
            build_date: String::new(),
            vehicle_id: "6925".into(),
            model_year: "2023".into(),
            lang: "enusa".into(),
            country: "USA".into(),
            content_market: "US".into(),
            iso_lang: "EN-US".into(),
        }
    }

    #[test]
    fn reads_results() {
        let json: Value = serde_json::from_str(
            r#"{"data":[
                {"source":"wsmppt","title":"PINPOINT TEST A: NO CRANK | 303-06","uid":"G2198756","procuid":"G2198755","pptid":"A","qsMatched":true},
                {"source":"tsb","title":"<b>Engine</b> noise &amp; vibration","articleNumber":"23-2001","effectiveDate":"2023-02-01","description":"Some vehicles","qsMatched":false},
                {"source":"wsmproc","title":"Starter","uid":"S1","procuid":"P1"}
              ],
              "filter":{"source":[{"name":"tsb","count":3},{"name":"wsmppt","count":"2"}],"dtcInfo":[{"name":"P0420","desc":"Catalyst <i>efficiency</i>"}]}}"#,
        )
        .unwrap();
        let p = read_page(&json, &ctx());
        assert_eq!(p.counts["tsb"], 3);
        assert_eq!(p.counts["wsmppt"], 2);
        assert_eq!(p.dtc_info[0].description, "Catalyst efficiency");
        // ODY books: the G… procuid is the procedure; the pinpoint test's letter goes with it
        let a = p.results[0].proc.clone().unwrap();
        assert_eq!(
            (a.proc_uid.as_str(), a.search_number.as_str(), a.ppt.as_str()),
            ("G2198755", "G2198756", "A")
        );
        assert_eq!(p.results[0].title, "PINPOINT TEST A: NO CRANK 303-06");
        let t = &p.results[1];
        assert_eq!(
            (t.title.as_str(), t.article.as_str(), t.applies),
            ("Engine noise & vibration", "23-2001", false)
        );
        assert!(t.url.contains("id=23-2001"));
        // GSI books the other way round
        let w = p.results[2].proc.clone().unwrap();
        assert_eq!((w.proc_uid.as_str(), w.search_number.as_str()), ("S1", "P1"));
    }

    #[test]
    fn build_dates_written_any_way() {
        assert!(same_day("20230322", "2023-03-22"));
        assert!(same_day("3/22/2023", "20230322"));
        assert!(same_day("2023-03-22T00:00:00", "03/22/2023"));
        assert!(!same_day("20230322", "20230323"));
        assert!(!same_day("", "20230322"));
    }

    #[test]
    fn reads_the_vehicles_options() {
        let raw = "BuildDate%3D2022-11-03%26PrimaryFeatureCodes%3DB%7CA%26MinorFeatureCodes%3DC%26VehicleLine%3DU7%26VehicleType%3DX";
        let (codes, line, bd) = feature_attributes(raw, "2022-11-03").unwrap();
        assert_eq!(codes, ["A", "B", "C"]);
        assert_eq!(line.as_deref(), Some("u7X"));
        assert_eq!(bd, "2022-11-03");
        assert!(feature_attributes(raw, "2020-01-01").is_none());
    }
}
