//! Ford's documents (TSB / GSB / SSM articles, FSA documents; the Workshop Manual's procedures later) turned into a
//! plain document the app draws in its own style ([`Block`]s of [`Inline`]s): headings, paragraphs, lists, tables,
//! pictures, rules. Ford's own styling, scripts, forms and hidden parts are left out; links and pictures get full
//! addresses. (The Electron app's `tsb/article.js` `redraw`, without a browser.)

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use crate::html::{Doc, El, Piece};

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text {
        text: String,
        bold: bool,
        italic: bool,
    },
    /// A link: its text, and the full address it opens
    Link {
        text: String,
        url: String,
    },
    Break,
}

/// A paragraph that's a warning, caution, notice or note (it starts with that word)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Callout {
    Warning,
    Caution,
    Notice,
    Note,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(u8, Vec<Inline>),
    Para(Vec<Inline>, Option<Callout>),
    List {
        ordered: bool,
        items: Vec<Vec<Block>>,
    },
    /// `grid`: a real table (with borders or headings), not just Ford's page layout
    Table {
        grid: bool,
        rows: Vec<Vec<Cell>>,
    },
    /// A picture: its full address, and what it shows
    Image {
        url: String,
        alt: String,
    },
    Rule,
    /// A Workshop Manual pinpoint test: answered one step at a time
    Pinpoint(Box<Pinpoint>),
    /// A place in a procedure links go to (Ford's `<a name="…">`): the block after it starts there
    Anchor(Vec<String>),
}

/// A pinpoint test ("PINPOINT TEST A: ACCESSORY DRIVE BELT NOISE"): its introduction, then its steps, each a check
/// with a question and its answers (Yes / No…), each answer going on to another step or ending the test
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pinpoint {
    /// Ford's letter for it ("A")
    pub name: String,
    pub title: String,
    /// Ford's id for it, from the anchor before it ("G2198756p1"), when there's one
    pub anchor: String,
    /// Newer tests: the file their steps are in, fetched when the test is opened (no steps here until then)
    pub spa: String,
    /// An older test, laid out as one table: Ford shows it all at once and records only its start
    pub legacy: bool,
    pub intro: Vec<Block>,
    pub steps: Vec<Step>,
}

/// One step of a pinpoint test
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Step {
    /// Ford's id for it, the one "GO to" links name ("G2198756t2")
    pub id: String,
    /// "A2"
    pub label: String,
    /// "CHECK THE ACCESSORY DRIVE BELT FOR CRACKING"
    pub title: String,
    /// What it asks ("Can the source of the noise be identified?")
    pub question: String,
    /// What to do (the step without its title, question and answers)
    pub blocks: Vec<Block>,
    pub answers: Vec<Answer>,
}

/// An answer to a step: "Yes", "No" (or what Ford calls it), what it says to do, and the steps it goes on to
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Answer {
    /// Ford's id for it ("Yes"), as Vehicle History is told
    pub id: String,
    /// As shown ("Yes")
    pub label: String,
    pub blocks: Vec<Block>,
    /// The steps (Ford's ids) it goes on to: one, or a choice of a few ("built before May: A3; after: A4")
    pub jumps: Vec<String>,
}

impl Answer {
    /// It ends the test (INSTALL a new component…) instead of going on to another step
    pub fn ends(&self) -> bool {
        self.jumps.is_empty()
    }
}

impl Pinpoint {
    pub fn step(&self, id: &str) -> Option<&Step> {
        self.steps.iter().find(|s| s.id == id)
    }

    /// Something to step through (steps, or steps to fetch); otherwise just what Ford says about it
    pub fn has_steps(&self) -> bool {
        !self.steps.is_empty() || !self.spa.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub blocks: Vec<Block>,
    pub header: bool,
    pub span: u16,
}

impl Block {
    /// Every picture's address in these blocks (in order, each once)
    pub fn images(blocks: &[Block]) -> Vec<String> {
        let mut out = Vec::new();
        fn walk(bs: &[Block], out: &mut Vec<String>) {
            for b in bs {
                match b {
                    Block::Image { url, .. } if !out.contains(url) => out.push(url.clone()),
                    Block::List { items, .. } => items.iter().for_each(|i| walk(i, out)),
                    Block::Table { rows, .. } => rows.iter().flatten().for_each(|c| walk(&c.blocks, out)),
                    Block::Pinpoint(p) => {
                        walk(&p.intro, out);
                        for s in &p.steps {
                            walk(&s.blocks, out);
                            s.answers.iter().for_each(|a| walk(&a.blocks, out));
                        }
                    }
                    _ => {}
                }
            }
        }
        walk(blocks, &mut out);
        out
    }

    /// Every pinpoint test in these blocks, in order
    pub fn pinpoints(blocks: &[Block]) -> Vec<&Pinpoint> {
        let mut out = Vec::new();
        fn walk<'a>(bs: &'a [Block], out: &mut Vec<&'a Pinpoint>) {
            for b in bs {
                match b {
                    Block::Pinpoint(p) => out.push(p),
                    Block::List { items, .. } => items.iter().for_each(|i| walk(i, out)),
                    Block::Table { rows, .. } => rows.iter().flatten().for_each(|c| walk(&c.blocks, out)),
                    _ => {}
                }
            }
        }
        walk(blocks, &mut out);
        out
    }

    /// A pinpoint test by its letter, to change (its steps, once they're fetched)
    pub fn pinpoint_mut<'a>(blocks: &'a mut [Block], name: &str) -> Option<&'a mut Pinpoint> {
        for b in blocks {
            let found = match b {
                Block::Pinpoint(p) if p.name == name => return Some(p),
                Block::List { items, .. } => items.iter_mut().find_map(|i| Block::pinpoint_mut(i, name)),
                Block::Table { rows, .. } => rows.iter_mut().flatten().find_map(|c| Block::pinpoint_mut(&mut c.blocks, name)),
                _ => None,
            };
            if found.is_some() {
                return found;
            }
        }
        None
    }

    /// All the text in these blocks (to tell whether there's anything to read)
    pub fn text(blocks: &[Block]) -> String {
        let mut out = String::new();
        fn inl(i: &[Inline], out: &mut String) {
            for x in i {
                match x {
                    Inline::Text { text, .. } | Inline::Link { text, .. } => out.push_str(text),
                    Inline::Break => out.push(' '),
                }
            }
            out.push(' ');
        }
        fn walk(bs: &[Block], out: &mut String) {
            for b in bs {
                match b {
                    Block::Heading(_, i) | Block::Para(i, _) => inl(i, out),
                    Block::List { items, .. } => items.iter().for_each(|i| walk(i, out)),
                    Block::Table { rows, .. } => rows.iter().flatten().for_each(|c| walk(&c.blocks, out)),
                    Block::Pinpoint(p) => {
                        out.push_str(&p.title);
                        out.push(' ');
                        walk(&p.intro, out);
                        for s in &p.steps {
                            out.push_str(&format!("{} {} {} ", s.label, s.title, s.question));
                            walk(&s.blocks, out);
                        }
                    }
                    _ => {}
                }
            }
        }
        walk(blocks, &mut out);
        crate::html::clean(&out)
    }
}

