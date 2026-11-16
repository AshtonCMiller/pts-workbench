//! Labor times: Ford's SLTS (Service Labor Time Standards) for a vehicle, as the Electron app's Labor Times tab reads
//! it (`manual/labor-times/`). SLTS is an old ASP.NET page that works by posting its form back to itself: the last page
//! it sent is kept (by the tab, [`SltsPage`]) and the next request posts its form back, filled in the way SLTS's own
//! controls would.
//!
//!   reading its pages     the search form ([`read_form`]), the report ([`parse_report`]: operations, their options,
//!                         combinations, supplements and overlaps), anything else it shows ([`read_raw`]), Word
//!                         Search's matches ([`read_matches`]), one-time use parts ([`read_parts_page`])
//!   asking it             [`Ford::slts_start`], [`Ford::slts_post_back`], [`Ford::slts_word_search`],
//!                         [`Ford::slts_parts`]
//!   what's kept           [`Saved`]: the tray, the search type, the last search (the Electron app's own format, so
//!                         it carries over both ways)

use serde::{Deserialize, Serialize};

use crate::html::{Doc, El, Piece, clean};

/// Customer pay time = warranty time × 1.5
pub const CUSTOMER_PAY: f64 = 1.5;

/// SLTS's own folder: every page asked for is inside it
pub const SLTS_BASE: &str = "https://www.fordtechservice.dealerconnection.com/vdirsnet/SLTSVinWebView/";

/// The last page SLTS sent (its form is what the next request posts back), and where it came from
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SltsPage {
    pub url: String,
    pub html: String,
}

impl SltsPage {
    pub fn doc(&self) -> Doc {
        Doc::parse(&self.html)
    }
}

// ---------- kept with the vehicle ----------

/// What's kept for a vehicle's Labor Times (the Electron app's tab state: `{ tray, type, pick, q }`)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub tray: Vec<TrayItem>,
    /// The search type ("Section", "Operation", "Word"…)
    #[serde(rename = "type")]
    pub kind: String,
    /// What was picked in the list (a section)
    pub pick: String,
    /// What was typed
    pub q: String,
}

/// An operation's option in the tray
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TrayItem {
    pub code: String,
    pub qual: String,
    /// Warranty time ("1.2"), or what SLTS says instead ("AT": actual time)
    pub time: String,
    pub name: String,
    pub kind: OpKind,
    /// A combination's base operation, and its name
    pub base: String,
    pub base_name: String,
    pub cart: Option<Cart>,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpKind {
    #[default]
    Base,
    /// Claimed with its base operation
    Combo,
    /// Adds on to the operation above it
    Supp,
}

/// SLTS's one-time use parts link for an option (its `discardPartList(…)`)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Cart {
    pub op_num_base: String,
    pub op_num: String,
    pub part_no: String,
    pub qty: String,
    pub desc: String,
    pub qual: String,
}

/// "When claiming 19700A4: Do not use with; 6007A1, 6007A34*" (`*`: any code starting so)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rule {
    pub subject: String,
    pub not: Vec<String>,
    pub cond: String,
}

// ---------- small things ----------

/// "1.2" → 1.2 (SLTS's other times, "AT" and the like, are none)
pub fn num(t: &str) -> Option<f64> {
    let t = t.trim();
    let ok = !t.is_empty()
        && !t.starts_with('.')
        && !t.ends_with('.')
        && t.chars().all(|c| c.is_ascii_digit() || c == '.')
        && t.matches('.').count() <= 1;
    if ok { t.parse().ok() } else { None }
}

/// Hours as the Electron app shows them: 1.5 → "1.5", 1.25 → "1.25", 2 → "2.0"
pub fn hrs(n: f64) -> String {
    let s = format!("{:.2}", (n * 100.0).round() / 100.0);
    match s.strip_suffix('0') {
        Some(t) => t.to_string(),
        None => s,
    }
}

/// 18124A → 18124, 19700D12 → 19700, 19700A4WA → 19700 (the number SLTS's operation search takes)
pub fn base_number(code: &str) -> String {
    let b = code.as_bytes();
    let up = |c: u8| c.is_ascii_uppercase();
    for i in 0..b.len() {
        if !up(b[i]) {
            continue;
        }
        // 1 or 2 letters, digits, letters, to the end
        for letters in [2, 1] {
            let mut j = i;
            let mut n = 0;
            while j < b.len() && up(b[j]) && n < letters {
                j += 1;
                n += 1;
            }
            if n < letters {
                continue;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            while j < b.len() && up(b[j]) {
                j += 1;
            }
            if j == b.len() {
                let base = &code[..i];
                return if base.is_empty() { code.to_string() } else { base.to_string() };
            }
        }
    }
    code.to_string()
}

/// 19700A, 19700D: base operations (a number, then one letter)
pub fn is_base(code: &str) -> bool {
    let b = code.as_bytes();
    b.len() >= 2
        && b[b.len() - 1].is_ascii_uppercase()
        && b[b.len() - 2].is_ascii_digit()
        && b.iter().all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
}

/// 5212BF → 5212B, 18125AT → 18125A: the operation an option belongs to
pub fn op_id_of(code: &str) -> String {
    let b = code.as_bytes();
    for i in 1..b.len() {
        if b[i].is_ascii_uppercase() && b[i - 1].is_ascii_digit() && b[..i].iter().all(|c| c.is_ascii_alphanumeric()) {
            return code[..=i].to_string();
        }
    }
    code.to_string()
}

/// Whether a code is one of a rule's ("6007A34*": any starting 6007A34)
pub fn matches(pattern: &str, code: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(p) => code.starts_with(p),
        None => code == pattern,
    }
}

/// Word Search: SLTS opens it in a window of its own
pub fn is_word(kind: &str) -> bool {
    kind.get(..4).is_some_and(|w| w.eq_ignore_ascii_case("word"))
}

// ---------- the page's HTML ----------

/// A row's cells
fn cells<'a>(tr: El<'a>) -> Vec<El<'a>> {
    tr.children().filter(|c| matches!(c.tag(), "td" | "th")).collect()
}

/// A table's own rows (not a table's inside it)
fn rows<'a>(table: El<'a>) -> Vec<El<'a>> {
    let mut out = Vec::new();
    for c in table.children() {
        match c.tag() {
            "tr" => out.push(c),
            "tbody" | "thead" | "tfoot" => out.extend(c.children().filter(|r| r.tag() == "tr")),
            _ => {}
        }
    }
    out
}

fn form<'a>(doc: &'a Doc) -> Option<El<'a>> {
    doc.root().first("form")
}

/// An option's value as a browser sends it (its text when it has none)
fn option_value(o: &El) -> String {
    o.attr("value").map(str::to_string).unwrap_or_else(|| o.text())
}

/// Every field SLTS's form would send (no image buttons: those are added for a click), in order
pub fn form_fields(doc: &Doc) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let Some(f) = form(doc) else { return out };
    for n in f.find_all(|e| matches!(e.tag(), "input" | "select" | "textarea")) {
        let Some(name) = n.attr("name").filter(|n| !n.is_empty()) else {
            continue;
        };
        let kind = n.attr("type").unwrap_or("").to_ascii_lowercase();
        let value = if n.tag() == "select" {
            // Like a browser: a list box (size > 1) with nothing picked, or an empty list, sends nothing; sending ''
            // for it makes ASP.NET reject the whole post ("Invalid postback argument")
            let list_box = n.attr("multiple").is_some() || n.attr("size").and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(1) > 1;
            let opts = n.all("option");
            let opt = opts
                .iter()
                .find(|o| o.attr("selected").is_some())
                .or(if list_box { None } else { opts.first() });
            match opt {
                Some(o) => option_value(o),
                None => continue,
            }
        } else if matches!(kind.as_str(), "image" | "submit" | "button" | "reset") {
            continue;
        } else if matches!(kind.as_str(), "checkbox" | "radio") {
            if n.attr("checked").is_none() {
                continue;
            }
            n.attr("value").filter(|v| !v.is_empty()).unwrap_or("on").to_string()
        } else {
            match n.attr("value").filter(|v| !v.is_empty()) {
                Some(v) => v.to_string(),
                None => n.raw_text(),
            }
        };
        set(&mut out, name, value);
    }
    out
}

