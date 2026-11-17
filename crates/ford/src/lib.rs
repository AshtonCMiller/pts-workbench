//! Ford PTS for PTS Workbench: asked directly with the profile's Ford sign-in (its cookies), the way Ford's own pages
//! ask. Ported from the Electron app's `src/manual.js` and its tabs' pages, one module per part:
//!
//! | module        | what                                                                        |
//! |---------------|-----------------------------------------------------------------------------|
//! | [`html`]      | reading Ford's pages (a forgiving HTML reader)                              |
//! | [`vehicle`]   | the VIN in PTS: decoding it, its book's settings                            |
//! | [`manual`]    | the Workshop Manual: its contents (only this vehicle), procedures           |
//! | [`oasis`]     | OASIS: vehicle information, field service actions, warnings, warranty       |
//! | [`tsb`]       | TSBs / GSBs / SSMs for the VIN; one article (or FSA document), or its PDF   |
//! | [`fsa`]       | an FSA's page from OASIS: its documents (PDFs); the document server's sign-on |
//! | [`doc`]       | Ford's documents read into a plain document the app draws in its own style  |
//! | (to come)     | the Workshop Manual and PC/ED, wiring, TSBs, Connected Vehicle, SLTS, Diagnostics |
//!
//! Each module has a pure part that reads Ford's answer (tested without Ford) and a method on [`Ford`] that asks.
//! The dealer code comes from the app's settings (set on first setup), never from the code. Nothing about the
//! vehicle's owner is kept.

pub mod diagnostics;
pub mod doc;
pub mod fsa;
pub mod html;
pub mod labor;
pub mod manual;
pub mod oasis;
pub mod tsb;
pub mod vehicle;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use workbench_net::{Http, Method, NetError, Request, Response, Url};

/// Where Ford PTS is
pub mod paths {
    pub const PTS: &str = "https://www.fordtechservice.dealerconnection.com";
    pub const PTS_HOST: &str = "www.fordtechservice.dealerconnection.com";
}

/// Signing in: the sign-in window starts at PTS (Ford sends it on to its sign-in pages when needed) and is done once
/// it's back on PTS. The cookies of these sites are taken.
pub mod signin {
    pub const START: &str = "https://www.fordtechservice.dealerconnection.com/";
    pub const DONE_HOST: &str = super::paths::PTS_HOST;
    pub const COOKIE_DOMAINS: &[&str] = &["dealerconnection.com", "ford.com"];
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The Ford sign-in has run out (Ford sent the request to its sign-in pages). The app signs in again.
    #[error("Sign in to PTS")]
    NeedsLogin,
    #[error("Ford returned an error ({0})")]
    Status(u16),
    #[error(transparent)]
    Net(#[from] NetError),
    #[error("{0}")]
    Format(String),
    /// Ford's book pages need what PTS's own vehicle page sets up (its PTSSession / FeaturesAttributes cookies), and a
    /// profile that hasn't shown PTS's vehicle page yet has none: the app opens the vehicle in PTS once (unseen)
    #[error("PTS isn't set up for this vehicle yet")]
    NotSetUp,
}

pub type Result<T> = std::result::Result<T, Error>;

/// The sites Ford's pages and pictures come from: an answer from anywhere else is Ford's sign-in
pub fn ford_host(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    ["dealerconnection.com", "fordservicecontent.com"]
        .iter()
        .any(|d| h == *d || h.ends_with(&format!(".{d}")))
}

/// Ford's own leftovers from unfinished sign-ins (each trip to its sign-in sets one); safe to throw away
pub fn sign_in_leftover(name: &str) -> bool {
    name.to_ascii_lowercase().starts_with("openidconnect.nonce.")
}

/// What Ford's error page says (its title, first heading and first words), short
pub fn error_page_says(body: &str) -> String {
    let doc = html::Doc::parse(body);
    let h = doc
        .root()
        .find(|e| matches!(e.tag(), "h1" | "h2"))
        .map(|e| e.text())
        .unwrap_or_default();
    let words = doc.root().text();
    let mut parts: Vec<String> = [doc.title(), h, words].into_iter().filter(|x| !x.is_empty()).collect();
    parts.dedup();
    parts.join(" · ").chars().take(240).collect()
}

/// A sign-in page instead of what was asked for (a form that posts to a sign-in)
pub fn looks_like_sign_in(page: &str) -> bool {
    let l = page.to_ascii_lowercase();
    let mut rest = l.as_str();
    while let Some(at) = rest.find("<form") {
        let tag = &rest[at..rest[at..].find('>').map(|e| at + e).unwrap_or(rest.len())];
        if ["login", "signin", "sso", "adfs"].iter().any(|w| tag.contains(w)) {
            // (as the Electron app: PTS's own pages have forms of their own and these in them)
            return !(l.contains("var params") || l.contains("contentdiv") || l.contains("tree"));
        }
        rest = &rest[at + 5..];
    }
    false
}

/// The first sign-in form tag in a page (for the log: its tag only, no values typed in it)
fn sign_in_form(page: &str) -> String {
    let l = page.to_ascii_lowercase();
    let Some(at) = l.find("<form") else { return String::new() };
    let end = l[at..].find('>').map(|e| at + e + 1).unwrap_or(l.len());
    page.get(at..end).unwrap_or("").chars().take(200).collect()
}

/// A short record of each request (address, status, where it ended up, which cookies went with it: their names
/// only), for working out problems: `manual.log` in the app's folder, started over past 400 KB
#[derive(Clone, Default)]
pub struct Log(Option<Arc<std::path::PathBuf>>);

impl Log {
    pub fn to(file: std::path::PathBuf) -> Self {
        Log(Some(Arc::new(file)))
    }