/// A full address for `rel` on the page at `base` ("https://…", "/path", "page.aspx?x", "../x")
pub fn resolve(base: &str, rel: &str) -> Option<String> {
    let rel = rel.trim();
    if rel.is_empty() || rel.starts_with('#') {
        return None;
    }
    let lower = rel.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return Some(rel.to_string());
    }
    if lower.starts_with("data:") || lower.starts_with("javascript:") || lower.starts_with("mailto:") {
        return None;
    }
    let (scheme, rest) = base.split_once("://")?;
    let host_end = rest.find('/').unwrap_or(rest.len());
    let origin = format!("{scheme}://{}", &rest[..host_end]);
    if let Some(r) = rel.strip_prefix("//") {
        return Some(format!("{scheme}://{r}"));
    }
    if rel.starts_with('/') {
        return Some(format!("{origin}{rel}"));
    }
    // relative to the base's folder
    let path = rest[host_end..].split(['?', '#']).next().unwrap_or("/");
    let mut parts: Vec<&str> = path.split('/').collect();
    parts.pop(); // the page itself
    let (rel_path, query) = match rel.find(['?', '#']) {
        Some(q) => (&rel[..q], &rel[q..]),
        None => (rel, ""),
    };
    for seg in rel_path.split('/') {
        match seg {
            "." => {}
            ".." => {
                if parts.len() > 1 {
                    parts.pop();
                }
            }
            s => parts.push(s),
        }
    }
    let joined = parts.join("/");
    let joined = if joined.starts_with('/') { joined } else { format!("/{joined}") };
    Some(format!("{origin}{joined}{query}"))
}

/// A query value from an address ("…?id=26-2414&t=Engine%20noise" → "26-2414"), decoded
pub fn query_value(url: &str, key: &str) -> Option<String> {
    let q = url.split_once('?')?.1.split('#').next().unwrap_or("");
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        k.eq_ignore_ascii_case(key).then(|| percent_decode(v))
    })
}

pub(crate) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                Ok(v) => {
                    out.push(v);
                    i += 3;
                    continue;
                }
                Err(_) => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What `href` opens: `javascript:window.open('x')` → x; a jump within the page (#x) → none
fn link_target(href: &str) -> Option<&str> {
    let h = href.trim();
    if h.to_ascii_lowercase().starts_with("javascript:") {
        let at = h.find("open(")? + 5;
        let rest = h[at..].trim_start();
        let q = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
        let end = rest[1..].find(q)?;
        return Some(&rest[1..1 + end]);
    }
    (!h.is_empty() && !h.starts_with('#')).then_some(h)
}

/// A picture that's only there for Ford's page layout (spacers, print buttons, icons, tiny ones)
fn layout_picture(e: &El) -> bool {
    let src = e.attr("src").unwrap_or("").to_ascii_lowercase();
    let tiny = |a: &str| {
        e.attr(a)
            .and_then(|v| v.trim_end_matches("px").parse::<u32>().ok())
            .is_some_and(|n| n <= 4)
    };
    src.is_empty()
        || [
            "spacer",
            "empty.gif",
            "blank.gif",
            "printer",
            "print.gif",
            "print.png",
            "print.jp",
            "/icons/",
        ]
        .iter()
        .any(|w| src.contains(w))
        || tiny("width")
        || tiny("height")
}

struct Builder<'b> {
    base: &'b str,
    out: Vec<Block>,
    cur: Vec<Inline>,
    bold: u32,
    italic: u32,
    link: Option<String>,
    /// Reading a Workshop Manual procedure (this one's): its links to other procedures, and Ford's page furniture
    proc: Option<&'b ProcCtx>,
    /// Ford's "Click for details": the next element is shown even though Ford hides it
    reveal_next: bool,
    /// Elements read already, or shown apart (a pinpoint test's body; a step's title, question and answers)
    skip: Rc<RefCell<HashSet<usize>>>,
    /// The last anchor's name (`<a name="…">`): a pinpoint test's id, when its title comes next
    anchor: Option<String>,
    /// Anchors not yet put in (they go before the next block)
    marks: Vec<String>,
}

/// The procedure being read (to tell links within it from links to others)
#[derive(Debug, Clone, Default)]
pub struct ProcCtx {
    pub search_number: String,
    pub proc_uid: String,
}

/// A link to a Workshop Manual procedure, as an address the app understands ("proc:sn=…&uid=…&type=…&hash=…&ppt=…")
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcLink {
    pub search_number: String,
    pub proc_uid: String,
    /// "Procedure" or "PublicationContent"
    pub link_type: String,
    /// A part of it to go to
    pub hash: String,
    /// A pinpoint test in it to go to
    pub ppt: String,
}

fn form_enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl ProcLink {
    pub fn to_url(&self) -> String {
        format!(
            "proc:sn={}&uid={}&type={}&hash={}&ppt={}",
            form_enc(&self.search_number),
            form_enc(&self.proc_uid),
            form_enc(&self.link_type),
            form_enc(&self.hash),
            form_enc(&self.ppt)
        )
    }

    pub fn from_url(url: &str) -> Option<ProcLink> {
        let q = url.strip_prefix("proc:")?;
        let get = |k: &str| query_value(&format!("x?{q}"), k).unwrap_or_default();
        Some(ProcLink {
            search_number: get("sn"),
            proc_uid: get("uid"),
            link_type: Some(get("type")).filter(|t| !t.is_empty()).unwrap_or_else(|| "Procedure".into()),
            hash: get("hash"),
            ppt: get("ppt"),
        })
    }

    /// Which open procedure it is (the same procedure, wherever it's opened from)
    pub fn key(&self) -> String {
        format!("{}|{}|{}", self.link_type, self.search_number, self.proc_uid)
    }
}

/// The manual writes a link's target as "G123#part" or "G123:procsub2": the procedure, and the part of it
pub fn split_target(for_attr: &str) -> (String, String) {
    for marker in [":procsub", ":figure_", ":table_"] {
        if let Some(at) = for_attr.find(marker) {
            return (for_attr[..at].to_string(), for_attr[at + 1..].to_string());
        }
    }
    match for_attr.split_once('#') {
        Some((sn, h)) => (sn.to_string(), h.to_string()),
        None => (for_attr.to_string(), String::new()),
    }
}

/// Ford's page pictures (arrows, print buttons, logos…), not the procedure's own
const UI_PICTURES: &[&str] = &[
    "caretright",
    "caretdown",
    "bars-black",
    "minus-black",
    "print",
    "arrowback",
    "ppt-minus",
    "ppt-plus",
    "fordecat",
    "loading",
    "spigot",
];
/// Ford's training banners beside pictures, which don't work outside PTS
fn banner(e: &El) -> bool {
    let t = format!(
        "{} {} {} {} {}",
        e.raw_text(),
        e.attr("alt").unwrap_or(""),
        e.attr("title").unwrap_or(""),
        e.attr("href").unwrap_or(""),
        e.attr("src").unwrap_or("")
    )
    .to_ascii_lowercase();
    ["graphics training", "learn about symbols", "symbols, color coding"]
        .iter()
        .any(|w| t.contains(w))
}

const SKIP: &[&str] = &[
    "input", "select", "button", "textarea", "iframe", "frame", "object", "embed", "map", "area", "head", "title", "option",
];
const BLOCK: &[&str] = &[
    "p",
    "div",
    "center",
    "section",
    "article",
    "form",
    "blockquote",
    "body",
    "html",
    "dd",
    "dt",
    "dl",
    "fieldset",
    "main",
    "header",
    "footer",
    "nav",
    "pre",
    "address",
    "figure",
    "figcaption",
    "caption",
];