/// Sets a field (in its place when it's already there)
pub fn set(fields: &mut Vec<(String, String)>, name: &str, value: impl Into<String>) {
    let value = value.into();
    match fields.iter_mut().find(|(k, _)| k == name) {
        Some(f) => f.1 = value,
        None => fields.push((name.to_string(), value)),
    }
}

pub fn field<'a>(fields: &'a [(String, String)], name: &str) -> &'a str {
    fields.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str()).unwrap_or("")
}

/// Where the form posts to (relative to the page)
pub fn form_action(doc: &Doc) -> String {
    form(doc).and_then(|f| f.attr("action")).unwrap_or("").to_string()
}

/// A hidden field's value, by its id
pub fn hidden(doc: &Doc, id: &str) -> String {
    doc.by_id(id).and_then(|e| e.attr("value")).unwrap_or("").to_string()
}

/// application/x-www-form-urlencoded
pub fn form_body(fields: &[(String, String)]) -> String {
    fields
        .iter()
        .map(|(k, v)| format!("{}={}", form_enc(k), form_enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// As a browser's URLSearchParams writes it (spaces as +)
pub fn form_enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b' ' => out.push('+'),
            b if b.is_ascii_alphanumeric() || b"*-._".contains(&b) => out.push(b as char),
            b => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn query(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", form_enc(k), form_enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

// ---------- the search form (mirrors SLTS's own) ----------

/// A choice in one of SLTS's lists
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.label.is_empty() { &self.value } else { &self.label })
    }
}

/// SLTS's search form as the page has it
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchForm {
    /// The search types ("Section", "Operation Number", "Word"…)
    pub types: Vec<Choice>,
    /// The one picked ("" when none is marked)
    pub current: String,
    /// The pick list (sections, the alphabetic index…), and the one picked
    pub pick: Option<(Vec<Choice>, String)>,
    /// The typing box's name, when there is one
    pub text_box: Option<String>,
    /// What's typed in it already
    pub text_value: String,
    /// What SLTS calls it ("Operation Number")
    pub label: String,
}

impl SearchForm {
    /// Whether the search needs something typed
    pub fn wants_text(&self) -> bool {
        ["word", "part", "operation"].contains(&self.current.to_ascii_lowercase().as_str())
    }

    /// Whether the typing box shows
    pub fn shows_text(&self) -> bool {
        self.text_box.is_some() || (self.pick.is_none() && self.wants_text())
    }

    /// The typing box's field
    pub fn text_name(&self) -> String {
        self.text_box.clone().unwrap_or_else(|| "TextBox_OperationOrPartNumber".into())
    }

    pub fn placeholder(&self) -> String {
        if self.label.is_empty() {
            "Search…".into()
        } else {
            format!("{}…", self.label)
        }
    }
}

/// SLTS's typing box: any text-like field (no type is text too) except the overlap checker's ten boxes
fn search_box<'a>(doc: &'a Doc) -> Option<El<'a>> {
    let f = form(doc)?;
    let ok = |n: &El| {
        let kind = n.attr("type").unwrap_or("text").to_ascii_lowercase();
        let name = n.attr("name").unwrap_or("");
        let numbered = name.len() > "TextBox_OperationNumber".len()
            && name[.."TextBox_OperationNumber".len()].eq_ignore_ascii_case("TextBox_OperationNumber")
            && name["TextBox_OperationNumber".len()..].chars().all(|c| c.is_ascii_digit());
        (n.tag() == "textarea" || (n.tag() == "input" && (kind == "text" || kind == "search"))) && !name.is_empty() && !numbered
    };
    let all = f.find_all(|e| matches!(e.tag(), "input" | "textarea") && ok(e));
    let in_form = |e: &El| {
        e.closest(|x| matches!(x.attr("id"), Some("TableRow_Form" | "Panel_SearchFor")))
            .is_some()
    };
    all.iter()
        .find(|e| in_form(e))
        .or_else(|| {
            all.iter().find(|e| {
                let n = e.attr("name").unwrap_or("").to_ascii_lowercase();
                ["searchfor", "word", "keyword", "operationorpart"].iter().any(|w| n.contains(w))
            })
        })
        .copied()
}

fn choices(select: El) -> (Vec<Choice>, Option<String>) {
    let mut picked = None;
    let list = select
        .all("option")
        .iter()
        .map(|o| {
            let c = Choice {
                value: option_value(o),
                label: o.text(),
            };
            if o.attr("selected").is_some() && picked.is_none() {
                picked = Some(c.value.clone());
            }
            c
        })
        .collect();
    (list, picked)
}

pub fn read_form(doc: &Doc) -> SearchForm {
    let (types, current) = doc.by_id("DropDownList_SearchType").map(choices).unwrap_or_default();
    let pick = doc.by_id("DropDownList_SearchFor").map(|s| {
        let (list, picked) = choices(s);
        let picked = picked.or_else(|| list.first().map(|c| c.value.clone())).unwrap_or_default();
        (list, picked)
    });
    let text = search_box(doc);
    SearchForm {
        types,
        current: current.unwrap_or_default(),
        pick,
        text_box: text.and_then(|t| t.attr("name")).map(str::to_string),
        text_value: text.and_then(|t| t.attr("value")).unwrap_or("").to_string(),
        label: doc.by_id("Label_SearchFor").map(|l| l.text()).unwrap_or_default(),
    }
}

/// Whether it's SLTS's main page (for this VIN)
pub fn is_slts_page(html: &str, vin: &str) -> bool {
    html.contains("DropDownList_SearchType") && (vin.is_empty() || html.to_uppercase().contains(&vin.to_uppercase()))
}

// ---------- the report ----------

/// One way an operation is claimed (an engine, a location…): its code, time and parts link
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Opt {
    pub code: String,
    /// What it's for ("2.0L GTDI"); empty: all
    pub qual: String,
    pub time: String,
    pub cart: Option<Cart>,
}

/// A note under an operation (blue in SLTS), and what it links to
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Note {
    pub text: String,
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Op {
    pub code: String,
    pub name: String,
    /// Its base part numbers
    pub parts: Vec<String>,
    pub kind: OpKind,
    /// A combination's base operation, a supplement's operation
    pub parent: String,
    /// The group it's under in a section's list ("DIAGNOSTIC OPERATIONS")
    pub group: String,
    pub desc: Vec<String>,
    pub notes: Vec<Note>,
    pub overlaps: Vec<String>,
    pub options: Vec<Opt>,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub title: String,
    pub ops: Vec<Op>,
}

impl Report {
    /// The operations under no other (combinations and supplements go under theirs)
    pub fn roots(&self) -> Vec<usize> {
        (0..self.ops.len())
            .filter(|&i| {
                let p = &self.ops[i].parent;
                p.is_empty() || !self.ops.iter().any(|o| &o.code == p)
            })
            .collect()
    }

    pub fn children(&self, i: usize) -> Vec<usize> {
        let code = &self.ops[i].code;
        (0..self.ops.len())
            .filter(|&j| j != i && !code.is_empty() && &self.ops[j].parent == code)
            .collect()
    }

    /// The operation asked for (by its code, or one of its options' codes)
    pub fn find(&self, want: &str) -> Option<usize> {
        if want.is_empty() {
            return None;
        }
        self.ops
            .iter()
            .position(|o| o.code == want)
            .or_else(|| self.ops.iter().position(|o| o.options.iter().any(|x| x.code == want)))
    }
}

/// A cell's code: its text, without spaces, in capitals
fn code_of(td: Option<&El>) -> String {
    td.map(|t| t.text().replace(' ', "").to_uppercase()).unwrap_or_default()
}

/// The quoted arguments of a `discardPartList('…', '…', …)` link
fn quoted(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\'' {
            continue;
        }
        let mut v = String::new();
        let mut closed = false;
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    v.push(c);
                    if let Some(n) = chars.next() {
                        v.push(n);
                    }
                }
                '\'' => {
                    closed = true;
                    break;
                }
                c => v.push(c),
            }
        }
        if closed {
            out.push(v.replace("&quot;", "\"").replace("&amp;", "&"));
        }
    }
    out
}

