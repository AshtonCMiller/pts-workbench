//! Wiring diagrams: Ford's wiring book for a vehicle, the way PTS's /wiring pages load it (the Electron app's
//! `wiringInfo` … `wiringLocations`):
//!
//!   /wiring/TableOfContents?…           the book's settings, written into the page (its release, vehicle ID, title)
//!   …/wiring/TableofContent (JSON)      the diagrams ("cells"): { Title, Type: Page | Connectors | LocIndex, Number,
//!                                       Code: [option codes] }
//!   …/wiring/PageList (JSON)            one diagram's pages: { page, Code, Startdate, Enddate }
//!   …/wiring/svg/{release}/{vehicle}/~W{book}/{lang}/svg/{cell}/0/{page}.svg   one page's drawing
//!   …/wiring/GetLocationComponentType   where a connector / splice / ground is (the location views, cell 151)
//!
//! The JSON and drawings come from Ford's document server (fordservicecontent.com), which wants its own sign-on
//! ([`Ford::content_sign_on`]). A release's drawings don't change, so each is kept on disk under `wiring/<release>/`
//! (older releases are removed). [`prepare`] makes a drawing ready to draw: Ford's scripts and notes out, its sheet
//! shown, its links and texts named so they can be found on the drawing, its pictures put inside it.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::Value;
use workbench_net::{Http, Method};

use crate::html::decode;
use crate::vehicle::enc;
use crate::{Error, Ford, Result};

const PTS: &str = crate::paths::PTS;
const CONTENT: &str = "https://www.fordservicecontent.com/Ford_Content/PublicationRuntimeRefreshPTS/";

/// A vehicle's wiring book, as its contents page sets it up
#[derive(Debug, Clone, PartialEq)]
pub struct Info {
    pub vin: String,
    pub book: String,
    pub book_type: String,
    pub vehicle_id: String,
    pub lang: String,
    /// Ford's release ("prod_1_3_11112026"): a release's files don't change
    pub env: String,
    /// Ford's document server ("https://www.fordservicecontent.com/Ford_Content/PublicationRuntimeRefreshPTS/")
    pub base: String,
    pub title: String,
}

/// One diagram in the book
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cell {
    /// Its number ("12"), and as Ford wrote it ("012")
    pub number: String,
    pub raw: String,
    pub title: String,
    /// "Page" (a diagram drawn here), "Connectors" (the connector list), "LocIndex" (the location index): the last
    /// two are Ford's own pages
    pub kind: String,
    /// The options it's for (none: every vehicle)
    pub codes: Vec<String>,
}

impl Cell {
    pub fn drawn(&self) -> bool {
        self.kind.is_empty() || self.kind == "Page"
    }
}

/// One page of a diagram
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PageInfo {
    pub page: u32,
    /// Which options it's for: "(A|B)+(C)" (each group needs one of its codes; "!X": not X)
    pub codes: String,
    /// Build dates it's for ("2023-01-15"; empty: any)
    pub start: String,
    pub end: String,
}

/// The vehicle's options, for what doesn't apply to it
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Features {
    /// Primary and minor option codes (upper case)
    pub codes: Vec<String>,
    /// "u7252" (Ford's vehicle line and type)
    pub line: Vec<String>,
    /// "20230115"
    pub build: String,
}

/// The book's contents
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Toc {
    pub title: String,
    pub cells: Vec<Cell>,
    /// The vehicle's options (none: nothing can be grayed)
    pub features: Option<Features>,
}

/// Where a connector, splice or ground is: a page of the location views (cell 151)
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Location {
    pub page: u32,
    pub qual: String,
    pub desc: String,
}

/// What a search result is
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HitKind {
    Component,
    Connector,
    Splice,
    Ground,
    Diagram,
    Other,
}

impl HitKind {
    pub fn label(self) -> &'static str {
        match self {
            HitKind::Component => "Component",
            HitKind::Connector => "Connector",
            HitKind::Splice => "Splice",
            HitKind::Ground => "Ground",
            HitKind::Diagram => "Diagram",
            HitKind::Other => "Wiring",
        }
    }
}

/// A search result: what it is, and where it goes (a page drawn here, or Ford's page)
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub kind: HitKind,
    pub title: String,
    /// Its diagram and page ("" when it's one of Ford's own pages: a connector face, the connector list)
    pub cell: String,
    pub page: u32,
    /// The item to mark on the page ("C1689", or a component's name)
    pub item: String,
    pub applies: bool,
    /// Ford's page for it
    pub url: String,
}

/// A link in a drawing
#[derive(Debug, Clone, PartialEq)]
pub enum Link {
    /// Another page ("151-2"), with the item to mark there
    Page {
        cell: String,
        page: u32,
        item: String,
        title: String,
    },
    /// A connector, splice or ground ("C1689"), with its connector face's name ("EQDCFC1689")
    Item { name: String, face: String },
    /// One of Ford's pages
    Url(String),
}

/// A drawing made ready to draw (see [`prepare`])
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Prepared {
    pub svg: String,
    /// Its size (Ford's view box)
    pub width: f32,
    pub height: f32,
    /// Each link: its group's ID in the drawing, the IDs of the texts in it, and where it goes
    pub links: Vec<(String, Vec<String>, Link)>,
    /// Each text: its ID in the drawing, and what it says
    pub texts: Vec<(String, String)>,
}

/// "012" → "12"
pub fn cell_no(c: &str) -> String {
    let digits: String = c.chars().filter(char::is_ascii_digit).collect();
    digits.parse::<u32>().map(|n| n.to_string()).unwrap_or_default()
}

/// Whether a diagram is for this vehicle (as Ford's wiring pages work it out)
pub fn cell_applies(c: &Cell, f: Option<&Features>) -> bool {
    let Some(f) = f else { return true };
    if c.codes.is_empty() {
        return true;
    }
    c.codes.iter().any(|code| {
        let code = code.replacen('-', "", 1).replacen('_', "0", 1).to_uppercase();
        f.codes.contains(&code)
    })
}

