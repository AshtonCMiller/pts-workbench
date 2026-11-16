//! A field service action's page (opened from OASIS) and its documents: Ford's FSA page lists them (the dealer
//! bulletin, technical instructions, owner letters…), each a PDF on Ford's document server (fordservicecontent.com),
//! which wants its own sign-on cookies ([`Ford::content_sign_on`]).
//! (The Electron app's `workbench/web-views.js`: its Ford PDFs and content sign-on.)

use std::time::{Duration, Instant};

use crate::doc::{self, Block, resolve};
use crate::html::Doc;
use crate::{Error, Ford, Result};
use workbench_net::{Http, Method};

/// One of an FSA's documents
#[derive(Debug, Clone, PartialEq)]
pub struct FsaDoc {
    /// "Dealer bulletin", "Technical instructions"…
    pub name: String,
    pub url: String,
}

/// An FSA's page, read
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FsaPage {
    /// Its documents (PDFs), in the page's order
    pub docs: Vec<FsaDoc>,
    /// The page's own words (when it has no documents, or for what it says around them)
    pub blocks: Vec<Block>,
}

/// What Ford's file names mean ("24S10_BULL.pdf" → "Dealer bulletin")
const NAMES: &[(&str, &str)] = &[
    ("_TECHX", "Technical instructions"),
    ("_TECH", "Technical instructions"),
    ("_BULL", "Dealer bulletin"),
    ("_MSRA", "Mobile repair assessment"),
    ("_DELIV_REC", "Pick-up & delivery record"),
    ("_REIMBURSEMENT", "Reimbursement"),
    ("_OWNER_LETTER_SPN", "Owner letter (Spanish)"),
    ("_OWNER_LETTER", "Owner letter"),
];

/// A document's name from its address: Ford's file names, or the file name made readable
pub fn doc_name(url: &str) -> String {
    let file = url.split(['?', '#']).next().unwrap_or("").rsplit('/').next().unwrap_or("Document");
    let file = doc::query_value(&format!("x?f={file}"), "f").unwrap_or_else(|| file.to_string());
    let stem = file.strip_suffix(".pdf").or_else(|| file.strip_suffix(".PDF")).unwrap_or(&file).to_string();
    let up = stem.to_uppercase();
    // the known endings, longest first ("_TECHX" before "_TECH"); "_OWNER_LETTER" anywhere in it
    let known = NAMES.iter().find(|(end, _)| {
        if end.starts_with("_OWNER") || *end == "_REIMBURSEMENT" { up.contains(end) } else { up.ends_with(end) }
    });
    match known {
        Some((_, name)) => name.to_string(),
        None => stem.replace('_', " ").chars().take(40).collect(),
    }
}

fn is_pdf(url: &str) -> bool {
    url.split(['?', '#']).next().unwrap_or("").to_ascii_lowercase().ends_with(".pdf")
}

/// Addresses of PDFs in a page: links, frames, and ones in its scripts' text (Ford's page loads them into a frame)
pub fn pdf_links(html: &str, base: &str) -> Vec<String> {
    let d = Doc::parse(html);
    let mut found: Vec<String> = Vec::new();
    let mut add = |raw: &str| {
        if let Some(u) = resolve(base, raw.trim()) {
            if is_pdf(&u) && !found.contains(&u) {
                found.push(u);
            }
        }
    };
    for e in d.root().descendants() {
        for attr in ["href", "src", "data-src", "data-url"] {
            if let Some(v) = e.attr(attr) {
                add(v);
            }
        }
        // onclick="showDoc('…/24S10_BULL.pdf')"
        if let Some(v) = e.attr("onclick") {
            for piece in v.split(['\'', '"']) {
                add(piece);
            }
        }
    }
    // anywhere else (scripts): quoted addresses ending in .pdf
    for piece in html.split(['\'', '"']) {
        if piece.len() < 300 && !piece.contains(char::is_whitespace) && is_pdf(piece) {
            add(piece);
        }
    }
    found
}