impl<'b> Builder<'b> {
    fn new(base: &'b str) -> Self {
        Builder {
            base,
            out: Vec::new(),
            cur: Vec::new(),
            bold: 0,
            italic: 0,
            link: None,
            proc: None,
            reveal_next: false,
            skip: Rc::default(),
            anchor: None,
            marks: Vec::new(),
        }
    }

    /// A builder for a part (a heading, a list item, a cell), reading the same way
    fn sub(&self) -> Builder<'b> {
        let mut b = Builder::new(self.base);
        b.proc = self.proc;
        b.skip = self.skip.clone();
        b
    }

    /// Where a link in a procedure goes: another procedure ("proc:…"), or somewhere else (an address); none for a
    /// jump within the same procedure (for now: shown as plain text)
    fn proc_link(&self, e: &El, p: &ProcCtx) -> Option<String> {
        let class = e.attr("class").unwrap_or("").to_ascii_lowercase();
        if let Some(f) = e.attr("data-for").filter(|f| !f.trim().is_empty()) {
            let (sn, hash) = split_target(f.trim());
            let uid = e.attr("data-procuid").unwrap_or("").to_string();
            let same = sn == p.search_number || (!uid.is_empty() && uid == p.proc_uid);
            if let Some(ppt) = e.attr("data-show-ppt") {
                if same {
                    // one of this procedure's own tests
                    return Some(format!("ppt:{ppt}"));
                }
                return Some({
                    ProcLink {
                        search_number: sn,
                        proc_uid: uid,
                        link_type: "Procedure".into(),
                        hash: String::new(),
                        ppt: ppt.to_string(),
                    }
                    .to_url()
                });
            }
            if ["expand-link", "procedure-link", "publication-content"]
                .iter()
                .any(|c| class.contains(c))
            {
                return Some(
                    ProcLink {
                        search_number: sn,
                        proc_uid: uid,
                        link_type: if class.contains("publication-content") {
                            "PublicationContent"
                        } else {
                            "Procedure"
                        }
                        .into(),
                        hash,
                        ppt: String::new(),
                    }
                    .to_url(),
                );
            }
        }
        let href = e.attr("href").unwrap_or("").trim();
        if href.to_ascii_lowercase().contains("/publication/proc?") {
            let url = resolve(self.base, href)?;
            let uid = query_value(&url, "procUid").unwrap_or_default();
            let sn = query_value(&url, "searchNumber").unwrap_or_default();
            let hash = url.split_once('#').map(|(_, h)| h.trim().to_string()).unwrap_or_default();
            if (!uid.is_empty() && uid == p.proc_uid) || sn.is_empty() {
                // a part of this procedure (a pinpoint test's step…)
                return (!hash.is_empty()).then(|| format!("jump:{hash}"));
            }
            return Some(
                ProcLink {
                    search_number: sn,
                    proc_uid: uid,
                    link_type: query_value(&url, "linkType").unwrap_or_else(|| "Procedure".into()),
                    hash: url.split_once('#').map(|(_, h)| h.to_string()).unwrap_or_default(),
                    ppt: String::new(),
                }
                .to_url(),
            );
        }
        if let Some(h) = href.strip_prefix('#').map(str::trim).filter(|h| !h.is_empty()) {
            return Some(format!("jump:{h}"));
        }
        link_target(href).and_then(|h| resolve(self.base, h))
    }

    /// The paragraph so far, as a block (when it has any words)
    fn flush(&mut self) {
        if !self.marks.is_empty() {
            self.out.push(Block::Anchor(std::mem::take(&mut self.marks)));
        }
        let mut inl = std::mem::take(&mut self.cur);
        // spaces at its start and end, and breaks there, go
        while matches!(inl.first(), Some(Inline::Break)) {
            inl.remove(0);
        }
        while matches!(inl.last(), Some(Inline::Break)) {
            inl.pop();
        }
        if let Some(Inline::Text { text, .. } | Inline::Link { text, .. }) = inl.first_mut() {
            *text = text.trim_start().to_string();
        }
        if let Some(Inline::Text { text, .. } | Inline::Link { text, .. }) = inl.last_mut() {
            *text = text.trim_end().to_string();
        }
        inl.retain(|i| !matches!(i, Inline::Text { text, .. } if text.is_empty()));
        let words: String = inl
            .iter()
            .map(|i| match i {
                Inline::Text { text, .. } | Inline::Link { text, .. } => text.as_str(),
                Inline::Break => " ",
            })
            .collect();
        let words = crate::html::clean(&words);
        if words.is_empty() {
            return;
        }
        // Ford's "*****" separator lines
        if words.len() >= 10 && words.chars().all(|c| c == '*') {
            self.out.push(Block::Rule);
            return;
        }
        let up = words.to_uppercase();
        let callout = [
            ("WARNING", Callout::Warning),
            ("CAUTION", Callout::Caution),
            ("NOTICE", Callout::Notice),
            ("NOTE", Callout::Note),
        ]
        .iter()
        .find(|(w, _)| up.starts_with(w) && !up[w.len()..].starts_with(|c: char| c.is_alphabetic()))
        .map(|(_, c)| *c);
        self.out.push(Block::Para(inl, callout));
    }

    fn text(&mut self, raw: &str) {
        // spaces of any kind as one (non-breaking ones too)
        let mut t = String::with_capacity(raw.len());
        let mut space = false;
        for c in raw.chars() {
            if c.is_whitespace() || c == '\u{a0}' {
                space = true;
            } else {
                if space {
                    t.push(' ');
                }
                space = false;
                t.push(c);
            }
        }
        if space {
            t.push(' ');
        }
        if t.is_empty() {
            return;
        }
        // no space right after a line break or at a paragraph's start
        if (self.cur.is_empty() || matches!(self.cur.last(), Some(Inline::Break))) && t.starts_with(' ') {
            t.remove(0);
            if t.is_empty() {
                return;
            }
        }
        let (bold, italic) = (self.bold > 0, self.italic > 0);
        match (&self.link, self.cur.last_mut()) {
            (Some(url), Some(Inline::Link { text, url: u })) if u == url => text.push_str(&t),
            (Some(url), _) => self.cur.push(Inline::Link { text: t, url: url.clone() }),
            (None, Some(Inline::Text { text, bold: b, italic: i })) if *b == bold && *i == italic => {
                // no double spaces where two pieces meet
                if text.ends_with(' ') && t.starts_with(' ') {
                    t.remove(0);
                }
                text.push_str(&t)
            }
            (None, _) => self.cur.push(Inline::Text { text: t, bold, italic }),
        }
    }

    fn kids(&mut self, e: El) {
        for p in e.pieces() {
            match p {
                Piece::Text(t) => self.text(t),
                Piece::El(c) => self.element(c),
            }
        }
    }

    fn element(&mut self, e: El) {
        if self.skip.borrow().contains(&e.key()) {
            return;
        }
        let tag = e.tag();
        let reveal = std::mem::take(&mut self.reveal_next) || (self.proc.is_some() && e.attr("data-content-type") == Some("intro"));
        if (e.hidden() && !reveal) || SKIP.contains(&tag) || matches!(tag, "meta" | "link") {
            return;
        }
        if self.proc.is_some() {
            // Ford's page furniture, and its training banners
            let id = e.attr("id").unwrap_or("");
            if matches!(id, "taginfo" | "rapDiv" | "copyright" | "goBackStepRow" | "lnkPptStepIntro")
                || e.has_class("show-vin")
                || (matches!(tag, "a" | "img") && banner(&e))
                || (tag == "table" && e.find(|x| x.tag() == "a" && x.has_class("bookinfosmall")).is_some())
            {
                return;
            }
            // "Click for details": the link goes, what it hid shows
            if tag == "a" && e.attr("data-pptlinktype") == Some("clickfordetails") {
                self.reveal_next = true;
                return;
            }
            // a pinpoint test's title: the test, read whole
            if tag == "span" && is_pinpoint_title(&e) {
                self.pinpoint(e);
                return;
            }
            if tag == "a" {
                if let Some(n) = e.attr("name").map(str::trim).filter(|n| !n.is_empty()) {
                    self.anchor = Some(n.to_string());
                    if !self.marks.iter().any(|m| m == n) {
                        self.marks.push(n.to_string());
                    }
                }
            }
        }
        match tag {
            "br" => self.cur.push(Inline::Break),
            "hr" => {
                self.flush();
                self.out.push(Block::Rule);
            }
            "b" | "strong" => {
                self.bold += 1;
                self.kids(e);
                self.bold -= 1;
            }
            "i" | "em" | "cite" => {
                self.italic += 1;
                self.kids(e);
                self.italic -= 1;
            }
            "a" => {
                let target = match self.proc {
                    Some(p) => self.proc_link(&e, p),
                    None => e.attr("href").and_then(link_target).and_then(|h| resolve(self.base, h)),
                };
                let before = self.link.clone();
                if target.is_some() && self.link.is_none() {
                    self.link = target;
                }
                self.kids(e);
                self.link = before;
            }
            "img" => {
                let src = e.attr("src").unwrap_or("").to_ascii_lowercase();
                if layout_picture(&e) || (self.proc.is_some() && UI_PICTURES.iter().any(|w| src.contains(w))) {
                    return;
                }
                let Some(url) = e.attr("src").and_then(|s| resolve(self.base, s)) else {
                    return;
                };
                self.flush();
                self.out.push(Block::Image {
                    url,
                    alt: e.attr("alt").unwrap_or("").trim().to_string(),
                });
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush();
                let level = tag[1..].parse().unwrap_or(3);
                let mut sub = self.sub();
                sub.kids(e);
                sub.flush();
                // a heading holding only words: a heading; holding more (pictures…): its blocks as they are
                // (an anchor in it goes before it)
                let (marks, mut blocks): (Vec<Block>, Vec<Block>) = sub.out.into_iter().partition(|b| matches!(b, Block::Anchor(_)));
                for m in marks {
                    match (self.out.last_mut(), m) {
                        (Some(Block::Anchor(v)), Block::Anchor(n)) => v.extend(n),
                        (_, m) => self.out.push(m),
                    }
                }
                if let [Block::Para(inl, _)] = blocks.as_slice() {
                    self.out.push(Block::Heading(level, inl.clone()));
                } else {
                    self.out.append(&mut blocks);
                }
            }
            "ul" | "ol" | "menu" => {
                self.flush();
                let items: Vec<Vec<Block>> = e
                    .children()
                    .map(|li| {
                        let mut sub = self.sub();
                        if li.tag() == "li" {
                            sub.kids(li)
                        } else {
                            sub.element(li)
                        }
                        sub.flush();
                        sub.out
                    })
                    .filter(|b| !b.is_empty())
                    .collect();
                if !items.is_empty() {
                    self.out.push(Block::List {
                        ordered: tag == "ol",
                        items,
                    });
                }
            }
            "table" => {
                self.flush();
                self.table(e);
            }
            t if BLOCK.contains(&t) || t == "li" || t == "tr" || t == "td" || t == "tbody" => {
                self.flush();
                self.kids(e);
                self.flush();
            }
            _ => self.kids(e), // inline (span, font, label, sup…): its content
        }
    }

    /// A table: its own rows (not those of tables inside it); a table of one column is just its cells' content
    fn table(&mut self, t: El) {
        if self.proc.is_some() {
            if let Some(p) = self.caption_pinpoint(&t) {
                self.out.push(Block::Pinpoint(Box::new(p)));
                return;
            }
        }
        let mut rows: Vec<Vec<Cell>> = Vec::new();
        let mut row_els = Vec::new();
        for c in t.children() {
            match c.tag() {
                "tr" => row_els.push(c),
                "thead" | "tbody" | "tfoot" => row_els.extend(c.children().filter(|r| r.tag() == "tr")),
                "caption" => {
                    let mut sub = self.sub();
                    sub.bold += 1;
                    sub.kids(c);
                    sub.flush();
                    self.out.append(&mut sub.out);
                }
                _ => {}
            }
        }
        for r in row_els {
            if r.hidden() || self.skip.borrow().contains(&r.key()) {
                continue;
            }
            let cells: Vec<Cell> = r
                .children()
                .filter(|c| matches!(c.tag(), "td" | "th") && !c.hidden() && !self.skip.borrow().contains(&c.key()))
                .map(|c| {
                    let mut sub = self.sub();
                    if c.tag() == "th" {
                        sub.bold += 1;
                    }
                    sub.kids(c);
                    sub.flush();
                    Cell {
                        blocks: sub.out,
                        header: c.tag() == "th",
                        span: c.attr("colspan").and_then(|s| s.trim().parse().ok()).unwrap_or(1).clamp(1, 12),
                    }
                })
                .collect();
            if cells.iter().any(|c| has_content(&c.blocks)) {
                rows.push(cells);
            } else {
                // a row of anchors only: they go before the table
                self.out.extend(cells.into_iter().flat_map(|c| c.blocks));
            }
        }
        if rows.is_empty() {
            return;
        }
        let grid =
            t.attr("border").and_then(|b| b.trim().parse::<u32>().ok()).is_some_and(|b| b > 0) || rows.iter().flatten().any(|c| c.header);
        let wide = rows
            .iter()
            .map(|r| r.iter().filter(|c| has_content(&c.blocks)).count())
            .max()
            .unwrap_or(0);
        if wide <= 1 {
            // Ford's page layout: one column, so just the content
            for c in rows.into_iter().flatten() {
                self.out.extend(c.blocks);
            }
            return;
        }
        self.out.push(Block::Table { grid, rows });
    }

    /// A pinpoint test from its title (`<span class="isipppt">`), and its body (`<div id="PPT…">`) wherever it is
    fn pinpoint(&mut self, s: El) {
        self.flush();
        let title = s
            .find(|x| matches!(x.attr("id"), Some("spappttitle" | "ppttitle")))
            .map(|t| t.text())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| s.text());
        let name = s.attr("id").unwrap_or("").trim().trim_start_matches("Span").to_string();
        let mut ppt = Pinpoint {
            name: name.clone(),
            title,
            anchor: self.anchor.take().unwrap_or_default(),
            spa: s.attr("data-spapptfile").unwrap_or("").trim().to_string(),
            ..Default::default()
        };
        if name.is_empty() && ppt.spa.is_empty() {
            // a title only: as a heading
            self.out.push(Block::Heading(
                3,
                vec![Inline::Text {
                    text: ppt.title,
                    bold: true,
                    italic: false,
                }],
            ));
            return;
        }
        let mut top = s;
        while let Some(p) = top.parent() {
            top = p;
        }
        let want = format!("PPT{name}");
        if let Some(body) = top.find(|x| x.attr("id") == Some(want.as_str())) {
            self.skip.borrow_mut().insert(body.key());
            self.ppt_body(body, &mut ppt);
        }
        self.out.push(Block::Pinpoint(Box::new(ppt)));
    }

    /// A test's body: its introduction, and its steps (rows marked as steps of this test)
    fn ppt_body(&self, body: El, ppt: &mut Pinpoint) {
        if let Some(w) = body.find(|x| x.tag() == "tr" && x.attr("data-content-type") == Some("wrapper")) {
            let mut sub = self.sub();
            sub.kids(w);
            sub.flush();
            ppt.intro = sub.out;
        }
        let steps = |any: bool| {
            body.find_all(|x| {
                x.tag() == "tr"
                    && x.attr("data-content-type") == Some("step")
                    && (any || x.attr("data-pptid").map_or(true, |i| i.trim().is_empty() || i == ppt.name))
            })
        };
        let mut rows = steps(false);
        if rows.is_empty() && !ppt.spa.is_empty() {
            rows = steps(true); // fetched steps can name the test their own way
        }
        if rows.is_empty() && ppt.spa.is_empty() && ppt.intro.is_empty() {
            // no steps ("Diagnostic steps are not provided for this symptom"): what it says, as it is
            let mut sub = self.sub();
            sub.kids(body);
            sub.flush();
            ppt.intro = sub.out;
            return;
        }
        for (n, tr) in rows.into_iter().enumerate() {
            let head = tr.find(|x| {
                x.tag() == "td"
                    && x.has_class("gray")
                    && x.find(|y| y.attr("data-content-type") == Some("stepresult")).is_none()
                    && !x.text().is_empty()
            });
            let id = tr
                .attr("data-pptstepid")
                .map(str::to_string)
                .or_else(|| {
                    head.and_then(|h| h.find(|a| a.tag() == "a" && a.attr("name").is_some()))
                        .and_then(|a| a.attr("name"))
                        .map(str::to_string)
                })
                .unwrap_or_default();
            let head_text = head.map(|h| h.text()).unwrap_or_default();
            let label = tr
                .attr("data-ppttypeandstep")
                .and_then(step_label)
                .or_else(|| head_label(&head_text))
                .unwrap_or_else(|| format!("{}{}", ppt.name, n + 1));
            let step = self.read_step(&[tr], head, id, label, &head_text);
            ppt.steps.push(step);
        }
    }

    /// One step from its parts (the elements it's made of), its title cell (shown apart)
    fn read_step(&self, parts: &[El], head: Option<El>, id: String, label: String, head_text: &str) -> Step {
        let title = head_text.strip_prefix(label.as_str()).unwrap_or(head_text).trim().to_string();
        let mut skip = Vec::new();
        if let Some(h) = head {
            skip.push(h.key());
        }
        let mut question = String::new();
        let mut answers = Vec::new();
        for part in parts {
            if let Some(q) = part.find(|x| x.attr("data-content-type") == Some("stepquestion")) {
                if question.is_empty() {
                    question = q.text();
                }
                skip.push(q.key());
            }
            // the answers: Ford marks them (data-content-type="stepresult"); older tests are just rows "Yes | GO to A2"
            let mut marked: Vec<(El, El)> = part
                .find_all(|x| x.attr("data-content-type") == Some("stepresult"))
                .into_iter()
                .filter_map(|b| Some((b, b.closest(|x| x.tag() == "tr")?)))
                .collect();
            if marked.is_empty() {
                let mut rows = part.find_all(|x| x.tag() == "tr");
                if part.tag() == "tr" {
                    rows.insert(0, *part);
                }
                marked = rows
                    .into_iter()
                    .filter_map(|r| {
                        let cells: Vec<El> = r.children().filter(|c| matches!(c.tag(), "td" | "th")).collect();
                        let first = cells.first()?;
                        let t = first.text().to_ascii_lowercase();
                        (cells.len() == 2 && matches!(t.as_str(), "yes" | "no")).then_some((*first, r))
                    })
                    .collect();
            }
            for (b, row) in marked {
                let mut sub = self.sub();
                for c in row.children().filter(|c| matches!(c.tag(), "td" | "th")) {
                    if c == b || b.is_inside(&c) {
                        continue;
                    }
                    sub.kids(c);
                    sub.flush();
                }
                let mut jumps: Vec<String> = Vec::new();
                for a in row.all("a") {
                    if let Some(j) = self.jump_of(&a) {
                        if !jumps.contains(&j) {
                            jumps.push(j);
                        }
                    }
                }
                let label = b.text();
                answers.push(Answer {
                    id: b
                        .attr("id")
                        .map(str::trim)
                        .filter(|i| !i.is_empty())
                        .map(str::to_string)
                        .unwrap_or_else(|| label.clone()),
                    label,
                    blocks: sub.out,
                    jumps,
                });
                // the answer's own table (or row) isn't the step's content
                let holder = row.closest(|x| x.tag() == "table").filter(|t| t.is_inside(part)).unwrap_or(row);
                skip.push(holder.key());
            }
        }
        self.skip.borrow_mut().extend(skip);
        let mut sub = self.sub();
        for part in parts {
            // (a part that was all answer: shown with the answers)
            if !self.skip.borrow().contains(&part.key()) {
                sub.kids(*part);
                sub.flush();
            }
        }
        Step {
            id,
            label,
            title,
            question,
            blocks: sub.out,
            answers,
        }
    }

    /// Where a link in an answer goes in this procedure ("GO to A2": that step's id), if it does
    fn jump_of(&self, a: &El) -> Option<String> {
        if a.attr("data-pptlinktype") == Some("clickfordetails") {
            return None;
        }
        let href = a.attr("href").unwrap_or("").trim();
        let hash = href.rsplit_once('#').map(|(_, h)| h.trim()).filter(|h| !h.is_empty())?.to_string();
        if href.starts_with('#') {
            return Some(hash);
        }
        let p = self.proc?;
        let url = resolve(self.base, href)?;
        let uid = query_value(&url, "procUid").unwrap_or_default();
        let sn = query_value(&url, "searchNumber").unwrap_or_default();
        let here = a.attr("data-hash-type") == Some("pptgotostep")
            || (!uid.is_empty() && uid == p.proc_uid)
            || (!sn.is_empty() && sn == p.search_number);
        here.then_some(hash)
    }

    /// Ford's older layout: a test as one table, "PINPOINT TEST A: …" its caption, each step a title row ("A1
    /// CHECK …", with its anchor) and the rows after it
    fn caption_pinpoint(&mut self, t: &El) -> Option<Pinpoint> {
        let caption = t.children().find(|c| c.tag() == "caption")?;
        let title = caption.text();
        let name = caption_test_name(&title)?;
        let mut rows: Vec<El> = Vec::new();
        for c in t.children() {
            match c.tag() {
                "tr" => rows.push(c),
                "thead" | "tbody" | "tfoot" => rows.extend(c.children().filter(|r| r.tag() == "tr")),
                _ => {}
            }
        }
        fn head_of<'a>(tr: &El<'a>, name: &str) -> Option<(El<'a>, String, String)> {
            let cells: Vec<El<'a>> = tr.children().collect();
            let [td] = cells.as_slice() else { return None };
            if !td.has_class("gray") {
                return None;
            }
            let a = td.children().find(|x| x.tag() == "a" && x.attr("name").is_some())?;
            let b = td.children().find(|x| x.tag() == "b")?;
            let label = head_label(&b.text()).filter(|l| l.to_ascii_uppercase().starts_with(name))?;
            Some((*td, a.attr("name").unwrap_or("").to_string(), label))
        }
        if !rows.iter().any(|r| head_of(r, &name).is_some()) {
            return None;
        }
        self.flush();
        let mut ppt = Pinpoint {
            name: name.clone(),
            title,
            anchor: self.anchor.take().unwrap_or_default(),
            legacy: true,
            ..Default::default()
        };
        let mut intro: Vec<El> = Vec::new();
        let mut groups: Vec<((El, String, String), Vec<El>)> = Vec::new();
        for r in rows {
            if let Some(h) = head_of(&r, &name) {
                groups.push((h, Vec::new()));
            } else if let Some((_, parts)) = groups.last_mut() {
                parts.push(r);
            } else {
                intro.push(r);
            }
        }
        let mut sub = self.sub();
        for r in intro {
            sub.kids(r);
            sub.flush();
        }
        ppt.intro = sub.out;
        for ((td, id, label), parts) in groups {
            let head_text = td.text();
            ppt.steps.push(self.read_step(&parts, None, id, label, &head_text));
        }
        Some(ppt)
    }
}