/// Whether a page is for this vehicle: its build dates, and its option groups
pub fn page_applies(p: &PageInfo, f: Option<&Features>) -> bool {
    let Some(f) = f else { return true };
    let day = |v: &str| -> Option<u64> {
        let d: String = v.chars().filter(char::is_ascii_digit).collect();
        d.get(..8).and_then(|d| d.parse().ok())
    };
    if let Some(build) = day(&f.build) {
        if day(&p.start).is_some_and(|s| s > build) || day(&p.end).is_some_and(|e| e < build) {
            return false;
        }
    }
    let groups: String = p.codes.chars().filter(|c| *c != '(' && *c != ')').collect();
    if groups.trim().is_empty() {
        return true;
    }
    let has = |q: &str| f.codes.iter().any(|c| c == q) || f.line.iter().any(|l| l.eq_ignore_ascii_case(q));
    groups.split('+').all(|g| {
        g.to_uppercase().split('|').any(|q| match q.strip_prefix('!') {
            Some(not) => !has(not),
            None => has(q),
        })
    })
}

fn s(v: &Value) -> String {
    match v {
        Value::String(t) => t.trim().to_string(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn num(v: &Value) -> u32 {
    s(v).parse::<f64>().map(|n| n as u32).unwrap_or(0)
}

/// Ford's contents list
pub fn read_toc(v: &Value) -> Vec<Cell> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter(|it| !it["Number"].is_null())
        .map(|it| Cell {
            number: cell_no(&s(&it["Number"])),
            raw: s(&it["Number"]),
            title: clean(&s(&it["Title"])),
            kind: s(&it["Type"]),
            codes: it["Code"]
                .as_array()
                .map(|a| a.iter().map(s).filter(|c| !c.is_empty()).collect())
                .unwrap_or_default(),
        })
        .filter(|c| !c.number.is_empty())
        .collect()
}

/// A diagram's page list
pub fn read_pages(v: &Value) -> Vec<PageInfo> {
    let mut out: Vec<PageInfo> = v
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| PageInfo {
            page: num(&p["page"]),
            // (as the page's script reads it: the list's items joined)
            codes: match &p["Code"] {
                Value::Array(a) => a.iter().map(s).collect::<Vec<_>>().join(","),
                other => s(other),
            },
            start: s(&p["Startdate"]),
            end: s(&p["Enddate"]),
        })
        .filter(|p| p.page > 0)
        .collect();
    out.sort_by_key(|p| p.page);
    out.dedup_by_key(|p| p.page);
    out
}

/// Where an item is (the location views)
pub fn read_locations(v: &Value) -> Vec<Location> {
    v.as_array()
        .into_iter()
        .flatten()
        .map(|l| Location {
            page: num(&l["Page"]),
            qual: clean(&s(&l["Qual"])),
            desc: clean(&s(&l["LocationDesc"])),
        })
        .filter(|l| l.page > 0)
        .collect()
}

/// Spaces tidied (Ford's titles have non-breaking spaces and runs of them)
pub fn clean(t: &str) -> String {
    t.replace('\u{a0}', " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A value in the contents page's script: the first thing after `before` up to `until`
fn between<'a>(html: &'a str, before: &str, until: &[char]) -> Option<&'a str> {
    let i = html.find(before)? + before.len();
    let rest = &html[i..];
    let end = rest.find(|c| until.contains(&c)).unwrap_or(rest.len());
    Some(rest[..end].trim()).filter(|v| !v.is_empty())
}

/// The book's settings, from its contents page (falling back on what PTS's book said)
pub fn read_info(html: &str, vin: &str, code: &str, params: &HashMap<String, String>, title: &str) -> Info {
    let p = |k: &str| params.get(k).map(String::as_str).unwrap_or("");
    // book: 'EQD'  ·  bookType: 'svg'  ·  languageCode: 'ENUSA' (the first place it's set like that)
    let quoted = |key: &str| -> Option<String> {
        let mut from = 0;
        while let Some(i) = html[from..].find(key).map(|n| n + from) {
            from = i + key.len();
            if html[..i].chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric()) {
                continue;
            }
            let Some(rest) = html[from..].trim_start().strip_prefix(':') else {
                continue;
            };
            let Some(rest) = rest.trim_start().strip_prefix('\'') else {
                continue;
            };
            if let Some(end) = rest.find('\'') {
                return Some(rest[..end].to_string()).filter(|v| !v.is_empty());
            }
        }
        None
    };
    // vehicleId=' + '6925  ·  booktitle=' + '2023 Bronco Sport' (written into the page's links)
    let joined = |key: &str| -> Option<String> {
        let mut from = 0;
        while let Some(i) = html[from..].find(key).map(|n| n + from) {
            from = i + key.len();
            let rest = html[from..].trim_start_matches(['\'', '"', ' ']);
            let Some(rest) = rest.strip_prefix('+') else { continue };
            let rest = rest.trim_start_matches(['\'', '"', ' ']);
            let end = rest.find(['\'', '"', '&']).unwrap_or(rest.len());
            return Some(rest[..end].trim().to_string()).filter(|v| !v.is_empty());
        }
        None
    };
    let toc_api = html
        .find("/wiring/TableofContent?environment=")
        .or_else(|| html.to_ascii_lowercase().find("/wiring/tableofcontent?environment="));
    let (env, base) = match toc_api {
        Some(i) => {
            let start = html[..i].rfind(['"', '\'']).map(|q| q + 1).unwrap_or(0);
            let base = html[start..i].trim_end_matches('/').to_string() + "/";
            let env = between(&html[i..], "environment=", &['"', '\'', '&', ' '])
                .unwrap_or("")
                .to_string();
            (env, base)
        }
        None => (p("environment").to_string(), CONTENT.to_string()),
    };
    let base = if base.starts_with("https://") && base.to_ascii_lowercase().contains("fordservicecontent.com/") {
        base
    } else {
        CONTENT.to_string()
    };
    let vehicle_id = joined("vehicleId=")
        .filter(|v| v.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or_else(|| p("vehicleId").to_string());
    Info {
        vin: vin.to_string(),
        book: quoted("book")
            .filter(|b| b.chars().all(|c| c.is_ascii_alphanumeric()))
            .unwrap_or_else(|| code.to_string()),
        book_type: quoted("bookType")
            .unwrap_or_else(|| {
                if p("WiringFormat").is_empty() {
                    "svg".into()
                } else {
                    p("WiringFormat").to_string()
                }
            })
            .to_lowercase(),
        vehicle_id,
        lang: quoted("languageCode").unwrap_or_else(|| "ENUSA".into()),
        env,
        base,
        title: joined("booktitle=")
            .or_else(|| Some(p("WiringBookTitle").to_string()).filter(|t| !t.is_empty()))
            .unwrap_or_else(|| title.to_string()),
    }
}

// ---------- making a drawing ready ----------

/// A tag's attributes ("name" → value, names in lower case)
fn attrs(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let b = tag.as_bytes();
    // past the tag's name
    let mut i = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
    while i < b.len() {
        while i < b.len() && (b[i] as char).is_whitespace() {
            i += 1;
        }
        let start = i;
        while i < b.len() && !(b[i] as char).is_whitespace() && b[i] != b'=' && b[i] != b'>' && b[i] != b'/' {
            i += 1;
        }
        let name = tag[start..i].to_ascii_lowercase();
        if name.is_empty() {
            i += 1;
            continue;
        }
        while i < b.len() && (b[i] as char).is_whitespace() {
            i += 1;
        }
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && (b[i] as char).is_whitespace() {
                i += 1;
            }
            let value = if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                let from = i + 1;
                let to = tag[from..].find(q as char).map(|n| from + n).unwrap_or(tag.len());
                i = to + 1;
                &tag[from..to.min(tag.len())]
            } else {
                let from = i;
                while i < b.len() && !(b[i] as char).is_whitespace() && b[i] != b'>' {
                    i += 1;
                }
                &tag[from..i]
            };
            out.push((name, decode(value)));
        } else {
            out.push((name, String::new()));
        }
    }
    out
}

