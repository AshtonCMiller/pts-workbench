//! Web requests with the app's sign-in cookies.
//!
//! One [`Http`] per window/profile: it holds that profile's cookies (its Xtime and Ford sign-ins) and every part
//! (Xtime, Ford) makes its requests through it, like the Electron app's per-window session. The cookies can be saved
//! to a file in the profile's folder and loaded on the next start, so you stay signed in.
//!
//! Signing in itself happens in Xtime's / Ford's own sign-in pages (a small sign-in window); the cookies those pages
//! set are handed over with [`Http::add_cookie`].
//!
//! [`WebSocket`] (ws.rs) is a secure WebSocket connection for Xtime's chat server, through the same proxy as the
//! requests ([`proxy`]).

use std::fs;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

pub mod proxy;
mod ws;

pub use reqwest::{Method, StatusCode};
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
pub use url::Url;
pub use ws::WebSocket;

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("not a web address: {0}")]
    BadUrl(String),
    #[error("the request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("couldn't read or write the saved sign-ins: {0}")]
    Io(#[from] std::io::Error),
    #[error("the saved sign-ins couldn't be read: {0}")]
    Cookies(String),
    /// A WebSocket connection (Xtime's chat server) didn't work: what went wrong
    #[error("{0}")]
    WebSocket(String),
}

pub type Result<T> = std::result::Result<T, NetError>;

/// What came back from a request. The body is read in full (Xtime's and Ford's answers are small).
#[derive(Debug, Clone)]
pub struct Response {
    pub status: StatusCode,
    /// Where the request ended up, after any redirects
    pub url: Url,
    /// The request was sent on to another site, which is how a run-out sign-in shows (Xtime and Ford send you to
    /// their sign-in pages instead of answering)
    pub redirected: bool,
    pub content_type: String,
    pub body: String,
}

impl Response {
    /// 200–299, like `fetch`'s `ok`
    pub fn ok(&self) -> bool {
        self.status.is_success()
    }
    pub fn is_json(&self) -> bool {
        self.content_type.to_ascii_lowercase().contains("json")
    }
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> serde_json::Result<T> {
        serde_json::from_str(&self.body)
    }
}

/// What came back from [`Http::send_bytes`]
#[derive(Debug, Clone)]
pub struct Bytes {
    pub status: StatusCode,
    pub url: Url,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// A cookie as the sign-in window hands it over (`pts-signin` prints a list of these as JSON). Only kept in the
/// profile's cookie store; never logged.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SiteCookie {
    pub name: String,
    pub value: String,
    /// "www.inspect.app.coxautoinc.com" or ".coxautoinc.com"
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    /// When it runs out (seconds since 1970); none for a session cookie
    pub expires: Option<i64>,
}

impl SiteCookie {
    /// As a site would set it ("name=value; Domain=…; Path=…; Max-Age=…; Secure; HttpOnly") and the address it's for
    pub fn to_set_cookie(&self, now: i64) -> Option<(String, Url)> {
        let host = self.domain.trim_start_matches('.');
        let url = Url::parse(&format!("https://{host}/")).ok()?;
        let mut s = format!(
            "{}={}; Domain={host}; Path={}",
            self.name,
            self.value,
            if self.path.is_empty() { "/" } else { &self.path }
        );
        if let Some(t) = self.expires {
            s.push_str(&format!("; Max-Age={}", (t - now).max(0)));
        }
        if self.secure {
            s.push_str("; Secure");
        }
        if self.http_only {
            s.push_str("; HttpOnly");
        }
        Some((s, url))
    }
}

/// One request: `Http::request(Method::GET, url)` then `.header(..)`, `.body(..)`, and `http.send(req)`
#[derive(Debug, Clone)]
pub struct Request {
    pub method: Method,
    pub url: Url,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl Request {
    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
    }
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }
    /// A JSON body, with its content type
    pub fn json(self, value: &serde_json::Value) -> Self {
        self.header("Content-Type", "application/json").body(value.to_string())
    }
}

/// A profile's web requests and cookies. Cheap to clone (the clones share the cookies).
#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
    jar: Arc<CookieStoreMutex>,
}

impl Http {
    /// No cookies yet (signed in nowhere)
    pub fn new(user_agent: &str) -> Result<Self> {
        Self::with_store(user_agent, CookieStore::default())
    }