/// The frames a page is made of (their addresses)
fn frames(html: &str, base: &str) -> Vec<String> {
    Doc::parse(html)
        .root()
        .find_all(|e| matches!(e.tag(), "frame" | "iframe"))
        .into_iter()
        .filter_map(|f| f.attr("src").and_then(|s| resolve(base, s)))
        .filter(|u| !is_pdf(u))
        .collect()
}

/// The script addresses in a page (`<script src=…>`: the page reader leaves scripts out, so they're read here)
fn script_sources(html: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = lower[from..].find("<script") {
        let start = from + at;
        let end = lower[start..].find('>').map(|e| start + e).unwrap_or(lower.len());
        let tag = &html[start..end];
        let tl = &lower[start..end];
        if let Some(s) = tl.find("src") {
            let rest = tag[s + 3..].trim_start();
            if let Some(rest) = rest.strip_prefix('=') {
                let rest = rest.trim_start();
                let q = rest.chars().next().unwrap_or(' ');
                let v = if q == '"' || q == '\'' {
                    rest[1..].split(q).next().unwrap_or("")
                } else {
                    rest.split(char::is_whitespace).next().unwrap_or("")
                };
                out.push(crate::html::decode(v));
            }
        }
        from = end;
    }
    out
}

/// When the document server's sign-on was last asked for (at most every 30 s)
static SIGNED_ON: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);

impl Ford {
    /// Ford's document server's cookies (CONTENT_AUTH, CONTENT_PERMISSIONS), as PTS's home page gets them: its page
    /// carries a one-time signed address on sso.fordservicecontent.com, and loading that hands them out. Skipped
    /// when they're there already (unless `force`). Whether they're there now.
    pub async fn content_sign_on(&self, force: bool) -> bool {
        let content = workbench_net::Url::parse("https://www.fordservicecontent.com/").expect("address");
        let has = |ford: &Ford| ford.http().cookie_sizes(&content).iter().any(|(n, _)| n.eq_ignore_ascii_case("CONTENT_AUTH"));
        if !force && has(self) {
            return true;
        }
        {
            let mut at = SIGNED_ON.lock().expect("sign-on lock");
            if at.is_some_and(|t| t.elapsed() < Duration::from_secs(30)) {
                return has(self);
            }
            *at = Some(Instant::now());
        }
        let Ok(home) = self.url("/") else { return false };
        let home = match self.text(home).await {
            Ok(h) => h,
            Err(e) => {
                self.log(&format!("content sign-on: PTS's home page: {e}"));
                return false;
            }
        };
        let srcs = script_sources(&home);
        let sso: Vec<String> = srcs
            .iter()
            .filter(|u| {
                let l = u.to_ascii_lowercase();
                (l.contains("fordservicecontent.com") || l.contains("akamaicallback")) && !l.contains("checkcookie")
            })
            .filter_map(|u| resolve("https://www.fordtechservice.dealerconnection.com/", u))
            .filter(|u| u.starts_with("https://"))
            .take(3)
            .collect();
        if sso.is_empty() {
            self.log(&format!("content sign-on: no sign-on address on PTS's home page ({} scripts)", srcs.len()));
            return false;
        }
        for u in sso {
            let Ok(url) = workbench_net::Url::parse(&u) else { continue };
            let host = url.host_str().unwrap_or("").to_string();
            let req = crate::browser_headers(
                Http::request(Method::GET, url)
                    .header("Accept", "*/*")
                    .header("Sec-Fetch-Dest", "script")
                    .header("Sec-Fetch-Mode", "no-cors")
                    .header("Sec-Fetch-Site", "cross-site"),
            );
            match self.http().send(req).await {
                Ok(r) => self.log(&format!("content sign-on {host} → {}", r.status.as_u16())),
                Err(e) => self.log(&format!("content sign-on {host}: {e}")),
            }
        }
        let now = has(self);
        self.log(&format!("   document server sign-on cookies: {}", if now { "yes" } else { "no" }));
        now
    }