fn attr<'a>(a: &'a [(String, String)], name: &str) -> Option<&'a str> {
    a.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

/// The tag with an attribute taken out (`name="…"` or `name='…'`)
fn drop_attr(tag: &str, name: &str) -> String {
    let lower = tag.to_ascii_lowercase();
    let mut out = tag.to_string();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name).map(|i| i + from) {
        let before_ok = i > 0 && lower.as_bytes()[i - 1].is_ascii_whitespace();
        let rest = lower[i + name.len()..].trim_start();
        if before_ok && rest.starts_with('=') {
            let after_eq = lower.len() - rest.len() + 1;
            let v = lower[after_eq..].trim_start();
            let vstart = lower.len() - v.len();
            let end = match v.as_bytes().first() {
                Some(q @ (b'"' | b'\'')) => lower[vstart + 1..]
                    .find(*q as char)
                    .map(|n| vstart + 1 + n + 1)
                    .unwrap_or(lower.len()),
                _ => lower[vstart..]
                    .find(|c: char| c.is_whitespace() || c == '>')
                    .map(|n| vstart + n)
                    .unwrap_or(lower.len()),
            };
            out = format!("{}{}", &tag[..i - 1], &tag[end..]);
            return drop_attr(&out, name);
        }
        from = i + name.len();
    }
    out
}

/// Takes out every `<name …>…</name>` (and `<name …/>`)
fn strip_elements(src: &str, name: &str) -> String {
    let lower = src.to_ascii_lowercase();
    let open = format!("<{name}");
    let close = format!("</{name}");
    let mut out = String::with_capacity(src.len());
    let mut at = 0;
    while let Some(i) = lower[at..].find(&open).map(|i| i + at) {
        // only the element itself (not <descx>)
        let next = lower.as_bytes().get(i + open.len()).copied().unwrap_or(b'>');
        if !(next.is_ascii_whitespace() || next == b'>' || next == b'/') {
            out.push_str(&src[at..i + open.len()]);
            at = i + open.len();
            continue;
        }
        out.push_str(&src[at..i]);
        let tag_end = lower[i..].find('>').map(|n| i + n + 1).unwrap_or(lower.len());
        if lower[..tag_end].ends_with("/>") {
            at = tag_end;
            continue;
        }
        at = match lower[tag_end..].find(&close) {
            Some(n) => lower[tag_end + n..].find('>').map(|m| tag_end + n + m + 1).unwrap_or(lower.len()),
            None => lower.len(),
        };
    }
    out.push_str(&src[at..]);
    out
}

/// A text's words (its tags and Ford's entities out)
fn text_of(inner: &str) -> String {
    let mut plain = String::new();
    let mut in_tag = false;
    for c in inner.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => plain.push(c),
            _ => {}
        }
    }
    clean(&decode(&plain))
}

/// A picture's type from its first bytes (Ford leaves it out: "data:;base64,…")
fn picture_type(b64: &str) -> &'static str {
    if b64.starts_with("/9j/") {
        "image/jpeg"
    } else if b64.starts_with("R0lG") {
        "image/gif"
    } else if b64.starts_with("UklGR") {
        "image/webp"
    } else if b64.starts_with("PHN2Zy") || b64.starts_with("PD94bW") {
        "image/svg+xml"
    } else {
        "image/png"
    }
}