/// Something to show (not only anchors)
fn has_content(bs: &[Block]) -> bool {
    bs.iter().any(|b| !matches!(b, Block::Anchor(_)))
}

/// A pinpoint test's title on Ford's page: `<span class="isipppt">` (or `spappt`, or an older `SpanA` that opens one)
fn is_pinpoint_title(e: &El) -> bool {
    e.has_class("isipppt")
        || e.has_class("spappt")
        || (e.attr("id").is_some_and(|i| i.starts_with("Span")) && e.attr("onclick").is_some_and(|o| o.contains("Pinpoint")))
}

/// "ASTEP1" → "A1"
fn step_label(t: &str) -> Option<String> {
    let up = t.to_ascii_uppercase();
    let at = up.find("STEP")?;
    let (letters, rest) = (&up[..at], &up[at + 4..]);
    let num: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let ok = !letters.is_empty()
        && letters.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && letters.chars().all(|c| c.is_ascii_alphanumeric());
    (ok && !num.is_empty()).then(|| format!("{letters}{num}"))
}

/// "A1 CHECK FOR THE SOURCE…" → "A1"
fn head_label(t: &str) -> Option<String> {
    let w = t.split_whitespace().next()?;
    let letters = w.chars().take_while(char::is_ascii_alphabetic).count();
    let digits = w[letters..].chars().take_while(char::is_ascii_digit).count();
    (letters > 0 && letters <= 3 && digits > 0 && letters + digits == w.len()).then(|| w.to_string())
}