fn cart_of(td: Option<&El>) -> Option<Cart> {
    let a = td?.find(|e| e.tag() == "a" && e.attr("href").is_some_and(|h| h.contains("discardPartList")))?;
    let href = a.attr("href")?;
    let open = href.find("discardPartList(")? + "discardPartList(".len();
    let close = href.rfind(')').filter(|c| *c >= open)?;
    let args = quoted(&href[open..close]);
    let get = |i: usize| args.get(i).cloned().unwrap_or_default();
    Some(Cart {
        op_num_base: get(0),
        op_num: get(1),
        part_no: get(2),
        qty: get(3),
        desc: get(4),
        qual: get(5),
    })
}

/// "When claiming 19700A4: Do not use with; 6007A1, 6007A34*, …" → rules
pub fn overlap_rules(lines: &[String]) -> Vec<Rule> {
    let mut out = Vec::new();
    for line in lines {
        let low = line.to_ascii_lowercase();
        let Some(at) = low.find("when claiming") else { continue };
        let rest = &line[at + "when claiming".len()..];
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let Some(colon) = rest.find(':') else { continue };
        let subject = rest[..colon].trim().to_uppercase();
        let after = rest[colon + 1..].trim_start();
        let Some(list) = after
            .get(.."do not use with".len())
            .filter(|p| p.eq_ignore_ascii_case("do not use with"))
            .map(|_| &after["do not use with".len()..])
        else {
            continue;
        };
        let list = list.strip_prefix(';').unwrap_or(list).trim_start();
        if subject.is_empty() || list.is_empty() {
            continue;
        }
        let (mut list, mut cond) = (list.to_string(), String::new());
        if let Some(i) = list.to_ascii_lowercase().find(" for vehicles") {
            // (with the spaces before it)
            let start = list[..i].trim_end().len();
            cond = list[i..].trim().to_string();
            list.truncate(start);
        }
        let not = list
            .split(',')
            .map(|x| x.trim().trim_end_matches('.').to_uppercase())
            .filter(|x| !x.is_empty() && x.chars().all(|c| c.is_ascii_digit() || c.is_ascii_uppercase() || c == '*'))
            .collect();
        out.push(Rule { subject, not, cond });
    }
    out
}

/// An option's row: its qualifier on the right, its code
fn is_option_row(r: &El) -> bool {
    let c = cells(*r);
    c.get(1).and_then(|t| t.attr("align")) == Some("right") && !code_of(c.get(3)).is_empty()
}

/// An operation's own anchor (`<a name="19700A">`; not SLTS's top / bottom ones)
fn op_anchor<'a>(r: &El<'a>) -> Option<El<'a>> {
    r.find(|e| {
        e.tag() == "a"
            && e.attr("name")
                .is_some_and(|n| !n.eq_ignore_ascii_case("top") && !n.eq_ignore_ascii_case("bottom"))
    })
}

/// An operation's title: its bold name in a small table with SLTS's ▲▼ (top / bottom) links, or its own `<a name>`
fn is_title_row(r: &El) -> bool {
    if is_option_row(r) {
        return false;
    }
    if op_anchor(r).is_some() {
        return true;
    }
    let c = cells(*r);
    let Some(td) = c.get(1) else { return false };
    let bold_in_table = td.all("table").iter().any(|t| t.first("b").is_some());
    let top_link = td
        .find(|e| e.tag() == "a" && e.attr("href").is_some_and(|h| h.ends_with("#top") || h.ends_with("#bottom")))
        .is_some();
    bold_in_table && top_link
}

/// The first option row after an operation's title row (before the next title)
fn next_option_row<'a>(row: &El<'a>) -> Option<El<'a>> {
    let mut r = row.next_element();
    while let Some(x) = r {
        if is_title_row(&x) {
            return None;
        }
        if is_option_row(&x) {
            return Some(x);
        }
        r = x.next_element();
    }
    None
}

/// The address in an `onclick` / `href` of `window.open('…')`
fn window_open(href: &str) -> String {
    href.find("window.open('")
        .and_then(|i| {
            let rest = &href[i + "window.open('".len()..];
            rest.find('\'').map(|e| rest[..e].to_string())
        })
        .unwrap_or_default()
}

/// The name without the part numbers SLTS puts in brackets after it
fn op_name(raw: &str, parts: &[String]) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')').map(|c| open + c) else {
            break;
        };
        let mut end = close + 1;
        while rest[end..].starts_with(char::is_whitespace) {
            end += rest[end..].chars().next().map(char::len_utf8).unwrap_or(1);
        }
        let bracket = &rest[open..end];
        out.push_str(&rest[..open]);
        if !parts.iter().any(|p| bracket.contains(p.as_str())) {
            out.push_str(bracket);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    clean(&out)
}