/// Where a picture in a drawing is (as on Ford's page: /ford_content/… is Ford's document server, other /…
/// addresses are PTS, and the rest are next to the drawing)
pub fn picture_url(href: &str, drawing_url: &str) -> Option<String> {
    let h = href.trim();
    if h.starts_with("https://") {
        return Some(h.to_string());
    }
    if h.to_ascii_lowercase().starts_with("/ford_content/") {
        return Some(format!("https://www.fordservicecontent.com{h}"));
    }
    if h.starts_with('/') {
        return Some(format!("{PTS}{h}"));
    }
    let dir = &drawing_url[..drawing_url.split('?').next().unwrap_or("").rfind('/')? + 1];
    Some(format!("{dir}{h}"))
}

/// A page link's place ("/wiring/page?…&cell=151&page=2&title=GENERATOR")
fn page_link(href: &str) -> Option<Link> {
    let h = decode(href);
    if !h.to_ascii_lowercase().contains("/wiring/page") {
        return None;
    }
    let query = h.split_once('?')?.1;
    let mut q: HashMap<String, String> = HashMap::new();
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            q.insert(k.to_ascii_lowercase(), crate::doc::percent_decode(&v.replace('+', " ")));
        }
    }
    let cell = cell_no(q.get("cell")?);
    (!cell.is_empty()).then(|| Link::Page {
        cell,
        page: q.get("page").and_then(|p| p.parse().ok()).filter(|p| *p > 0).unwrap_or(1),
        item: q.get("item").cloned().filter(|i| i != "0").unwrap_or_default(),
        title: clean(q.get("title").map(String::as_str).unwrap_or("")),
    })
}

/// Ford's drawing made ready to draw: its scripts, notes and hidden-ness out, its links and texts named (so they can
/// be found on the drawing), its size set, the pictures inside it given their type. The pictures it fetches from
/// elsewhere are listed in [`Prepared::svg`] as they are; see [`pictures`].
pub fn prepare(text: &str) -> std::result::Result<Prepared, String> {
    let mut src = text.to_string();
    for name in ["script", "desc", "metadata", "foreignobject", "iframe", "object", "embed"] {
        src = strip_elements(&src, name);
    }
    // the XML declaration and doctype
    for (open, close) in [("<?xml", "?>"), ("<!doctype", ">")] {
        if let Some(i) = src.to_ascii_lowercase().find(open) {
            if let Some(n) = src[i..].find(close) {
                src.replace_range(i..i + n + close.len(), "");
            }
        }
    }
    let lower = src.to_ascii_lowercase();
    let start = lower.find("<svg").ok_or("no drawing in Ford's reply")?;
    let end = lower.rfind("</svg>").map(|i| i + 6).unwrap_or(src.len());
    let src = &src[start..end];

    let mut out = String::with_capacity(src.len() + 4096);
    let mut links: Vec<(String, Vec<String>, Link)> = Vec::new();
    let mut texts: Vec<(String, String)> = Vec::new();
    // the links open around where we are (their place in `links`; None for an <a> that isn't one)
    let mut open: Vec<Option<usize>> = Vec::new();
    let (mut width, mut height) = (1100.0f32, 850.0f32);
    let mut at = 0;
    while let Some(i) = src[at..].find('<').map(|n| n + at) {
        out.push_str(&src[at..i]);
        let Some(close) = src[i..].find('>').map(|n| i + n + 1) else {
            at = src.len();
            break;
        };
        let tag = &src[i..close];
        let name: String = tag[1..]
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == ':' || *c == '_' || *c == '-')
            .collect::<String>()
            .to_ascii_lowercase();
        let closing = tag.starts_with("</");
        at = close;
        // (the hidden sheet, as Ford's page script shows it)
        let mut tag = tag.to_string();
        if !closing && !tag.starts_with("<!") {
            let lower = tag.to_ascii_lowercase();
            if lower.contains("visibility") {
                tag = drop_attr(&tag, "visibility").replace("visibility:hidden", "visibility:visible");
                tag = tag.replace("visibility: hidden", "visibility: visible");
            }
            if lower.contains("display") && attr(&attrs(&tag), "id").is_some_and(|id| id.starts_with("Sheet")) {
                tag = drop_attr(&tag, "display").replace("display:none", "display:inline");
            }
            // no handlers
            for (k, _) in attrs(&tag) {
                if k.starts_with("on") {
                    tag = drop_attr(&tag, &k);
                }
            }
        }
        match (name.as_str(), closing) {
            ("svg", false) if out.trim().is_empty() => {
                let a = attrs(&tag);
                let vb: Vec<f32> = attr(&a, "viewbox")
                    .unwrap_or("")
                    .split([' ', ','])
                    .filter(|v| !v.is_empty())
                    .filter_map(|v| v.parse().ok())
                    .collect();
                let num = |k: &str| attr(&a, k).and_then(|v| v.trim_end_matches("px").parse::<f32>().ok());
                let (x, y, w, h) = match vb.as_slice() {
                    [x, y, w, h] if *w > 0.0 && *h > 0.0 => (*x, *y, *w, *h),
                    _ => (0.0, 0.0, num("width").unwrap_or(1100.0), num("height").unwrap_or(850.0)),
                };
                width = w;
                height = h;
                let mut t = tag.clone();
                for k in ["width", "height", "viewbox", "preserveaspectratio", "id", "xmlns", "xmlns:xlink"] {
                    t = drop_attr(&t, k);
                }
                let t = t.trim_end_matches('>').trim_end_matches('/').to_string();
                out.push_str(&format!(
                    "{t} xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{w}\" height=\"{h}\" viewBox=\"{x} {y} {w} {h}\">"
                ));
            }
            ("a", false) => {
                let a = attrs(&tag);
                let href = attr(&a, "xlink:href").or_else(|| attr(&a, "href")).unwrap_or("").trim().to_string();
                let link = if href.is_empty() {
                    attr(&a, "name").filter(|n| !n.trim().is_empty()).map(|n| Link::Item {
                        name: n.trim().to_string(),
                        face: attr(&a, "faceview").unwrap_or("").trim().to_string(),
                    })
                } else {
                    page_link(&href).or_else(|| {
                        let abs = picture_url(&href, PTS).filter(|u| u.starts_with("https://"));
                        abs.map(Link::Url)
                    })
                };
                let id = format!("wl{}", links.len());
                match link {
                    Some(l) => {
                        open.push(Some(links.len()));
                        links.push((id.clone(), Vec::new(), l));
                        out.push_str(&format!("<g id=\"{id}\""));
                        if tag.ends_with("/>") {
                            out.push_str("/>");
                            open.pop();
                        } else {
                            out.push('>');
                        }
                    }
                    None => {
                        open.push(None);
                        out.push_str(if tag.ends_with("/>") { "<g/>" } else { "<g>" });
                        if tag.ends_with("/>") {
                            open.pop();
                        }
                    }
                }
            }
            ("a", true) => {
                open.pop();
                out.push_str("</g>");
            }
            ("text", false) if !tag.ends_with("/>") => {
                let id = format!("wt{}", texts.len());
                let inner_end = src[at..].to_ascii_lowercase().find("</text").map(|n| at + n).unwrap_or(at);
                let words = text_of(&src[at..inner_end]);
                let t = drop_attr(&tag, "id");
                out.push_str(&format!("<text id=\"{id}\"{}", &t[5..]));
                if !words.is_empty() {
                    if let Some(Some(l)) = open.iter().rev().find(|o| o.is_some()) {
                        links[*l].1.push(id.clone());
                    }
                }
                texts.push((id, words));
            }
            ("image" | "feimage", false) => {
                let a = attrs(&tag);
                let href = attr(&a, "xlink:href").or_else(|| attr(&a, "href")).unwrap_or("");
                // a picture inside the file whose type Ford left out
                if let Some(rest) = href.strip_prefix("data:;").or_else(|| href.strip_prefix("data:,")) {
                    let b64 = rest.split_once(',').map(|(_, d)| d).unwrap_or(rest);
                    let fixed = format!("data:{};{rest}", picture_type(b64));
                    out.push_str(&tag.replace(href, &fixed));
                } else {
                    out.push_str(&tag);
                }
            }
            _ => out.push_str(&tag),
        }
    }
    out.push_str(&src[at..]);
    Ok(Prepared {
        svg: out,
        width,
        height,
        links,
        texts,
    })
}