/// "PINPOINT TEST A: …" → "A"
fn caption_test_name(title: &str) -> Option<String> {
    let up = title.to_ascii_uppercase();
    let rest = up.trim_start().strip_prefix("PINPOINT TEST")?.trim_start();
    let (name, _) = rest.split_once(':')?;
    let name = name.trim();
    let letters = name.chars().take_while(char::is_ascii_alphabetic).count();
    let ok = (1..=3).contains(&letters) && name[letters..].len() <= 3 && name[letters..].chars().all(|c| c.is_ascii_digit());
    ok.then(|| name.to_string())
}

/// A test's steps fetched apart (newer tests: Ford's rows of steps): its introduction (if any) and steps
pub fn read_spa_steps(html: &str, base: &str, p: &ProcCtx, name: &str) -> (Vec<Block>, Vec<Step>) {
    let d = Doc::parse(&format!("<div id=\"spa\"><table><tbody>{html}</tbody></table></div>"));
    let root = d.by_id("spa").unwrap_or(d.root());
    let mut b = Builder::new(base);
    b.proc = Some(p);
    let mut ppt = Pinpoint {
        name: name.to_string(),
        spa: "fetched".into(),
        ..Default::default()
    };
    b.ppt_body(root, &mut ppt);
    if ppt.steps.is_empty() && ppt.intro.is_empty() {
        // not rows of steps: whatever came back, as it is
        b.kids(root);
        b.flush();
        ppt.intro = b.out;
    }
    (ppt.intro, ppt.steps)
}