    /// With the sign-ins saved last time (see [`Http::save_cookies`]); a missing file is the same as none
    pub fn with_saved_cookies(user_agent: &str, file: &Path) -> Result<Self> {
        let store = match fs::File::open(file) {
            Ok(f) => cookie_store::serde::json::load_all(BufReader::new(f)).map_err(|e| NetError::Cookies(e.to_string()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => CookieStore::default(),
            Err(e) => return Err(e.into()),
        };
        Self::with_store(user_agent, store)
    }

    fn with_store(user_agent: &str, store: CookieStore) -> Result<Self> {
        let jar = Arc::new(CookieStoreMutex::new(store));
        let client = reqwest::Client::builder()
            .cookie_provider(jar.clone())
            .user_agent(user_agent)
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(90))
            .build()?;
        Ok(Self { client, jar })
    }

    /// Saves every sign-in cookie (including session-only ones, which is what keeps you signed in between starts).
    /// The file holds sign-ins: it belongs in the profile's own folder and nowhere else. Written whole or not at all.
    pub fn save_cookies(&self, file: &Path) -> Result<()> {
        let tmp = file.with_extension("tmp");
        {
            let mut w = BufWriter::new(fs::File::create(&tmp)?);
            let store = self.jar.lock().expect("cookie store lock");
            cookie_store::serde::json::save_incl_expired_and_nonpersistent(&store, &mut w).map_err(|e| NetError::Cookies(e.to_string()))?;
            w.flush()?;
        }
        fs::rename(&tmp, file)?;
        Ok(())
    }

    /// A cookie's value for an address (e.g. Xtime's XSRF-TOKEN)
    pub fn cookie(&self, url: &Url, name: &str) -> Option<String> {
        let store = self.jar.lock().expect("cookie store lock");
        store
            .matches(url)
            .into_iter()
            .find(|c| c.name() == name)
            .map(|c| c.value().to_string())
    }

    /// Adds a cookie as a site would set it ("name=value; Domain=…; Path=/; Secure"), e.g. from the sign-in window
    pub fn add_cookie(&self, set_cookie: &str, url: &Url) -> Result<()> {
        let mut store = self.jar.lock().expect("cookie store lock");
        store.parse(set_cookie, url).map_err(|e| NetError::Cookies(e.to_string()))?;
        Ok(())
    }

    /// Adds the cookies a sign-in window handed over; how many were taken
    pub fn add_site_cookies(&self, cookies: &[SiteCookie]) -> usize {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        cookies
            .iter()
            .filter_map(|c| c.to_set_cookie(now))
            .filter(|(s, url)| self.add_cookie(s, url).is_ok())
            .count()
    }

    /// The cookies sent to an address: each one's name and size (name and value, in characters). Never their values.
    pub fn cookie_sizes(&self, url: &Url) -> Vec<(String, usize)> {
        let store = self.jar.lock().expect("cookie store lock");
        store
            .matches(url)
            .into_iter()
            .map(|c| (c.name().to_string(), c.name().len() + c.value().len() + 2))
            .collect()
    }

    /// Throws away the cookies sent to an address whose names pass `pick`; the names thrown away
    pub fn remove_cookies(&self, url: &Url, pick: impl Fn(&str) -> bool) -> Vec<String> {
        let mut store = self.jar.lock().expect("cookie store lock");
        let host = url.host_str().unwrap_or_default().to_string();
        let gone: Vec<(String, String, String)> = store
            .matches(url)
            .into_iter()
            .filter(|c| pick(c.name()))
            .map(|c| {
                let domain = c
                    .domain()
                    .map(|d| d.trim_start_matches('.').to_string())
                    .unwrap_or_else(|| host.clone());
                (c.name().to_string(), domain, c.path().unwrap_or("/").to_string())
            })
            .collect();
        for (name, domain, path) in &gone {
            // removed by its key; for one kept under a slightly different key, an expired copy replaces it
            if store.remove(domain, path, name).is_none() {
                let _ = store.parse(&format!("{name}=; Max-Age=0; Domain={domain}; Path={path}"), url);
            }
        }
        gone.into_iter().map(|(n, _, _)| n).collect()
    }

    /// Whether there are any cookies for an address (a sign-in, as far as the app can tell without asking)
    pub fn has_cookies(&self, url: &Url) -> bool {
        let store = self.jar.lock().expect("cookie store lock");
        !store.matches(url).is_empty()
    }

    pub fn request(method: Method, url: Url) -> Request {
        Request {
            method,
            url,
            headers: Vec::new(),
            body: None,
        }
    }

    /// Sends a request with this profile's cookies, following redirects, and reads the whole answer
    pub async fn send(&self, req: Request) -> Result<Response> {
        let asked_host = req.url.host_str().unwrap_or_default().to_string();
        let mut b = self.client.request(req.method, req.url);
        for (k, v) in &req.headers {
            b = b.header(k.as_str(), v.as_str());
        }
        if let Some(body) = req.body {
            b = b.body(body);
        }
        let res = b.send().await?;
        let status = res.status();
        let url = res.url().clone();
        let content_type = res
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let redirected = url.host_str().unwrap_or_default() != asked_host;
        let body = res.text().await?;
        Ok(Response {
            status,
            url,
            redirected,
            content_type,
            body,
        })
    }

    /// Sends a request and reads the answer as bytes (pictures, PDFs)
    pub async fn send_bytes(&self, req: Request) -> Result<Bytes> {
        let mut b = self.client.request(req.method, req.url);
        for (k, v) in &req.headers {
            b = b.header(k.as_str(), v.as_str());
        }
        if let Some(body) = req.body {
            b = b.body(body);
        }
        let res = b.send().await?;
        let status = res.status();
        let url = res.url().clone();
        let content_type = res
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = res.bytes().await?.to_vec();
        Ok(Bytes {
            status,
            url,
            content_type,
            bytes,
        })
    }

    pub async fn get(&self, url: Url) -> Result<Response> {
        self.send(Self::request(Method::GET, url)).await
    }
}

/// An address on a site: a path ("/api/…") or a full address
pub fn join(base: &Url, path_or_url: &str) -> Result<Url> {
    base.join(path_or_url).map_err(|_| NetError::BadUrl(path_or_url.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookies_are_kept_per_site() {
        let http = Http::new("test").unwrap();
        let xtime = Url::parse("https://www.inspect.app.coxautoinc.com/").unwrap();
        let ford = Url::parse("https://www.fordtechservice.dealerconnection.com/").unwrap();
        http.add_cookie("XSRF-TOKEN=abc%3D; Path=/; Secure", &xtime).unwrap();
        assert_eq!(http.cookie(&xtime, "XSRF-TOKEN").as_deref(), Some("abc%3D"));
        assert_eq!(http.cookie(&ford, "XSRF-TOKEN"), None);
    }

    #[test]
    fn signin_cookies_are_taken() {
        let http = Http::new("test").unwrap();
        let site = Url::parse("https://www.inspect.app.coxautoinc.com/api/x").unwrap();
        assert!(!http.has_cookies(&site));
        let c = |name: &str, domain: &str, expires| SiteCookie {
            name: name.into(),
            value: "v".into(),
            domain: domain.into(),
            path: "/".into(),
            secure: true,
            http_only: true,
            expires,
        };
        let taken = http.add_site_cookies(&[
            c("a", ".coxautoinc.com", None),
            c("b", "www.inspect.app.coxautoinc.com", Some(i64::MAX / 2)),
            c("old", ".coxautoinc.com", Some(1)),
        ]);
        assert!(taken >= 2); // the run-out one may or may not count as taken
        assert_eq!(http.cookie(&site, "a").as_deref(), Some("v"));
        assert_eq!(http.cookie(&site, "b").as_deref(), Some("v"));
        assert_eq!(http.cookie(&site, "old"), None); // already run out
        assert!(http.has_cookies(&site));
    }

    #[test]
    fn saved_cookies_come_back() {
        let dir = std::env::temp_dir().join(format!("wb-net-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("cookies.json");
        let site = Url::parse("https://www.inspect.app.coxautoinc.com/").unwrap();
        let http = Http::new("test").unwrap();
        http.add_cookie("session=one; Path=/; Secure; HttpOnly", &site).unwrap(); // session-only: still saved
        http.save_cookies(&file).unwrap();
        let again = Http::with_saved_cookies("test", &file).unwrap();
        assert_eq!(again.cookie(&site, "session").as_deref(), Some("one"));
        fs::remove_dir_all(&dir).ok();
    }
}