    pub fn line(&self, line: &str) {
        let Some(file) = &self.0 else { return };
        if std::fs::metadata(&**file).is_ok_and(|m| m.len() > 400_000) {
            let _ = std::fs::remove_file(&**file);
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&**file) {
            use std::io::Write;
            let _ = writeln!(f, "{stamp} {line}");
        }
    }
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n).collect::<String>())
    } else {
        s.to_string()
    }
}

/// Akamai's "Access Denied" page ("You don't have permission to access … Reference #18.…")
pub fn turned_away(body: &str) -> bool {
    let l = body.to_ascii_lowercase();
    l.contains("access denied") && (l.contains("reference #") || l.contains("errors.edgesuite.net"))
}

/// What a browser (Chrome, as the app says it is) sends along with each request, added where the request doesn't
/// say otherwise: Ford's website protection turns away requests that don't look like a browser's
pub(crate) fn browser_headers(mut req: Request) -> Request {
    let json = req
        .headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("accept") && v.contains("json"))
        || req.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("x-requested-with"));
    let page = [
        (
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7",
        ),
        ("Sec-Fetch-Dest", if json { "empty" } else { "document" }),
        ("Sec-Fetch-Mode", if json { "cors" } else { "navigate" }),
        ("Sec-Fetch-Site", "same-origin"),
        ("Accept-Language", "en-US,en;q=0.9"),
        ("sec-ch-ua", r#""Chromium";v="130", "Google Chrome";v="130", "Not?A_Brand";v="99""#),
        ("sec-ch-ua-mobile", "?0"),
        ("sec-ch-ua-platform", r#""Windows""#),
        ("Referer", "https://www.fordtechservice.dealerconnection.com/"),
    ];
    for (k, v) in page {
        if !req.headers.iter().any(|(h, _)| h.eq_ignore_ascii_case(k)) {
            req.headers.push((k.to_string(), v.to_string()));
        }
    }
    if !json && !req.headers.iter().any(|(h, _)| h.eq_ignore_ascii_case("upgrade-insecure-requests")) {
        req.headers.push(("Upgrade-Insecure-Requests".into(), "1".into()));
    }
    req
}

/// One profile's Ford PTS: its requests, with that profile's cookies, and what's remembered between them
#[derive(Clone)]
pub struct Ford {
    http: Http,
    base: Url,
    /// The dealership's P&A code, from settings
    dealer_code: String,
    /// Each VIN's book (its settings for the manual), once read
    books: Arc<Mutex<HashMap<String, vehicle::Book>>>,
    /// Each VIN's decode, once read (asking Ford for it takes a couple of seconds)
    vehicles: Arc<Mutex<HashMap<String, vehicle::Vehicle>>>,
    /// When the cookies were last trimmed (at most once a minute)
    trimmed_at: Arc<Mutex<Option<Instant>>>,
    log: Log,
    /// Where the manual's contents are kept (see [`Ford::with_cache`])
    cache: Option<std::path::PathBuf>,
    /// Each VIN's side of the diagnostics search, and when it was read (see diagnostics.rs)
    diag: Arc<Mutex<HashMap<String, (Instant, diagnostics::DiagContext)>>>,
    /// Each VIN's SLTS model code ("A8"), once SLTS has said (see labor.rs)
    slts_codes: Arc<Mutex<HashMap<String, String>>>,
    /// Each VIN's copy of PTS's FeaturesAttributes cookie (its options, for the diagnostics search), and when it was
    /// looked for (none: PTS had none)
    features: Arc<Mutex<HashMap<String, (Instant, Option<String>)>>>,
}

impl Ford {
    pub fn new(http: Http, dealer_code: impl Into<String>) -> Result<Self> {
        let base = Url::parse(paths::PTS).map_err(|_| Error::Format("PTS address".into()))?;
        Ok(Self {
            http,
            base,
            dealer_code: dealer_code.into(),
            books: Arc::default(),
            vehicles: Arc::default(),
            trimmed_at: Arc::default(),
            log: Log::default(),
            cache: None,
            diag: Arc::default(),
            slts_codes: Arc::default(),
            features: Arc::default(),
        })
    }

    /// Keeps a record of its requests in `file` (see [`Log`])
    pub fn with_log(mut self, file: std::path::PathBuf) -> Self {
        self.log = Log::to(file);
        self
    }

    pub fn log(&self, line: &str) {
        self.log.line(line);
    }

    pub fn base(&self) -> &Url {
        &self.base
    }

    pub fn dealer_code(&self) -> &str {
        &self.dealer_code
    }

    pub fn set_dealer_code(&mut self, code: impl Into<String>) {
        self.dealer_code = code.into();
    }

    pub fn http(&self) -> &Http {
        &self.http
    }

    /// Forget what's remembered (signed in as someone else)
    pub fn reset(&self) {
        self.books.lock().expect("books lock").clear();
        self.diag.lock().expect("diag lock").clear();
        self.features.lock().expect("features lock").clear();
    }

    pub(crate) fn url(&self, path: &str) -> Result<Url> {
        Ok(workbench_net::join(&self.base, path)?)
    }

    /// Sends a request to Ford. A run-out sign-in ([`Error::NeedsLogin`]) is told apart from Ford's own errors.
    /// Ford turns a request down ("Request Too Long") when the cookies sent with it pass its limit: what piles up is
    /// its sign-in leftovers, which are thrown away and the request tried once more. Ford's sign-in cookie comes in
    /// pieces that only work together; with one missing, the rest go too, so PTS asks to sign in again.
    pub(crate) async fn fetch(&self, req: Request) -> Result<Response> {
        let req = browser_headers(req);
        let mut retried = false;
        loop {
            // which cookies go with it: names only, Ford's sign-in leftovers counted
            let sent = self.http.cookie_sizes(&req.url);
            let leftovers = sent.iter().filter(|(n, _)| sign_in_leftover(n)).count();
            let names: Vec<&str> = sent.iter().map(|(n, _)| n.as_str()).filter(|n| !sign_in_leftover(n)).collect();
            let size: usize = sent.iter().map(|(_, s)| s).sum();
            let res = match self.http.send(req.clone()).await {
                Ok(r) => r,
                Err(e) => {
                    self.log(&format!("{:?} {} → failed: {e}", req.method, short(req.url.as_str(), 220)));
                    return Err(e.into());
                }
            };
            let host = res.url.host_str().unwrap_or_default().to_string();
            self.log(&format!(
                "{:?} {} → {} {}{} · cookies {} ({} characters; {}{})",
                req.method,
                short(req.url.as_str(), 220),
                res.status.as_u16(),
                res.content_type,
                if res.url != req.url {
                    format!(" (ended at {})", short(res.url.as_str(), 160))
                } else {
                    String::new()
                },
                sent.len(),
                size,
                names.join(","),
                if leftovers > 0 {
                    format!(", {leftovers} sign-in leftovers")
                } else {
                    String::new()
                },
            ));
            // Ford's website protection (Akamai) turning the request away: not a sign-in problem
            if res.status.as_u16() == 403 && turned_away(&res.body) {
                self.log(&format!(
                    "   turned away by the website's protection: {}",
                    short(&error_page_says(&res.body), 240)
                ));
                return Err(Error::Format("Ford's website turned the app's request away (Access Denied)".into()));
            }
            if matches!(res.status.as_u16(), 401 | 403) || !ford_host(&host) {
                if res.status.as_u16() == 403 {
                    self.log(&format!("   Ford's page said: {}", short(&error_page_says(&res.body), 240)));
                }
                self.log(&format!(
                    "   signed out: {}",
                    if ford_host(&host) {
                        "Ford said no".to_string()
                    } else {
                        format!("sent to {host}")
                    }
                ));
                return Err(Error::NeedsLogin);
            }
            if res.ok() {
                return Ok(res);
            }
            let said = error_page_says(&res.body).to_ascii_lowercase();
            self.log(&format!("   Ford's page said: {}", short(&said, 240)));
            if res.status.as_u16() == 400 && (said.contains("request too long") || said.contains("header too long")) && !retried {
                retried = true;
                if self.trim_cookies(&res.url) {
                    continue;
                }
                return Err(Error::NeedsLogin);
            }
            if said.contains("chunked cookie is incomplete") {
                self.http
                    .remove_cookies(&res.url, |n| n.to_ascii_lowercase().starts_with("ford.tso."));
                return Err(Error::NeedsLogin);
            }
            return Err(Error::Status(res.status.as_u16()));
        }
    }

    /// A page's text (a sign-in page instead counts as signed out)
    pub(crate) async fn text(&self, url: Url) -> Result<String> {
        let res = self.fetch(Http::request(Method::GET, url)).await?;
        // (its title only: the page itself may hold the vehicle's details)
        self.log(&format!(
            "   {} characters, titled {:?}",
            res.body.len(),
            short(&html::Doc::parse(&res.body).title(), 120)
        ));
        if looks_like_sign_in(&res.body) {
            self.log(&format!("   signed out: a sign-in form in the page ({})", sign_in_form(&res.body)));
            return Err(Error::NeedsLogin);
        }
        Ok(res.body)
    }

    /// Throws away Ford's sign-in leftovers when they're what made the cookies too long; when they aren't enough,
    /// every cookie for the address (PTS asks to sign in again). Whether the cookies fit again.
    fn trim_cookies(&self, url: &Url) -> bool {
        {
            let mut at = self.trimmed_at.lock().expect("trim lock");
            if at.is_some_and(|t| t.elapsed() < Duration::from_secs(60)) {
                return false;
            }
            *at = Some(Instant::now());
        }
        let sizes = self.http.cookie_sizes(url);
        let leftovers: usize = sizes.iter().filter(|(n, _)| sign_in_leftover(n)).map(|(_, s)| s).sum();
        let total: usize = sizes.iter().map(|(_, s)| s).sum();
        if leftovers > 0 && total - leftovers < 12_000 {
            return !self.http.remove_cookies(url, sign_in_leftover).is_empty();
        }
        self.http.remove_cookies(url, |_| true);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_ford_from_its_sign_in() {
        assert!(ford_host("www.fordtechservice.dealerconnection.com"));
        assert!(ford_host("x.fordservicecontent.com"));
        assert!(!ford_host("fedlogin.ford.com"));
        assert!(!ford_host("notdealerconnection.com"));
        assert!(sign_in_leftover("OpenIdConnect.nonce.abc123"));
        assert!(!sign_in_leftover("Ford.TSO.PTSSuiteC1"));
        assert!(looks_like_sign_in(r#"<html><form method=post action="https://x/adfs/ls">"#));
        assert!(!looks_like_sign_in(r#"<form action="/search">"#));
        assert_eq!(
            error_page_says("<html><title>Bad Request</title><h2>Request Too Long</h2>HTTP Error 400</html>"),
            "Bad Request · Request Too Long · Bad Request Request Too Long HTTP Error 400"
        );
    }
}