/// The pictures a drawing fetches from elsewhere (their `href`s as written)
pub fn pictures(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0;
    let lower = svg.to_ascii_lowercase();
    while let Some(i) = lower[at..].find("<image").map(|n| n + at) {
        let end = lower[i..].find('>').map(|n| i + n + 1).unwrap_or(lower.len());
        let a = attrs(&svg[i..end]);
        if let Some(h) = attr(&a, "xlink:href").or_else(|| attr(&a, "href")) {
            if !h.starts_with("data:") && !h.starts_with('#') && !out.iter().any(|x| x == h) {
                out.push(h.to_string());
            }
        }
        at = end;
    }
    out
}

/// A picture as an address inside the drawing (base64)
pub fn data_url(bytes: &[u8], kind: &str) -> String {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4 + 32);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    let kind = if kind.starts_with("image/") {
        kind.split(';').next().unwrap_or(kind)
    } else {
        "image/png"
    };
    format!("data:{kind};base64,{out}")
}

/// What a search result is (from Ford's result type and connector)
pub fn hit_kind(wtype: &str, conn: &str) -> HitKind {
    let t = wtype.to_ascii_lowercase();
    let c = conn.to_ascii_uppercase();
    let numbered = |p: char| c.starts_with(p) && c[1..].starts_with(|ch: char| ch.is_ascii_digit());
    if t == "celttl" {
        HitKind::Diagram
    } else if numbered('S') || t.contains("splice") {
        HitKind::Splice
    } else if numbered('G') || t.contains("ground") {
        HitKind::Ground
    } else if t.contains("component") {
        HitKind::Component
    } else if !c.is_empty() || t == "cfc" || t.contains("conn") {
        HitKind::Connector
    } else {
        HitKind::Other
    }
}

// ---------- asking Ford ----------

/// Each VIN's wiring book, and when it was read (kept two hours)
pub(crate) type InfoCache = std::sync::Arc<std::sync::Mutex<HashMap<String, (Instant, Info)>>>;

impl Ford {
    /// The vehicle's wiring book (read from its contents page; kept two hours)
    pub async fn wiring_info(&self, vin: &str) -> Result<Info> {
        let book = self.book(vin).await?;
        if let Some((at, info)) = self.wiring.lock().expect("wiring lock").get(&book.vin) {
            if at.elapsed() < Duration::from_secs(2 * 60 * 60) {
                return Ok(info.clone());
            }
        }
        let mut code = book.param("WiringBookCode").split(',').next().unwrap_or("").trim().to_string();
        if code.is_empty() {
            code = self
                .diag_context(vin)
                .await
                .ok()
                .and_then(|c| c.wiring_books.first().cloned())
                .unwrap_or_default();
        }
        if code.is_empty() {
            return Err(Error::Format("Ford has no wiring diagrams for this vehicle".into()));
        }
        let format = if book.param("WiringFormat").is_empty() {
            "svg"
        } else {
            book.param("WiringFormat")
        };
        let url = self.url(&format!(
            "/wiring/TableOfContents?vin={}&vehicleId={}&bookType={}&country=&language=&languageCode=ENUSA&book={}",
            enc(&book.vin),
            enc(book.param("vehicleId")),
            enc(&format.to_lowercase()),
            enc(&code)
        ))?;
        let html = self.text(url).await?;
        let info = read_info(&html, &book.vin, &code, &book.params, &book.title);
        self.log(&format!(
            "   wiring: book {} ({}), vehicle {}, {}",
            info.book,
            info.book_type,
            info.vehicle_id,
            if info.env.is_empty() { "no release" } else { info.env.as_str() }
        ));
        self.wiring
            .lock()
            .expect("wiring lock")
            .insert(book.vin.clone(), (Instant::now(), info.clone()));
        Ok(info)
    }