/// SLTS's report: one long table read into operations (code, title, description, notes, overlaps) with their options
/// (one per engine, location…), and the combinations and supplements that add on to them. None: not a report.
pub fn parse_report(doc: &Doc) -> Option<Report> {
    let xml = doc.by_id("TableRow_XmlContent")?;
    // The report table: rows of six cells (gap, description, gap, operation number, gap, time). A section can come
    // back as several of them: all are read, in order; only the innermost, so a wrapper's rows aren't read twice.
    let op_row = |r: &El| {
        let c = cells(*r);
        c.len() == 6 && c[3].attr("width") == Some("100")
    };
    let tables: Vec<El> = xml.all("table").into_iter().filter(|t| rows(*t).iter().any(op_row)).collect();
    let tables: Vec<El> = tables
        .iter()
        .filter(|t| !tables.iter().any(|o| o != *t && o.is_inside(t)))
        .copied()
        .collect();
    if tables.is_empty() {
        return None;
    }
    let title = xml
        .find(|e| e.tag() == "td" && e.has_class("copysubtitle"))
        .map(|t| t.text())
        .unwrap_or_default();
    let mut ops: Vec<Op> = Vec::new();
    let mut cur: Option<usize> = None;
    let mut base: Option<usize> = None;
    #[derive(PartialEq)]
    enum Mode {
        Base,
        Combo,
        Supp,
    }
    let mut mode = Mode::Base;
    let mut supp_parent: Option<usize> = None;
    let mut group = String::new();
    for row in tables.iter().flat_map(|t| rows(*t)) {
        if !op_row(&row) {
            continue;
        }
        let tds = cells(row);
        let text = row.text();
        let low = text.to_ascii_lowercase();
        if text.is_empty() || low.split_whitespace().collect::<Vec<_>>() == ["operation", "description", "operation", "number", "time"] {
            continue;
        }
        let code3 = code_of(tds.get(3));
        // A section's list puts a coloured group title ("DIAGNOSTIC OPERATIONS") between operations
        let in_b_font = tds[1].all("b").iter().any(|b| b.first("font").is_some());
        if tds[1].first("table").is_none() && in_b_font && code3.is_empty() {
            group = tds[1].text();
            cur = None;
            base = None;
            mode = Mode::Base;
            supp_parent = None;
            continue;
        }
        if code3 == "OPERATIONNUMBER" {
            continue; // the column headings
        }
        // An operation's title row: its name in bold (with part numbers) in a small table of its own. It carries
        // <a name="CODE"> only when the operation has a single time; otherwise the code comes from its options.
        if is_title_row(&row) {
            let mut code = op_anchor(&row).and_then(|a| a.attr("name")).unwrap_or("").to_uppercase();
            if code.is_empty() {
                code = op_id_of(&code_of(next_option_row(&row).and_then(|r| cells(r).get(3).copied()).as_ref()));
            }
            let b = tds[1].first("b");
            let parts: Vec<String> = b
                .map(|b| {
                    b.find_all(|e| e.tag() == "a" && e.attr("href").is_some_and(|h| h.contains("partlink")))
                        .iter()
                        .map(|a| a.text())
                        .collect()
                })
                .unwrap_or_default();
            let name = op_name(&b.map(|b| b.text()).unwrap_or_default(), &parts);
            let (mut kind, mut parent) = (OpKind::Base, None);
            if code.is_empty() || is_base(&code) {
                mode = Mode::Base;
                supp_parent = None;
            } else if mode == Mode::Supp && supp_parent.is_some_and(|p| code.starts_with(&ops[p].code)) {
                kind = OpKind::Supp;
                parent = supp_parent;
            } else if mode == Mode::Supp {
                mode = Mode::Combo;
            }
            if kind != OpKind::Supp && mode == Mode::Combo && base.is_some() {
                kind = OpKind::Combo;
                parent = base;
            }
            let mut op = Op {
                name: if name.is_empty() { code.clone() } else { name },
                code,
                parts,
                kind,
                parent: parent.map(|p| ops[p].code.clone()).unwrap_or_default(),
                group: group.clone(),
                ..Default::default()
            };
            if !code3.is_empty() {
                op.options.push(Opt {
                    code: code3,
                    qual: String::new(),
                    time: tds.get(5).map(|t| t.text()).unwrap_or_default(),
                    cart: cart_of(tds.get(3)),
                });
            }
            ops.push(op);
            let i = ops.len() - 1;
            if kind == OpKind::Base {
                base = Some(i);
            }
            cur = Some(i);
            continue;
        }
        let underlined = row.all("b").iter().any(|b| b.first("u").is_some()) || row.all("u").iter().any(|u| u.first("b").is_some());
        if low.starts_with("combinations") && underlined {
            mode = Mode::Combo;
            continue;
        }
        if low.starts_with("supplements") && !low[11..].starts_with(|c: char| c.is_ascii_alphanumeric()) && text.chars().count() < 40 {
            mode = Mode::Supp;
            supp_parent = cur;
            continue;
        }
        let tokens = row.all("token");
        if !tokens.is_empty() {
            if let Some(c) = cur {
                ops[c].overlaps.extend(tokens.iter().map(|t| t.text()).filter(|t| !t.is_empty()));
            }
            continue;
        }
        if tds[1].attr("align") == Some("right") && !code3.is_empty() {
            if let Some(c) = cur {
                ops[c].options.push(Opt {
                    code: code3,
                    qual: tds[1].text(),
                    time: tds.get(5).map(|t| t.text()).unwrap_or_default(),
                    cart: cart_of(tds.get(3)),
                });
            }
            continue;
        }
        let blue = row.find(|e| {
            e.tag() == "a"
                && (e.has_class("hyperlink")
                    || e.closest(|f| f.tag() == "font" && f.attr("color").is_some())
                        .is_some_and(|f| f.is_inside(&row)))
        });
        if let (Some(a), Some(c)) = (blue, cur) {
            ops[c].notes.push(Note {
                text: a.text(),
                url: window_open(a.attr("href").unwrap_or("")),
            });
            continue;
        }
        let t = tds[1].text();
        if t.is_empty() {
            continue;
        }
        match cur {
            Some(c) => ops[c].desc.push(t),
            None => group = t,
        }
    }
    for op in &mut ops {
        op.rules = overlap_rules(&op.overlaps);
    }
    Some(Report { title, ops })
}

// ---------- anything else SLTS shows (an index, a message, nothing found) ----------