/// Which of these tests an id names: its letter ("A"), Ford's id for it or one of its steps ("G2198756p1",
/// "G2198756t3"), or the ways PC/ED writes them ("2015_GASOLINE_s5p001_DH", "PPT_DH") — the test's letter
pub fn find_test(blocks: &[Block], id: &str) -> Option<String> {
    let tests = Block::pinpoints(blocks);
    let want = id.trim();
    let want = want.strip_prefix("PPT").or_else(|| want.strip_prefix("ppt")).unwrap_or(want);
    if want.is_empty() {
        return None;
    }
    let lc = want.to_ascii_lowercase();
    for p in &tests {
        if p.name == want || p.anchor == want || p.spa == want || p.steps.iter().any(|s| s.id == want) {
            return Some(p.name.clone());
        }
    }
    if let Some(p) = tests.iter().find(|p| p.name.to_ascii_lowercase() == lc) {
        return Some(p.name.clone());
    }
    let tail = lc
        .split(['_', ':', '#', '|'])
        .filter(|x| !x.is_empty())
        .last()
        .unwrap_or(&lc)
        .to_string();
    for p in &tests {
        let ids: Vec<String> = [&p.name, &p.spa, &p.anchor]
            .iter()
            .filter(|x| !x.is_empty())
            .map(|x| x.to_ascii_lowercase())
            .collect();
        if ids
            .iter()
            .any(|x| *x == tail || lc.ends_with(&format!("_{x}")) || x.ends_with(&format!("_{lc}")) || x.ends_with(&format!("_{tail}")))
        {
            return Some(p.name.clone());
        }
    }
    tests
        .iter()
        .find(|p| caption_test_name(&p.title).is_some_and(|n| n.to_ascii_lowercase() == lc || n.to_ascii_lowercase() == tail))
        .map(|p| p.name.clone())
}

/// A Workshop Manual procedure (Ford's content, at `base`) as a document: Ford's page furniture and banners left out,
/// links to other procedures as "proc:" addresses ([`ProcLink`])
pub fn read_procedure(root: El, base: &str, p: &ProcCtx) -> Vec<Block> {
    let mut b = Builder::new(base);
    b.proc = Some(p);
    b.kids(root);
    b.flush();
    b.out
}