    /// An address on Ford's wiring server
    fn wiring_api(info: &Info, name: &str, extra: &[(&str, &str)]) -> String {
        let mut q = vec![
            ("book", info.book.as_str()),
            ("bookType", info.book_type.as_str()),
            ("contentmarket", ""),
            ("contentlanguage", ""),
        ];
        q.extend_from_slice(extra);
        q.push(("languageCode", info.lang.as_str()));
        q.push(("fromPageBase", PTS));
        let query: Vec<String> = q.iter().map(|(k, v)| format!("{k}={}", enc(v))).collect();
        format!(
            "{}/wiring/{name}?environment={}&{}",
            info.base.trim_end_matches('/'),
            enc(&info.env),
            query.join("&")
        )
    }

    /// A request to Ford's document server, as the wiring pages make it: its sign-on cookies first, and asked once
    /// more with fresh ones when it turns the request away
    async fn content_text(&self, url: &str, accept: &str) -> Result<String> {
        let u = workbench_net::Url::parse(url).map_err(|_| Error::Format("Not an address".into()))?;
        self.content_sign_on(false).await;
        let ask = || {
            crate::browser_headers(
                Http::request(Method::GET, u.clone())
                    .header("Accept", accept)
                    .header("Sec-Fetch-Dest", "empty")
                    .header("Sec-Fetch-Mode", "cors")
                    .header("Sec-Fetch-Site", "cross-site")
                    .header("Origin", PTS),
            )
        };
        let mut res = self.http().send(ask()).await?;
        if matches!(res.status.as_u16(), 401 | 403) {
            self.content_sign_on(true).await;
            res = self.http().send(ask()).await?;
        }
        let short: String = url.chars().take(160).collect();
        self.log(&format!("GET {short} → {}", res.status.as_u16()));
        if matches!(res.status.as_u16(), 401 | 403) {
            return Err(Error::NeedsLogin);
        }
        if !res.status.is_success() {
            return Err(Error::Status(res.status.as_u16()));
        }
        Ok(res.body)
    }

    async fn content_json(&self, url: &str) -> Result<Value> {
        let text = self.content_text(url, "application/json, text/plain, */*").await?;
        serde_json::from_str(&text).map_err(|_| {
            let start: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(160).collect();
            self.log(&format!("   not JSON: {start}"));
            Error::Format("Ford's wiring server sent something unexpected".into())
        })
    }

    /// The vehicle's options, for graying out what doesn't apply: PTS's decode, and its FeaturesAttributes cookie
    /// when it's this vehicle's
    async fn wiring_features(&self, vin: &str) -> Option<Features> {
        let book = self.book(vin).await.ok()?;
        let v = &book.vehicle;
        let mut codes: Vec<String> = v.features.iter().map(|c| c.to_uppercase()).collect();
        codes.extend(
            v.minor_features
                .iter()
                .map(|c| c.chars().take(5).collect::<String>().to_uppercase()),
        );
        let mut line = Vec::new();
        let mut build: String = v.build_date.chars().filter(char::is_ascii_digit).collect();
        if let Some((more, l, bd)) = self.vehicle_options(&book.vin, &v.build_date).await {
            codes.extend(more.into_iter().map(|c| c.to_uppercase()));
            line.extend(l);
            if build.is_empty() {
                build = bd.chars().filter(char::is_ascii_digit).collect();
            }
        }
        codes.sort();
        codes.dedup();
        (!codes.is_empty()).then_some(Features { codes, line, build })
    }

    /// The book's contents (and the vehicle's options, to gray out what doesn't apply)
    pub async fn wiring_toc(&self, vin: &str) -> Result<Toc> {
        let info = self.wiring_info(vin).await?;
        let cells = read_toc(&self.content_json(&Self::wiring_api(&info, "TableofContent", &[])).await?);
        if cells.is_empty() {
            return Err(Error::Format("Ford's wiring book is empty for this vehicle".into()));
        }
        let features = self.wiring_features(vin).await;
        self.log(&format!(
            "   wiring contents: {} diagrams{}",
            cells.len(),
            if features.is_some() { ", options known" } else { "" }
        ));
        Ok(Toc {
            title: info.title,
            cells,
            features,
        })
    }

    /// Kept on disk (a release's files don't change): where, for this release (older releases go)
    fn wiring_file(&self, info: &Info, name: &str) -> Option<std::path::PathBuf> {
        let root = self.cache.as_ref()?.join("wiring");
        let release: String = info
            .env
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || "_.-".contains(*c))
            .collect();
        if release.is_empty() {
            return None;
        }
        let dir = root.join(&release);
        if !dir.exists() {
            // a new release: the old ones go
            if let Ok(list) = std::fs::read_dir(&root) {
                for e in list.flatten() {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
            let _ = std::fs::create_dir_all(&dir);
        }
        let file: String = format!("{}_{name}", info.book)
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || "_.-".contains(*c))
            .collect();
        Some(dir.join(file))
    }

    /// A diagram's pages
    pub async fn wiring_pages(&self, vin: &str, cell: &str) -> Result<Vec<PageInfo>> {
        let info = self.wiring_info(vin).await?;
        let c = format!("{:0>3}", cell_no(cell));
        let file = self.wiring_file(&info, &format!("pages_{c}.json"));
        if let Some(text) = file.as_ref().and_then(|f| std::fs::read_to_string(f).ok()) {
            let kept: Option<Value> = serde_json::from_str(&text).ok();
            if let Some(v) = kept {
                return Ok(read_pages(&v));
            }
        }
        let v = self
            .content_json(&Self::wiring_api(&info, "PageList", &[("cell", &c), ("page", "1")]))
            .await?;
        if let Some(f) = file {
            let _ = std::fs::write(f, v.to_string());
        }
        Ok(read_pages(&v))
    }