/// What a link in SLTS's own markup does here
#[derive(Debug, Clone, PartialEq)]
pub enum RawLink {
    /// An operation number: search for it (its base number)
    Search(String),
    /// SLTS's own link (an index entry): its form posted back
    PostBack(String, String),
    Url(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawPiece {
    pub text: String,
    pub link: Option<RawLink>,
}

/// SLTS's page when it isn't a report: its messages, and what it shows as lines of text and links
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Raw {
    pub messages: Vec<String>,
    pub lines: Vec<Vec<RawPiece>>,
}

impl Raw {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// SLTS's search controls and standing notes: left out of what it shows
const LEFT_OUT: &[&str] = &[
    "Table_Main",
    "TableRow_Wiring_SearchDoc",
    "TableRow_DiffMarket",
    "MinimumOperationNumbers",
    "ValidateOperationNumbers",
    "DuplicateOperationNumbers",
];

fn link_of(a: &El) -> Option<RawLink> {
    let href = a.attr("href").unwrap_or("");
    let code: String = a.text().split_whitespace().collect();
    // 19700, 19700A, 19700A12: an operation number
    let b = code.as_bytes();
    let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
    let letters = b[digits..].iter().take_while(|c| c.is_ascii_alphabetic()).count();
    let tail = &b[digits + letters..];
    if (3..=6).contains(&digits) && letters <= 2 && tail.iter().all(u8::is_ascii_digit) {
        return Some(RawLink::Search(code[..digits].to_string()));
    }
    if let Some(i) = href.find("__doPostBack(") {
        let args = quoted(&href[i..]);
        if args.len() >= 2 {
            return Some(RawLink::PostBack(args[0].clone(), args[1].clone()));
        }
    }
    if href.get(..6).is_some_and(|h| h.eq_ignore_ascii_case("https:")) {
        return Some(RawLink::Url(href.to_string()));
    }
    None
}

pub fn read_raw(doc: &Doc) -> Raw {
    let messages = [
        "MinimumOperationNumbers",
        "ValidateOperationNumbers",
        "Label_InfoNotAvaiable_DiffMarket",
        "Label_DiffMarket",
    ]
    .iter()
    .filter_map(|id| doc.by_id(id).map(|e| e.text()))
    .filter(|t| !t.is_empty())
    .collect();
    let mut lines: Vec<Vec<RawPiece>> = Vec::new();
    let mut line: Vec<RawPiece> = Vec::new();
    fn end(lines: &mut Vec<Vec<RawPiece>>, line: &mut Vec<RawPiece>) {
        // the line's text tidied: pieces of text together, spaces as one
        let mut out: Vec<RawPiece> = Vec::new();
        for p in line.drain(..) {
            match (out.last_mut(), &p.link) {
                (Some(last), None) if last.link.is_none() => last.text.push_str(&p.text),
                _ => out.push(p),
            }
        }
        for p in &mut out {
            let lead = p.text.starts_with(char::is_whitespace) && p.link.is_none();
            let trail = p.text.ends_with(char::is_whitespace) && p.link.is_none();
            let mut t = clean(&p.text);
            if !t.is_empty() {
                if lead {
                    t.insert(0, ' ');
                }
                if trail {
                    t.push(' ');
                }
            } else if p.link.is_none() && !p.text.is_empty() {
                t = " ".into();
            }
            p.text = t;
        }
        out.retain(|p| !p.text.is_empty());
        if let Some(f) = out.first_mut() {
            if f.link.is_none() {
                f.text = f.text.trim_start().to_string();
            }
        }
        if let Some(l) = out.last_mut() {
            if l.link.is_none() {
                l.text = l.text.trim_end().to_string();
            }
        }
        out.retain(|p| !p.text.is_empty());
        if out.iter().any(|p| !p.text.trim().is_empty()) {
            lines.push(out);
        }
    }
    fn walk(e: El, lines: &mut Vec<Vec<RawPiece>>, line: &mut Vec<RawPiece>) {
        if e.attr("id").is_some_and(|id| LEFT_OUT.contains(&id)) {
            return;
        }
        let tag = e.tag();
        if matches!(tag, "script" | "style" | "img" | "input" | "select" | "option" | "button") {
            return;
        }
        if tag == "a" {
            let text = e.text();
            if !text.is_empty() {
                line.push(RawPiece { text, link: link_of(&e) });
            }
            return;
        }
        let block = matches!(
            tag,
            "tr" | "p" | "div" | "br" | "li" | "table" | "ul" | "ol" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "form"
        );
        if block {
            end(lines, line);
        }
        if matches!(tag, "td" | "th") {
            line.push(RawPiece {
                text: "   ".into(),
                link: None,
            });
        }
        for p in e.pieces() {
            match p {
                Piece::El(c) => walk(c, lines, line),
                Piece::Text(t) => line.push(RawPiece {
                    text: t.to_string(),
                    link: None,
                }),
            }
        }
        if block {
            end(lines, line);
        }
    }
    if let Some(f) = form(doc) {
        walk(f, &mut lines, &mut line);
    }
    end(&mut lines, &mut line);
    Raw { messages, lines }
}

// ---------- Word Search (SLTS's "Keyword Search for Operation Description" window) ----------

/// An operation Word Search found: its code (the list's value) and name
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Match {
    pub value: String,
    pub text: String,
}

impl Match {
    /// "18124A ---- Shock Absorber - Front" → ("18124A", "Shock Absorber - Front")
    pub fn split(&self) -> (Option<String>, String) {
        let t = self.text.trim();
        let code: String = t.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        let rest = t[code.len()..].trim_start();
        if !code.is_empty() && rest.starts_with('-') {
            let name = rest.trim_start_matches('-').trim();
            return (Some(code), name.to_string());
        }
        (None, t.to_string())
    }

    /// The operation's code (e.g. "18124A")
    pub fn code(&self) -> String {
        let v = clean(&self.value);
        let src = if v.is_empty() {
            self.text.split_whitespace().next().unwrap_or("").to_string()
        } else {
            v
        };
        src.to_uppercase().chars().filter(char::is_ascii_alphanumeric).collect()
    }
}

/// Word Search's address for the vehicle (from SLTS's main page)
pub fn word_url(main: &Doc) -> String {
    let h = |id: &str| hidden(main, id);
    let membership = crate::doc::percent_decode(&h("hidden_cookie_tps_membership"));
    let file = h("hidden_FileName").replace('\\', "|");
    format!(
        "AlphaIndex1.aspx?{}",
        query(&[
            ("year_select", &h("hidden_Year")),
            ("model_select", &h("hidden_Model")),
            ("vin", &h("hidden_Vin")),
            ("fileName", &file),
            ("cookie_tps_membership", &membership),
            ("tab", "tab"),
        ])
    )
}

pub fn read_matches(doc: &Doc) -> Vec<Match> {
    doc.by_id("ListBox_OperationDesc")
        .map(|l| {
            l.all("option")
                .iter()
                .map(|o| Match {
                    value: option_value(o),
                    text: o.text(),
                })
                .filter(|m| !m.text.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

// ---------- one-time use parts (SLTS's "Required Parts" list for an operation) ----------

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Parts {
    pub head: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Why there's none
    pub note: String,
}

/// The parts page's address for an option (the main page's hidden fields say which vehicle)
pub fn parts_url(fields: &[(String, String)], c: &Cart) -> String {
    let f = |n: &str| field(fields, n).to_string();
    format!(
        "DiscardPartList_New.aspx?{}",
        query(&[
            ("strVin", &f("hidden_Vin")),
            ("yearSelected", &f("hidden_Year")),
            ("modelSelected", &f("hidden_Model")),
            ("market", &f("hidden_market")),
            ("market_orgi", &f("hidden_market_orgi")),
            ("country", &f("hidden_country")),
            ("language", &f("hidden_language")),
            ("opNumBase", &c.op_num_base),
            ("opNum", &c.op_num),
            ("disPrtNo", &c.part_no),
            ("disPrtQty", &c.qty),
            ("disPrtDesc", &c.desc),
            ("qualDesc", &c.qual),
            ("filename", &f("hidden_FileName")),
        ])
    )
}

/// The parts page is a plain table: whatever columns it has (its biggest innermost table)
pub fn read_parts_page(html: &str) -> Parts {
    let doc = Doc::parse(html);
    let mut best: Option<Vec<Vec<String>>> = None;
    for t in doc.root().all("table") {
        if t.first("table").is_some() {
            continue; // innermost tables only
        }
        let rows: Vec<Vec<String>> = rows(t)
            .iter()
            .map(|r| cells(*r).iter().map(|c| c.text()).collect::<Vec<_>>())
            .filter(|r| r.iter().any(|c| !c.is_empty()))
            .collect();
        if rows.len() >= 2 && rows[0].len() >= 2 && best.as_ref().is_none_or(|b| rows.len() > b.len()) {
            best = Some(rows);
        }
    }
    let Some(mut best) = best else {
        let body = doc.root().first("body").map(|b| b.text()).unwrap_or_else(|| doc.root().text());
        return Parts {
            note: body.chars().take(200).collect(),
            ..Default::default()
        };
    };
    let head = best.remove(0);
    let rows = best.into_iter().filter(|r| r.len() >= 2 && r.join("") != head.join("")).collect();
    Parts {
        head,
        rows,
        note: String::new(),
    }
}

/// The parts for an option, with SLTS's link's own first item when the page couldn't be read
pub fn parts_or_link(mut p: Parts, c: &Cart) -> Parts {
    if p.rows.is_empty() && !c.part_no.is_empty() {
        p.head = vec!["Part / item".into(), "Description".into(), "Qty".into()];
        p.rows = vec![vec![c.part_no.clone(), c.desc.clone(), c.qty.clone()]];
    }
    p
}

// ---------- the tray ----------

/// The tray's warnings: options Ford says not to claim together, combinations without their base operation
pub fn tray_warnings(tray: &[TrayItem]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |w: String| {
        if !out.contains(&w) {
            out.push(w);
        }
    };
    for t in tray {
        for r in &t.rules {
            if !matches(&r.subject, &t.code) {
                continue;
            }
            for other in tray.iter().map(|x| &x.code) {
                if other == &t.code {
                    continue;
                }
                if r.not.iter().any(|p| matches(p, other)) {
                    let mut pair = [t.code.clone(), other.clone()];
                    pair.sort();
                    add(format!(
                        "{} and {} overlap: Ford says not to claim them together{}.",
                        pair[0],
                        pair[1],
                        if r.cond.is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", r.cond)
                        }
                    ));
                }
            }
        }
        if t.kind == OpKind::Combo && !t.base.is_empty() && !tray.iter().any(|x| x.kind == OpKind::Base && x.code.starts_with(&t.base)) {
            add(format!(
                "{} is a combination: also claim its base operation {}{}.",
                t.code,
                t.base,
                if t.base_name.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", t.base_name)
                }
            ));
        }
    }
    out
}

/// The tray's warranty time, and how many have no set time (actual time)
pub fn tray_total(tray: &[TrayItem]) -> (f64, usize) {
    let mut w = 0.0;
    let mut actual = 0;
    for t in tray {
        match num(&t.time) {
            Some(n) => w += n,
            None => actual += 1,
        }
    }
    (w, actual)
}

/// An option as it goes in the tray
pub fn tray_item(report: &Report, op: &Op, opt: &Opt) -> TrayItem {
    let base = report.ops.iter().find(|o| !op.parent.is_empty() && o.code == op.parent);
    TrayItem {
        code: opt.code.clone(),
        qual: opt.qual.clone(),
        time: opt.time.clone(),
        name: op.name.clone(),
        kind: op.kind,
        base: base.map(|b| b.code.clone()).unwrap_or_default(),
        base_name: base.map(|b| b.name.clone()).unwrap_or_default(),
        cart: opt.cart.clone(),
        rules: op.rules.clone(),
    }
}

// ---------- asking SLTS ----------

mod asking {
    use super::*;
    use crate::{Error, Ford, Result};
    use workbench_net::{Http, Method};

    /// Inside SLTS's folder, or nothing
    pub fn slts_url(path: &str) -> Option<String> {
        let base = workbench_net::Url::parse(SLTS_BASE).ok()?;
        let u = workbench_net::join(&base, path).ok()?;
        let ok = u.scheme() == base.scheme()
            && u.host_str() == base.host_str()
            && u.path().to_ascii_lowercase().starts_with("/vdirsnet/sltsvinwebview/");
        ok.then(|| u.to_string())
    }

    fn short(s: &str, n: usize) -> String {
        s.chars().take(n).collect()
    }

    impl Ford {
        /// One SLTS page: a GET, or its form posted back (`fields`)
        async fn slts_request(&self, url: &str, fields: Option<&[(String, String)]>) -> Result<SltsPage> {
            let href = slts_url(url).ok_or_else(|| Error::Format("Not an SLTS address".into()))?;
            let u = workbench_net::Url::parse(&href).map_err(|_| Error::Format("SLTS address".into()))?;
            let mut req = Http::request(if fields.is_some() { Method::POST } else { Method::GET }, u)
                .header("Accept", "text/html,*/*")
                .header("Referer", href.clone());
            if let Some(f) = fields {
                req = req.header("Content-Type", "application/x-www-form-urlencoded").body(form_body(f));
            }
            let res = self.fetch(req).await?;
            self.log(&format!("   {} characters", res.body.len()));
            let low = res.body.to_ascii_lowercase();
            if crate::looks_like_sign_in(&res.body) && !low.contains("slts") && !low.contains("labor") {
                return Err(Error::NeedsLogin);
            }
            // an SLTS page without its form is an error page: what it says goes in the log (its words only)
            if !low.contains("<form") {
                let d = Doc::parse(&res.body);
                self.log(&format!(
                    "   labor: SLTS sent a page with no form ({}): {}",
                    short(&d.title(), 120),
                    short(&d.root().text(), 300)
                ));
            }
            Ok(SltsPage {
                url: res.url.to_string(),
                html: res.body,
            })
        }

        /// Opens SLTS for this VIN (GenerateT.aspx). The vehicle's SLTS model code (e.g. "A8") is remembered per VIN.
        pub async fn slts_start(&self, vin: &str) -> Result<SltsPage> {
            let book = self.book(vin).await?;
            let v = &book.vehicle;
            let known = self.slts_codes.lock().expect("slts lock").get(&book.vin).cloned();
            let or = |a: &str, b: &str| if a.is_empty() { b.to_string() } else { a.to_string() };
            let year = or(&v.year, book.param("modelYear"));
            let name = or(&v.market_name, &v.display_name);
            let iso = or(&or(book.param("isoLang"), book.param("isolang")), "EN-US");
            let country = or(book.param("country"), "USA");
            let q = |code: &str| {
                query(&[
                    ("strVin", &book.vin),
                    ("strYear", &year),
                    ("strModelDesc", &name),
                    ("strGsltsVehCode", code),
                    ("usercountry", &country),
                    ("isolang", &iso),
                    ("masterid", book.param("vehicleId")),
                ])
            };
            // GenerateT.aspx works out the model code itself when it's left empty
            let mut tries = vec![format!("GenerateT.aspx?{}", q(known.as_deref().unwrap_or("")))];
            if known.is_some() {
                tries.push(format!("GenerateT.aspx?{}", q("")));
            }
            let mut last = String::new();
            for t in &tries {
                let href = slts_url(t).ok_or_else(|| Error::Format("SLTS address".into()))?;
                let u = workbench_net::Url::parse(&href).map_err(|_| Error::Format("SLTS address".into()))?;
                let req = Http::request(Method::GET, u)
                    .header("Accept", "text/html,*/*")
                    .header("Referer", format!("{}/", crate::paths::PTS));
                let res = match self.fetch(req).await {
                    Ok(r) => r,
                    Err(Error::NeedsLogin) => return Err(Error::NeedsLogin),
                    Err(e) => {
                        last = e.to_string();
                        self.log(&format!("   SLTS: GenerateT failed: {e}"));
                        continue;
                    }
                };
                if is_slts_page(&res.body, &book.vin) {
                    let from_url = res
                        .url
                        .query_pairs()
                        .find(|(k, _)| k.eq_ignore_ascii_case("strGsltsVehCode"))
                        .map(|(_, v)| v.to_string())
                        .filter(|v| !v.is_empty());
                    let code = from_url.or_else(|| {
                        let d = Doc::parse(&res.body);
                        let info = hidden(&d, "hidden_VinInfo");
                        let mut parts = info.split('|');
                        let year = parts.next().unwrap_or("");
                        let code = parts.next().unwrap_or("");
                        (year.len() == 4 && year.chars().all(|c| c.is_ascii_digit()) && !code.is_empty()).then(|| code.to_string())
                    });
                    self.log(&format!("   SLTS opened (model code {})", code.as_deref().unwrap_or("?")));
                    if let Some(c) = code {
                        self.slts_codes.lock().expect("slts lock").insert(book.vin.clone(), c);
                    }
                    return Ok(SltsPage {
                        url: res.url.to_string(),
                        html: res.body,
                    });
                }
                if crate::looks_like_sign_in(&res.body) {
                    return Err(Error::NeedsLogin);
                }
                last = short(&Doc::parse(&res.body).root().text(), 200);
                self.log(&format!(
                    "   SLTS: GenerateT gave something else ({}): {last}",
                    short(res.url.as_str(), 160)
                ));
            }
            Err(Error::Format(format!(
                "Couldn't open SLTS for this vehicle{}",
                if last.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", short(&last, 120))
                }
            )))
        }

        /// Posts SLTS's form back: a dropdown changed (`target`), or its View button (`click`); `overrides` on top of
        /// what the form already holds. Opens SLTS first when it isn't yet; starts it over once when its session ran
        /// out. The page SLTS sent back.
        pub async fn slts_post_back(
            &self,
            vin: &str,
            page: Option<SltsPage>,
            overrides: &[(String, String)],
            target: &str,
            argument: &str,
            click: bool,
        ) -> Result<SltsPage> {
            let page = match page {
                Some(p) => p,
                None => self.slts_start(vin).await?,
            };
            let fill = |p: &SltsPage, arg: &str| {
                let d = p.doc();
                let mut f = form_fields(&d);
                for (k, v) in overrides {
                    set(&mut f, k, v.clone());
                }
                set(&mut f, "__EVENTTARGET", target);
                set(&mut f, "__EVENTARGUMENT", arg);
                if click {
                    set(&mut f, "ImageButton_View_SearchFor.x", "9");
                    set(&mut f, "ImageButton_View_SearchFor.y", "9");
                }
                (resolve(&p.url, &form_action(&d)), f)
            };
            let (url, f) = fill(&page, argument);
            let next = self.slts_request(&url, Some(&f)).await?;
            if next.html.contains("DropDownList_SearchType") {
                return Ok(next);
            }
            // the session ran out: start over once
            self.log("   labor: SLTS's session ran out: opening it again");
            let fresh = self.slts_start(vin).await?;
            let (url, f) = fill(&fresh, "");
            self.slts_request(&url, Some(&f)).await
        }

        /// Word Search: 1. its window for this vehicle (AlphaIndex1.aspx), 2. the words posted → the operations named
        /// so. Also the main page (opened when it wasn't), and what Word Search's page said when it found none.
        pub async fn slts_word_search(&self, vin: &str, page: Option<SltsPage>, words: &str) -> Result<(SltsPage, Vec<Match>, String)> {
            let page = match page {
                Some(p) => p,
                None => self.slts_start(vin).await?,
            };
            let url = word_url(&page.doc());
            let first = self.slts_request(&url, None).await?;
            let fd = first.doc();
            if fd.by_id("TextBox_OperationDesc").is_none() {
                return Err(Error::Format("SLTS's word search didn't open".into()));
            }
            let mut f = form_fields(&fd);
            set(&mut f, "__EVENTTARGET", "");
            set(&mut f, "__EVENTARGUMENT", "");
            set(&mut f, "TextBox_OperationDesc", words);
            set(&mut f, "ImageButton_View_OperationDesc.x", "9");
            set(&mut f, "ImageButton_View_OperationDesc.y", "9");
            let found = self.slts_request(&resolve(&first.url, &form_action(&fd)), Some(&f)).await?;
            let d = found.doc();
            let list = read_matches(&d);
            self.log(&format!(
                "   labor: word search: {} match{}",
                list.len(),
                if list.len() == 1 { "" } else { "es" }
            ));
            let said = if list.is_empty() {
                let t = form(&d).map(|f| f.text()).unwrap_or_default();
                clean(&t.replace("Keyword Search for Operation Description", "").replace("View Cancel", ""))
            } else {
                String::new()
            };
            Ok((page, list, said))
        }

        /// One-time use parts for an option (SLTS's "Required Parts" list); the main page says which vehicle
        pub async fn slts_parts(&self, vin: &str, page: Option<SltsPage>, cart: &Cart) -> Parts {
            let page = match page {
                Some(p) => Some(p),
                None => self.slts_start(vin).await.ok(),
            };
            let fields = page.map(|p| form_fields(&p.doc())).unwrap_or_default();
            let read = match self.slts_request(&parts_url(&fields, cart), None).await {
                Ok(p) => read_parts_page(&p.html),
                Err(e) => Parts {
                    note: format!("Couldn't get the full list from SLTS ({e})."),
                    ..Default::default()
                },
            };
            parts_or_link(read, cart)
        }
    }

    /// An address relative to a page's
    fn resolve(page: &str, action: &str) -> String {
        let Ok(base) = workbench_net::Url::parse(if page.is_empty() { SLTS_BASE } else { page }) else {
            return action.to_string();
        };
        let a = if action.is_empty() { page } else { action };
        workbench_net::join(&base, a)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| page.to_string())
    }
}

pub use asking::slts_url;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(base_number("18124A"), "18124");
        assert_eq!(base_number("19700D12"), "19700");
        assert_eq!(base_number("19700A4WA"), "19700");
        assert_eq!(base_number("5212BF"), "5212");
        assert_eq!(base_number("ABC"), "ABC");
        assert_eq!(base_number("19700"), "19700");
        assert!(is_base("19700A") && !is_base("19700A4") && !is_base("A"));
        assert_eq!(op_id_of("5212BF"), "5212B");
        assert_eq!(op_id_of("18125AT"), "18125A");
        assert_eq!(op_id_of("19700A4"), "19700A");
        assert_eq!(
            (hrs(1.5), hrs(1.25), hrs(2.0), hrs(0.1 + 0.2)),
            ("1.5".into(), "1.25".into(), "2.0".into(), "0.3".into())
        );
        assert_eq!((num("1.2"), num("AT"), num(" 3 "), num("1.")), (Some(1.2), None, Some(3.0), None));
        assert!(matches("6007A34*", "6007A345") && !matches("6007A1", "6007A12"));
    }

    #[test]
    fn rules() {
        let r = overlap_rules(&[
            "When claiming 19700A4: Do not use with; 6007A1, 6007A34*, 12345.".into(),
            "When claiming 18124A: Do not use with 5212B For vehicles with 2.0L".into(),
            "Something else".into(),
        ]);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].subject, "19700A4");
        assert_eq!(r[0].not, ["6007A1", "6007A34*", "12345"]);
        assert_eq!(
            (r[1].not.clone(), r[1].cond.as_str()),
            (vec!["5212B".to_string()], "For vehicles with 2.0L")
        );
    }

    const FORM: &str = r#"<html><body><form name="Form1" method="post" action="./SLTSMain.aspx?x=1" id="Form1">
      <input type="hidden" name="__VIEWSTATE" id="__VIEWSTATE" value="abc+/=" />
      <input type="hidden" name="hidden_Vin" id="hidden_Vin" value="1FTFW1E50PFA00000">
      <input type="hidden" name="hidden_Year" id="hidden_Year" value="2023">
      <input type="hidden" name="hidden_Model" id="hidden_Model" value="F-150">
      <input type="hidden" name="hidden_FileName" id="hidden_FileName" value="a\b.xml">
      <input type="hidden" name="hidden_cookie_tps_membership" id="hidden_cookie_tps_membership" value="x%3Dy">
      <table id="Table_Main"><tr id="TableRow_Form"><td>
        <span id="Label_SearchFor">Operation Number</span>
        <select name="DropDownList_SearchType" onchange="__doPostBack('DropDownList_SearchType','')" id="DropDownList_SearchType">
          <option value="Section">Section</option>
          <option selected="selected" value="Operation">Operation Number</option>
          <option value="Word">Word Search</option>
        </select>
        <input name="TextBox_OperationOrPartNumber" type="text" value="19700" id="TextBox_OperationOrPartNumber">
        <input type="image" name="ImageButton_View_SearchFor" src="v.gif">
        <select name="ListBox_Empty" size="4"></select>
        <input type="checkbox" name="Check_On" checked> <input type="checkbox" name="Check_Off">
        <input name="TextBox_OperationNumber1" type="text">
      </td></tr></table>
      <div id="MinimumOperationNumbers">Enter at least 3 digits</div>
    </form></body></html>"#;

    #[test]
    fn reads_the_form() {
        let d = Doc::parse(FORM);
        let f = form_fields(&d);
        let names: Vec<&str> = f.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            names,
            [
                "__VIEWSTATE",
                "hidden_Vin",
                "hidden_Year",
                "hidden_Model",
                "hidden_FileName",
                "hidden_cookie_tps_membership",
                "DropDownList_SearchType",
                "TextBox_OperationOrPartNumber",
                "Check_On",
                "TextBox_OperationNumber1"
            ]
        );
        assert_eq!(field(&f, "DropDownList_SearchType"), "Operation");
        assert_eq!(field(&f, "Check_On"), "on");
        assert_eq!(form_action(&d), "./SLTSMain.aspx?x=1");
        assert_eq!(form_body(&[("a b".into(), "c+/=".into())]), "a+b=c%2B%2F%3D");
        let s = read_form(&d);
        assert_eq!(s.current, "Operation");
        assert_eq!(s.types.len(), 3);
        assert_eq!(s.text_box.as_deref(), Some("TextBox_OperationOrPartNumber"));
        assert_eq!((s.text_value.as_str(), s.label.as_str()), ("19700", "Operation Number"));
        assert!(s.shows_text() && s.pick.is_none());
        assert_eq!(
            word_url(&d),
            "AlphaIndex1.aspx?year_select=2023&model_select=F-150&vin=1FTFW1E50PFA00000&fileName=a%7Cb.xml&cookie_tps_membership=x%3Dy&tab=tab"
        );
        let raw = read_raw(&d);
        assert_eq!(raw.messages, ["Enter at least 3 digits"]);
        assert!(raw.is_empty());
    }

    // trimmed from an SLTS report: one operation with two options and parts, a combination, a supplement, overlaps
    const REPORT: &str = r##"<html><body><form id="Form1" action="x">
      <table><tr id="TableRow_XmlContent"><td>
      <table><tr><td class="copysubtitle">Section 19 - Climate Control</td></tr></table>
      <table>
        <tr><td></td><td>Operation Description</td><td></td><td width="100">Operation Number</td><td></td><td>Time</td></tr>
        <tr><td></td><td><b><font color="#003399">ALL OPERATIONS - REMOVE AND INSTALL</font></b></td><td></td><td width="100"></td><td></td><td></td></tr>
        <tr><td></td><td><table><tr><td><b>Compressor - A/C (<a href="javascript:partlink('19703')">19703</a>)</b></td>
            <td><a href="#top">▲</a></td></tr></table></td><td></td><td width="100"></td><td></td><td></td></tr>
        <tr><td></td><td>Includes: Recover and recharge.</td><td></td><td width="100"></td><td></td><td></td></tr>
        <tr><td></td><td align="right">2.0L GTDI</td><td></td>
            <td width="100"><a href="javascript:discardPartList('19703A','19703A1','W123','2','O-RING &quot;A&quot;','2.0L')">19703A1</a></td><td></td><td>2.4</td></tr>
        <tr><td></td><td align="right">3.5L</td><td></td><td width="100">19703A2</td><td></td><td>AT</td></tr>
        <tr><td></td><td><token>When claiming 19703A1: Do not use with; 19704A*.</token></td><td></td><td width="100"></td><td></td><td></td></tr>
        <tr><td></td><td><font color="blue"><a href="javascript:window.open('Note.aspx?n=1')">See note</a></font></td><td></td><td width="100"></td><td></td><td></td></tr>
        <tr><td></td><td><b><u>Combinations</u></b></td><td></td><td width="100"></td><td></td><td></td></tr>
        <tr><td></td><td><table><tr><td><b><a name="19703A40"></a>Replace Accumulator</b></td></tr></table></td><td></td><td width="100">19703A40</td><td></td><td>0.3</td></tr>
        <tr><td></td><td>Supplements</td><td></td><td width="100"></td><td></td><td></td></tr>
        <tr><td></td><td><table><tr><td><b><a name="19703A4012"></a>Flush System</b></td></tr></table></td><td></td><td width="100">19703A4012</td><td></td><td>0.5</td></tr>
      </table>
      </td></tr></table></form></body></html>"##;

    #[test]
    fn reads_the_report() {
        let r = parse_report(&Doc::parse(REPORT)).expect("a report");
        assert_eq!(r.title, "Section 19 - Climate Control");
        assert_eq!(r.ops.len(), 3);
        let a = &r.ops[0];
        assert_eq!(
            (a.code.as_str(), a.name.as_str(), a.kind),
            ("19703A", "Compressor - A/C", OpKind::Base)
        );
        assert_eq!(a.parts, ["19703"]);
        assert_eq!(a.group, "ALL OPERATIONS - REMOVE AND INSTALL");
        assert_eq!(a.desc, ["Includes: Recover and recharge."]);
        assert_eq!(a.options.len(), 2);
        assert_eq!(
            (a.options[0].code.as_str(), a.options[0].qual.as_str(), a.options[0].time.as_str()),
            ("19703A1", "2.0L GTDI", "2.4")
        );
        let c = a.options[0].cart.as_ref().expect("parts");
        assert_eq!(
            (c.op_num.as_str(), c.part_no.as_str(), c.desc.as_str()),
            ("19703A1", "W123", "O-RING \"A\"")
        );
        assert_eq!(a.options[1].time, "AT");
        assert_eq!(
            a.notes,
            [Note {
                text: "See note".into(),
                url: "Note.aspx?n=1".into()
            }]
        );
        assert_eq!(a.rules[0].not, ["19704A*"]);
        let combo = &r.ops[1];
        assert_eq!(
            (combo.code.as_str(), combo.kind, combo.parent.as_str()),
            ("19703A40", OpKind::Combo, "19703A")
        );
        assert_eq!(combo.options[0].time, "0.3");
        let supp = &r.ops[2];
        assert_eq!((supp.kind, supp.parent.as_str()), (OpKind::Supp, "19703A40"));
        assert_eq!(r.roots(), [0]);
        assert_eq!(r.children(0), [1]);
        assert_eq!(r.children(1), [2]);
        assert_eq!(r.find("19703A2"), Some(0));
        assert_eq!(r.find("19703A40"), Some(1));

        // the tray: a combination without its base, and an overlap
        let mut tray = vec![tray_item(&r, &r.ops[1], &r.ops[1].options[0])];
        assert_eq!(tray[0].base, "19703A");
        assert!(tray_warnings(&tray)[0].contains("also claim its base operation 19703A (Compressor - A/C)"));
        tray.push(tray_item(&r, &r.ops[0], &r.ops[0].options[0]));
        tray.push(TrayItem {
            code: "19704A".into(),
            time: "1.0".into(),
            ..Default::default()
        });
        tray.push(tray_item(&r, &r.ops[0], &r.ops[0].options[1]));
        assert_eq!(
            tray_warnings(&tray),
            ["19703A1 and 19704A overlap: Ford says not to claim them together."]
        );
        let (w, actual) = tray_total(&tray);
        assert_eq!((hrs(w), actual), ("3.7".into(), 1));
        // kept in the Electron app's format
        let s = Saved {
            tray: tray[..1].to_vec(),
            kind: "Operation".into(),
            pick: String::new(),
            q: "19703".into(),
        };
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["type"], "Operation");
        assert_eq!(json["tray"][0]["kind"], "combo");
        assert_eq!(json["tray"][0]["baseName"], "Compressor - A/C");
        assert_eq!(json["tray"][0]["cart"], serde_json::Value::Null);
        let back: Saved = serde_json::from_value(json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn reads_other_pages() {
        let html = r##"<form><div id="MinimumOperationNumbers">x</div><table>
          <tr><td><a href="javascript:__doPostBack('Index$1','A')">A</a></td><td>Air bags</td></tr>
          <tr><td><a href="#">19700</a> Compressor</td></tr>
          <tr><td><input type="hidden" value="secret"><select><option>Pick</option></select></td></tr></table></form>"##;
        let raw = read_raw(&Doc::parse(html));
        assert_eq!(raw.lines.len(), 2);
        assert_eq!(raw.lines[0][0].link, Some(RawLink::PostBack("Index$1".into(), "A".into())));
        assert_eq!(raw.lines[0][1].text, " Air bags");
        assert_eq!(raw.lines[1][0].link, Some(RawLink::Search("19700".into())));
        assert_eq!(raw.lines[1][1].text, " Compressor");

        let m = Match {
            value: "18124A".into(),
            text: "18124A ---- Shock Absorber - Front".into(),
        };
        assert_eq!(m.split(), (Some("18124A".into()), "Shock Absorber - Front".into()));
        assert_eq!(m.code(), "18124A");

        let p = read_parts_page(
            "<table><tr><td>Part</td><td>Description</td><td>Qty</td></tr><tr><td>W123</td><td>O-ring</td><td>2</td></tr></table>",
        );
        assert_eq!((p.head.len(), p.rows.len()), (3, 1));
        let none = parts_or_link(
            read_parts_page("<p>Nothing</p>"),
            &Cart {
                part_no: "W1".into(),
                ..Default::default()
            },
        );
        assert_eq!(none.rows, [vec!["W1".to_string(), String::new(), String::new()]]);
    }
}