/// A Ford page (`html`, at the address `base`) as a document. The page's `<body>` when it has one.
pub fn read(html: &str, base: &str) -> Vec<Block> {
    let doc = Doc::parse(html);
    let root = doc.root().first("body").unwrap_or(doc.root());
    let mut b = Builder::new(base);
    b.kids(root);
    b.flush();
    b.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Inline {
        Inline::Text {
            text: s.into(),
            bold: false,
            italic: false,
        }
    }

    #[test]
    fn resolves_addresses() {
        let base = "https://www.x.com/vdirsnet/TSB_SSM/DisplayArticles.aspx?vin=1";
        assert_eq!(
            resolve(base, "DisplayArticle.aspx?id=1").unwrap(),
            "https://www.x.com/vdirsnet/TSB_SSM/DisplayArticle.aspx?id=1"
        );
        assert_eq!(resolve(base, "/a/b.gif").unwrap(), "https://www.x.com/a/b.gif");
        assert_eq!(resolve(base, "../img/c.png").unwrap(), "https://www.x.com/vdirsnet/img/c.png");
        assert_eq!(resolve(base, "https://y.com/z").unwrap(), "https://y.com/z");
        assert_eq!(resolve(base, "//cdn.y.com/z").unwrap(), "https://cdn.y.com/z");
        assert_eq!(resolve(base, "#top"), None);
        assert_eq!(
            query_value("https://x/a.aspx?type=GSB&id=26%2D2414&t=Engine+noise", "id").unwrap(),
            "26-2414"
        );
        assert_eq!(
            query_value("https://x/a.aspx?type=GSB&t=Engine+noise", "t").unwrap(),
            "Engine noise"
        );
        assert_eq!(link_target("javascript:window.open('/a.pdf','x')"), Some("/a.pdf"));
    }

    #[test]
    fn reads_an_article() {
        let html = r#"<html><head><title>TSB</title><style>p{}</style></head><body>
          <table><tr><td><img src="spacer.gif" width=1><h2>Engine   noise</h2></td></tr></table>
          <p>Some <b>2021-2023</b> F-150 vehicles may<br>exhibit a noise. See <a href="DisplayArticle.aspx?id=2">TSB 23-1</a>.</p>
          <p>**************</p>
          <div>NOTE: Use the <i>correct</i> tool.</div>
          <ul><li>One</li><li>Two <img src="/pics/two.jpg" alt="Two"></li></ul>
          <table border="1"><tr><th>Part</th><th>Qty</th></tr><tr><td>Bolt</td><td>2</td></tr></table>
          <div style="display:none">hidden</div><input type=hidden value=x>
          <p>&nbsp;</p>
        </body></html>"#;
        let b = read(html, "https://ford.x/vdirsnet/TSB_SSM/DisplayArticle.aspx?id=1");
        assert_eq!(b[0], Block::Heading(2, vec![t("Engine noise")]));
        let Block::Para(inl, None) = &b[1] else { panic!("{:?}", b[1]) };
        assert_eq!(inl[0], t("Some "));
        assert_eq!(
            inl[1],
            Inline::Text {
                text: "2021-2023".into(),
                bold: true,
                italic: false
            }
        );
        assert_eq!(inl[2], t(" F-150 vehicles may"));
        assert_eq!(inl[3], Inline::Break);
        assert_eq!(
            inl[5],
            Inline::Link {
                text: "TSB 23-1".into(),
                url: "https://ford.x/vdirsnet/TSB_SSM/DisplayArticle.aspx?id=2".into()
            }
        );
        assert_eq!(b[2], Block::Rule);
        assert!(matches!(&b[3], Block::Para(_, Some(Callout::Note))));
        let Block::List { ordered: false, items } = &b[4] else {
            panic!("{:?}", b[4])
        };
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[1][1],
            Block::Image {
                url: "https://ford.x/pics/two.jpg".into(),
                alt: "Two".into()
            }
        );
        let Block::Table { grid: true, rows } = &b[5] else {
            panic!("{:?}", b[5])
        };
        assert!(rows[0][0].header && !rows[1][0].header);
        assert_eq!(b.len(), 6);
        assert_eq!(Block::images(&b), ["https://ford.x/pics/two.jpg"]);
        assert!(Block::text(&b).starts_with("Engine noise Some 2021-2023 F-150"));
    }

    const PROC: &str =
        "https://www.fordtechservice.dealerconnection.com/publication/proc?linkType=Procedure&searchNumber=G2198755&procUid=G2198756";

    fn proc_ctx() -> ProcCtx {
        ProcCtx {
            search_number: "G2198755".into(),
            proc_uid: "G2198756".into(),
        }
    }

    #[test]
    fn reads_a_pinpoint_test() {
        // trimmed from a real procedure (2023 Bronco Sport, 303-05B)
        let html = format!(
            r##"<div id="contentDiv"><p>Pinpoint Tests</p><a name="G2198756p1" data-steps-display="single"></a>
            <span id="SpanA" class="isipppt" onclick="javascript:PinpointToggleSingleStep(this, 'A')"><img src="./CaretRight.jpg">
              <span><b><span id="spappttitle">PINPOINT TEST A: ACCESSORY DRIVE BELT NOISE</span><img id="linkShowAllSteps" src="./bars-black.png"></b></span></span><br>
            <div id="PPTA"><table border="1"><tbody>
              <tr data-content-type="wrapper"><td><span id="lnkPptStepIntro"><img src="./ppt-minus.png"><b>Introduction</b></span>
                <div data-content-type="intro"><p><b>Possible Causes</b></p><ul><li>Damaged belt</li></ul></div></td></tr>
              <tr data-content-type="step" data-pptid="A" data-pptstepid="G2198756t1" data-ppttypeandstep="ASTEP1" style="display:'';"><td><table><tbody>
                <tr><td class="gray"><a name="G2198756t1"></a><b>A1 CHECK FOR THE SOURCE OF THE NOISE.</b></td></tr>
                <tr><td><ul class="nospace"><li>Use a stethoscope to determine the source of the noise.</li></ul><br>
                  <b data-content-type="stepquestion">Can the source of the noise be identified?</b><br><br>
                  <center><table border="1"><tbody><tr><td class="gray"><b data-content-type="stepresult" id="Yes">Yes</b></td>
                    <td><a data-pptlinktype="clickfordetails" href="{PROC}&goToHash=#">Click for details</a><span style="display:none;"> INSTALL a new component as necessary. </span></td></tr></tbody></table></center>
                  <center><table border="1"><tbody><tr><td class="gray"><b data-content-type="stepresult" id="No">No</b></td>
                    <td>GO to <a href="{PROC}&goToHash=#G2198756t2" data-hash-type="pptgotostep">A2</a></td></tr></tbody></table></center></td></tr>
                <tr id="goBackStepRow" style="display: none;"><td><img src="./arrowback.png"></td></tr>
              </tbody></table></td></tr>
              <tr data-content-type="step" data-pptid="A" data-pptstepid="G2198756t2" data-ppttypeandstep="ASTEP2" style="display:none;"><td><table><tbody>
                <tr><td class="gray"><a name="G2198756t2"></a><b>A2 CHECK THE BELT FOR CRACKING</b></td></tr>
                <tr><td><p>Look at the belt.</p><b data-content-type="stepquestion">Is the belt OK?</b>
                  <table border="1"><tbody><tr><td class="gray"><b data-content-type="stepresult" id="Yes">Yes</b></td>
                    <td>If built before May, GO to <a href="#G2198756t3">A3</a>. After, GO to <a href="#G2198756t4">A4</a></td></tr></tbody></table>
                  <table border="1"><tbody><tr><td class="gray"><b data-content-type="stepresult" id="No">No</b></td>
                    <td>INSTALL a new belt. REFER to: <a data-for="G1000" data-procuid="G1001" class="procedure-link">Belt</a></td></tr></tbody></table></td></tr>
              </tbody></table></td></tr>
            </tbody></table></div>
            <p>A DTC chart: GO to <a data-for="G2198755" data-procuid="G2198756" data-show-ppt="A">Pinpoint Test A</a></p></div>"##
        );
        let d = Doc::parse(&html);
        let bs = read_procedure(d.by_id("contentDiv").unwrap(), PROC, &proc_ctx());
        let tests = Block::pinpoints(&bs);
        assert_eq!(tests.len(), 1);
        let t = tests[0];
        assert_eq!(
            (t.name.as_str(), t.title.as_str(), t.anchor.as_str(), t.legacy),
            ("A", "PINPOINT TEST A: ACCESSORY DRIVE BELT NOISE", "G2198756p1", false)
        );
        assert!(Block::text(&t.intro).contains("Damaged belt"));
        assert!(!Block::text(&t.intro).contains("Introduction"));
        assert_eq!(t.steps.len(), 2);

        let s1 = &t.steps[0];
        assert_eq!(
            (s1.id.as_str(), s1.label.as_str(), s1.title.as_str()),
            ("G2198756t1", "A1", "CHECK FOR THE SOURCE OF THE NOISE.")
        );
        assert_eq!(s1.question, "Can the source of the noise be identified?");
        let words = Block::text(&s1.blocks);
        assert!(words.contains("stethoscope"), "{words}");
        // its title, question and answers are shown apart, not in its content
        for w in ["CHECK FOR", "identified?", "INSTALL", "GO to"] {
            assert!(!words.contains(w), "{w} in {words}");
        }
        assert_eq!(s1.answers.len(), 2);
        assert_eq!((s1.answers[0].id.as_str(), s1.answers[0].ends()), ("Yes", true));
        assert_eq!(Block::text(&s1.answers[0].blocks), "INSTALL a new component as necessary.");
        assert_eq!(s1.answers[1].jumps, vec!["G2198756t2".to_string()]);
        // "GO to A2": a link within the procedure
        assert!(
            matches!(&s1.answers[1].blocks[..], [Block::Para(i, None)] if i.iter().any(|x| matches!(x, Inline::Link { url, .. } if url == "jump:G2198756t2")))
        );

        let s2 = &t.steps[1];
        assert_eq!(s2.label, "A2");
        assert_eq!(s2.answers[0].jumps, vec!["G2198756t3".to_string(), "G2198756t4".to_string()]);
        assert!(s2.answers[1].ends());
        assert!(
            matches!(&s2.answers[1].blocks[..], [Block::Para(i, _)] if i.iter().any(|x| matches!(x, Inline::Link { url, .. } if url.starts_with("proc:"))))
        );
        assert_eq!(Block::text(&s2.blocks), "Look at the belt.");

        // the test's body isn't read twice; the DTC chart's link opens the test
        assert_eq!(Block::text(&bs).matches("Look at the belt").count(), 1);
        assert!(
            bs.iter()
                .any(|b| matches!(b, Block::Para(i, _) if i.iter().any(|x| matches!(x, Inline::Link { url, .. } if url == "ppt:A"))))
        );

        // finding it, however it's named
        for id in ["A", "a", "PPTA", "G2198756p1", "G2198756t2", "2015_GASOLINE_s5p001_A"] {
            assert_eq!(find_test(&bs, id).as_deref(), Some("A"), "{id}");
        }
        assert_eq!(find_test(&bs, "B"), None);
    }

    #[test]
    fn reads_an_older_pinpoint_table() {
        let html = r##"<div id="c"><a name="X1"></a><table><caption>PINPOINT TEST B: NO START</caption>
            <tr><td>Check the fuse first.</td></tr>
            <tr><td class="gray"><a name="Q1"></a><b>B1 CHECK THE BATTERY</b></td></tr>
            <tr><td>Measure the voltage. Is it over 12 V?</td></tr>
            <tr><td><table><tr><td class="gray">Yes</td><td>GO to <a href="#Q2">B2</a></td></tr>
                            <tr><td class="gray">No</td><td>CHARGE the battery.</td></tr></table></td></tr>
            <tr><td class="gray"><a name="Q2"></a><b>B2 CHECK THE STARTER</b></td></tr>
            <tr><td>Listen for a click.</td></tr>
        </table></div>"##;
        let d = Doc::parse(html);
        let bs = read_procedure(d.by_id("c").unwrap(), PROC, &proc_ctx());
        let t = Block::pinpoints(&bs)[0];
        assert_eq!((t.name.as_str(), t.legacy, t.anchor.as_str()), ("B", true, "X1"));
        assert_eq!(Block::text(&t.intro), "Check the fuse first.");
        assert_eq!(t.steps.len(), 2);
        assert_eq!(
            (t.steps[0].id.as_str(), t.steps[0].label.as_str(), t.steps[0].title.as_str()),
            ("Q1", "B1", "CHECK THE BATTERY")
        );
        assert_eq!(Block::text(&t.steps[0].blocks), "Measure the voltage. Is it over 12 V?");
        let a = &t.steps[0].answers;
        assert_eq!(a.len(), 2);
        assert_eq!((a[0].id.as_str(), a[0].jumps.clone()), ("Yes", vec!["Q2".to_string()]));
        assert!(a[1].ends());
        assert_eq!(t.steps[1].answers.len(), 0);
    }

    #[test]
    fn reads_fetched_steps() {
        let html = r##"<tr data-content-type="step" data-pptid="G9p1" data-pptstepid="s1" data-ppttypeandstep="DHSTEP1"><td>
            <table><tr><td class="gray"><a name="s1"></a><b>DH1 CHECK IT</b></td></tr><tr><td>Do it.
            <table><tr><td class="gray"><b data-content-type="stepresult" id="Yes">Yes</b></td><td>Done.</td></tr></table></td></tr></table></td></tr>"##;
        let (intro, steps) = read_spa_steps(html, PROC, &proc_ctx(), "DH");
        assert!(intro.is_empty());
        assert_eq!(steps.len(), 1);
        assert_eq!((steps[0].label.as_str(), steps[0].title.as_str()), ("DH1", "CHECK IT"));
        assert_eq!(Block::text(&steps[0].blocks), "Do it.");
        assert_eq!(steps[0].answers[0].id, "Yes");
    }

    #[test]
    fn step_labels() {
        assert_eq!(step_label("ASTEP1").as_deref(), Some("A1"));
        assert_eq!(step_label("DHSTEP12").as_deref(), Some("DH12"));
        assert_eq!(step_label("STEP1"), None);
        assert_eq!(head_label("A10 CHECK").as_deref(), Some("A10"));
        assert_eq!(head_label("CHECK THE"), None);
        assert_eq!(caption_test_name("PINPOINT TEST AB: X").as_deref(), Some("AB"));
        assert_eq!(caption_test_name("PINPOINT TEST: X"), None);
    }

    #[test]
    fn keeps_where_links_go() {
        let html = r##"<div id="c"><a name="procsub1"></a><h2><a name="h2a"></a>Removal</h2><p>Text.</p>
            <a name="procsub2"></a><table border="1"><tr><td><a name="cellmark"></a>In a cell</td><td>Two</td></tr>
            <tr><td><a name="rowonly"></a></td></tr></table><p>See <a href="#procsub1">above</a>.</p></div>"##;
        let d = Doc::parse(html);
        let bs = read_procedure(d.by_id("c").unwrap(), PROC, &proc_ctx());
        let a = |b: &Block| match b {
            Block::Anchor(n) => n.join(","),
            _ => String::new(),
        };
        assert_eq!(a(&bs[0]), "procsub1,h2a");
        assert!(matches!(&bs[1], Block::Heading(2, _)));
        assert!(matches!(&bs[2], Block::Para(..)));
        assert_eq!(a(&bs[3]), "procsub2");
        assert_eq!(a(&bs[4]), "rowonly");
        let Block::Table { rows, .. } = &bs[5] else { panic!("{:?}", bs[5]) };
        assert_eq!(rows.len(), 1);
        assert_eq!(a(&rows[0][0].blocks[0]), "cellmark");
        assert!(matches!(&bs[6], Block::Para(i, _) if i.iter().any(|x| matches!(x, Inline::Link { url, .. } if url == "jump:procsub1"))));
    }
}