    /// One page's drawing, ready to draw ([`prepare`]), with its pictures inside it
    pub async fn wiring_page(&self, vin: &str, cell: &str, page: u32) -> Result<Prepared> {
        let info = self.wiring_info(vin).await?;
        let c = format!("{:0>3}", cell_no(cell));
        let page = page.max(1);
        let file = self.wiring_file(&info, &format!("{c}_{page}.svg"));
        if let Some(svg) = file.as_ref().and_then(|f| std::fs::read_to_string(f).ok()) {
            if let Ok(p) = prepare(&svg) {
                return Ok(p);
            }
        }
        let base = info.base.replacen("/Ford_Content/", "/ford_content/", 1);
        let url = format!(
            "{}/wiring/svg/{}/{}/~W{}/{}/svg/{c}/0/{page}.svg?fromPageBase={}",
            base.trim_end_matches('/'),
            enc(&info.env),
            enc(&info.vehicle_id),
            enc(&info.book),
            enc(&info.lang),
            enc(PTS)
        );
        let raw = self.content_text(&url, "image/svg+xml,text/html,*/*").await?;
        if !raw.to_ascii_lowercase().contains("<svg") {
            let start: String = raw.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(160).collect();
            self.log(&format!("   wiring page {c}-{page}: no drawing ({start})"));
            return Err(Error::Format(format!("Ford has no drawing for {}-{page}", cell_no(&c))));
        }
        // its pictures (the location views' vehicle) put inside it, so it draws on its own
        let mut svg = raw;
        for href in pictures(&svg) {
            let Some(abs) = picture_url(&href, &url) else { continue };
            match self.bytes(&abs).await {
                Ok((bytes, kind)) => svg = svg.replace(&href, &data_url(&bytes, &kind)),
                Err(e) => self.log(&format!("   wiring picture: {e}")),
            }
        }
        let prepared = prepare(&svg).map_err(|e| Error::Format(e.to_string()))?;
        if let Some(f) = file {
            let _ = std::fs::write(f, &svg);
        }
        self.log(&format!(
            "   wiring page {c}-{page}: {} characters, {} links, {} texts",
            svg.len(),
            prepared.links.len(),
            prepared.texts.len()
        ));
        Ok(prepared)
    }

    /// Where a connector ("C1689"), splice ("S105") or ground ("G101") is: pages of the location views
    pub async fn wiring_locations(&self, vin: &str, item: &str) -> Result<Vec<Location>> {
        let info = self.wiring_info(vin).await?;
        let kind = match item.chars().next().map(|c| c.to_ascii_uppercase()) {
            Some('C') => "CONNECTOR",
            Some('S') => "SPLICE",
            _ => "GROUND",
        };
        let v = self
            .content_json(&Self::wiring_api(
                &info,
                "GetLocationComponentType",
                &[("Item", item), ("componentType", kind)],
            ))
            .await?;
        Ok(read_locations(&v))
    }

    /// Ford's search, wiring only (components, connectors, splices, grounds, diagrams)
    pub async fn wiring_search(&self, vin: &str, text: &str) -> Result<Vec<Hit>> {
        let q: String = text.trim().chars().take(100).collect();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let req = crate::diagnostics::DiagRequest {
            text: q,
            sources: vec!["wiring".into()],
            rows: 150,
            ..Default::default()
        };
        let page = self.diag_search(vin, &req).await?;
        Ok(page
            .results
            .into_iter()
            .filter_map(|r| {
                let w = r.wiring?;
                let kind = hit_kind(&w.kind, &w.conn);
                let cell = cell_no(&w.cell);
                // Ford's component titles end with where they are ("HEATED WIPER PARK RELAY: Power Distribution/BCM")
                let item = if !w.conn.is_empty() {
                    w.conn.clone()
                } else if kind == HitKind::Component {
                    clean(r.title.split(": ").next().unwrap_or(""))
                } else {
                    String::new()
                };
                // the connector faces and the connector list are Ford's own pages
                let own = w.kind == "cfc" || cell == "150";
                Some(Hit {
                    kind,
                    title: clean(&r.title),
                    cell: if own { String::new() } else { cell },
                    page: w.page.parse().unwrap_or(1),
                    item,
                    applies: r.applies,
                    url: r.url,
                })
            })
            .collect())
    }