    /// An FSA's page (from OASIS): its documents (PDFs), and its own words. A page made of frames is read through
    /// its frames.
    pub async fn fsa_page(&self, url: &str) -> Result<FsaPage> {
        let u = workbench_net::Url::parse(url).map_err(|_| Error::Format("Not an address on Ford's site".into()))?;
        if !crate::ford_host(u.host_str().unwrap_or("")) {
            return Err(Error::Format("Not an address on Ford's site".into()));
        }
        // the address itself may be the PDF
        if is_pdf(url) {
            return Ok(FsaPage {
                docs: vec![FsaDoc {
                    name: doc_name(url),
                    url: url.to_string(),
                }],
                blocks: Vec::new(),
            });
        }
        let res = self.fetch(Http::request(Method::GET, u)).await?;
        let at = res.url.to_string();
        if res.content_type.to_ascii_lowercase().contains("pdf") {
            return Ok(FsaPage {
                docs: vec![FsaDoc {
                    name: doc_name(&at),
                    url: at,
                }],
                blocks: Vec::new(),
            });
        }
        if crate::looks_like_sign_in(&res.body) {
            return Err(Error::NeedsLogin);
        }
        let mut links = pdf_links(&res.body, &at);
        let mut blocks = doc::read(&res.body, &at);
        // a page of frames: what's in them
        for f in frames(&res.body, &at).into_iter().take(4) {
            let Ok(fu) = workbench_net::Url::parse(&f) else { continue };
            if !crate::ford_host(fu.host_str().unwrap_or("")) {
                continue;
            }
            match self.fetch(Http::request(Method::GET, fu)).await {
                Ok(r) => {
                    let fat = r.url.to_string();
                    for l in pdf_links(&r.body, &fat) {
                        if !links.contains(&l) {
                            links.push(l);
                        }
                    }
                    if blocks.is_empty() || Block::text(&blocks).len() < 40 {
                        blocks.extend(doc::read(&r.body, &fat));
                    }
                }
                Err(e) => self.log(&format!("   FSA page frame: {e}")),
            }
        }
        let docs: Vec<FsaDoc> = links.into_iter().map(|u| FsaDoc { name: doc_name(&u), url: u }).collect();
        self.log(&format!(
            "   FSA page: {} documents ({})",
            docs.len(),
            docs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>().join(", ")
        ));
        Ok(FsaPage { docs, blocks })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_ford_documents() {
        assert_eq!(doc_name("https://x/fsa/24S10_BULL.pdf"), "Dealer bulletin");
        assert_eq!(doc_name("https://x/fsa/24S10_TECHX.pdf"), "Technical instructions");
        assert_eq!(doc_name("https://x/fsa/24S10_Owner_Letter_SPN.pdf"), "Owner letter (Spanish)");
        assert_eq!(doc_name("https://x/fsa/24S10_Owner_Letter.pdf?v=2"), "Owner letter");
        assert_eq!(doc_name("https://x/fsa/Some%20Other_Doc.pdf"), "Some Other Doc");
    }

    #[test]
    fn finds_the_documents() {
        let html = r##"<html><head><script>var first = "/content/24S10/24S10_BULL.pdf";</script>
            <script src="https://sso.fordservicecontent.com/x?sig=1&amp;cb=akamaiCallback"></script></head>
            <body><a href="https://www.fordservicecontent.com/content/24S10/24S10_TECHX.pdf" target="doc">Instructions</a>
            <a href="#" onclick="show('../content/24S10/24S10_Owner_Letter.pdf')">Owner</a>
            <a href="/other.aspx">Other</a><iframe name="doc" src="Blank.aspx"></iframe></body></html>"##;
        let base = "https://www.fordtechservice.dealerconnection.com/oasis/fsa/Index";
        let l = pdf_links(html, base);
        assert_eq!(
            l,
            [
                "https://www.fordservicecontent.com/content/24S10/24S10_TECHX.pdf",
                "https://www.fordtechservice.dealerconnection.com/oasis/content/24S10/24S10_Owner_Letter.pdf",
                "https://www.fordtechservice.dealerconnection.com/content/24S10/24S10_BULL.pdf",
            ]
        );
        assert_eq!(frames(html, base), ["https://www.fordtechservice.dealerconnection.com/oasis/fsa/Blank.aspx"]);
        assert_eq!(script_sources(html), ["https://sso.fordservicecontent.com/x?sig=1&cb=akamaiCallback"]);
    }
}