    /// Ford's own wiring page (what the app doesn't draw: connector faces, the connector list, the location index)
    pub fn wiring_ford_url(info: &Info, kind: &str, cell: &str, page: u32, item: &str, title: &str) -> String {
        let mut q = vec![
            ("book", info.book.clone()),
            ("vehicleId", info.vehicle_id.clone()),
            ("vin", info.vin.clone()),
            ("cell", cell.to_string()),
            ("page", if page == 0 { String::new() } else { page.to_string() }),
        ];
        if !item.is_empty() {
            q.push(("item", item.to_string()));
        }
        q.push(("sortby", String::new()));
        q.push(("bookType", info.book_type.clone()));
        q.push(("languageCode", info.lang.clone()));
        if !title.is_empty() {
            q.push(("title", title.to_string()));
        }
        q.push(("booktitle", info.title.clone()));
        let query: Vec<String> = q.iter().map(|(k, v)| format!("{k}={}", enc(v))).collect();
        format!("{PTS}/wiring/{kind}/?{}", query.join("&"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(t: &str) -> Value {
        serde_json::from_str(t).unwrap()
    }

    #[test]
    fn reads_the_contents_and_pages() {
        let toc = read_toc(&parse(
            r#"[{ "Title": "Charging   System", "Type": "Page", "Number": "012", "Code": [] },
                { "Title": "Connector views", "Type": "Connectors", "Number": 150 },
                { "Title": "No number" }]"#,
        ));
        assert_eq!(toc.len(), 2);
        assert_eq!((toc[0].number.as_str(), toc[0].title.as_str()), ("12", "Charging System"));
        assert!(toc[0].drawn() && !toc[1].drawn());
        assert_eq!(clean("Charging\u{a0} System "), "Charging System");
        let pages = read_pages(&parse(
            r#"[{ "page": "2", "Code": ["(4X4|AWD)+(!HEV)"], "Startdate": "2022-06-01", "Enddate": "" },
                { "page": 1, "Code": [] }]"#,
        ));
        assert_eq!(pages.iter().map(|p| p.page).collect::<Vec<_>>(), [1, 2]);
        let f = Features {
            codes: vec!["AWD".into(), "DRAFE".into()],
            line: vec![],
            build: "20230115".into(),
        };
        assert!(page_applies(&pages[1], Some(&f)));
        assert!(!page_applies(
            &PageInfo {
                start: "2023-06-01".into(),
                ..pages[1].clone()
            },
            Some(&f)
        ));
        assert!(!page_applies(
            &PageInfo {
                codes: "(HEV)".into(),
                ..pages[1].clone()
            },
            Some(&f)
        ));
        assert!(page_applies(&pages[1], None));
        let cell = Cell {
            codes: vec!["DRA-FE".into()],
            ..Default::default()
        };
        assert!(cell_applies(&cell, Some(&f)));
        assert!(!cell_applies(
            &Cell {
                codes: vec!["XYZ".into()],
                ..Default::default()
            },
            Some(&f)
        ));
    }

    #[test]
    fn reads_the_books_settings() {
        let html = r#"<script>var x = { book: 'EQD', bookType: 'svg', languageCode: 'ENUSA' };
            var api = "https://www.fordservicecontent.com/Ford_Content/PublicationRuntimeRefreshPTS//wiring/TableofContent?environment=prod_1_3_11112026&book=EQD";
            var u = 'vehicleId=' + '6925'; var t = 'booktitle=' + '2023 Bronco Sport';</script>"#;
        let info = read_info(html, "VIN", "XX", &HashMap::new(), "");
        assert_eq!(info.book, "EQD");
        assert_eq!(info.env, "prod_1_3_11112026");
        assert_eq!(
            info.base,
            "https://www.fordservicecontent.com/Ford_Content/PublicationRuntimeRefreshPTS/"
        );
        assert_eq!(info.vehicle_id, "6925");
        assert_eq!(info.title, "2023 Bronco Sport");
        assert!(Ford::wiring_api(&info, "PageList", &[("cell", "012")]).starts_with(
            "https://www.fordservicecontent.com/Ford_Content/PublicationRuntimeRefreshPTS/wiring/PageList?environment=prod_1_3_11112026&book=EQD"
        ));
    }

    #[test]
    fn makes_a_drawing_ready() {
        let svg = r#"<?xml version="1.0"?><svg xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="-0.75 -0.749 1100 850" height="100%" width="100%" id="svgDoc">
            <desc><Qualifier/><note>x</note></desc><script>alert(1)</script>
            <g style="visibility:hidden" id="Sheet_1" onclick="go()">
              <a id="dialog_C202" name="C202" faceview="EQDCIC202"><text font-size="12">C202</text></a>
              <a id="dialog_151-2" xlink:href="/wiring/page?vin=X&amp;cell=151&amp;page=2&amp;title=GENERATOR"><text>151-2</text></a>
              <text transform="matrix(0.9 0 0 1 837 540)" fill="black">W&amp;H</text>
              <image xlink:href="data:;base64,iVBORw0KGgo=" />
            </g></svg>"#;
        let p = prepare(svg).unwrap();
        assert_eq!((p.width, p.height), (1100.0, 850.0));
        assert!(!p.svg.contains("<script") && !p.svg.contains("<desc") && !p.svg.contains("onclick"));
        assert!(p.svg.contains("visibility:visible"));
        assert!(p.svg.starts_with("<svg") && p.svg.contains("xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(p.svg.contains("width=\"1100\" height=\"850\" viewBox=\"-0.75 -0.749 1100 850\""));
        assert_eq!(p.links.len(), 2);
        assert_eq!(
            p.links[0].2,
            Link::Item {
                name: "C202".into(),
                face: "EQDCIC202".into()
            }
        );
        assert_eq!(
            p.links[1].2,
            Link::Page {
                cell: "151".into(),
                page: 2,
                item: String::new(),
                title: "GENERATOR".into()
            }
        );
        assert_eq!(p.links[0].1, ["wt0"]);
        assert_eq!(
            p.texts.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(),
            ["C202", "151-2", "W&H"]
        );
        assert!(p.svg.contains("<g id=\"wl0\"><text id=\"wt0\" font-size=\"12\">C202</text></g>"));
        assert!(p.svg.contains("data:image/png;base64,iVBORw0KGgo="));
        assert_eq!(hit_kind("conn_search", "C1689"), HitKind::Connector);
        assert_eq!(hit_kind("conn_search", "S105"), HitKind::Splice);
        assert_eq!(hit_kind("celttl", ""), HitKind::Diagram);
        assert_eq!(
            picture_url("/ford_content/a.png", "x").unwrap(),
            "https://www.fordservicecontent.com/ford_content/a.png"
        );
        assert_eq!(picture_url("b.png", "https://h/x/y/1.svg?q=/z").unwrap(), "https://h/x/y/b.png");
        assert_eq!(data_url(b"foobar", "image/png"), "data:image/png;base64,Zm9vYmFy");
    }
}
