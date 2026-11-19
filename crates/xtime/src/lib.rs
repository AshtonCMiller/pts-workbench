//! Xtime for PTS Workbench: asked directly with the profile's Xtime sign-in (its cookies), the way Xtime's own
//! pages ask. Ported from the Electron app's `src/xtime/` (one file there, one module here):
//!
//! | here          | there          | what                                                          |
//! |---------------|----------------|---------------------------------------------------------------|
//! | [`employee`]  | `employee.js`  | your Xtime technician ID from your employee number            |
//! | [`dashboard`] | `dashboard.js` | your ROs from Xtime's dashboard                               |
//! | [`ro`]        | `ro.js`        | one RO for the Xtime View; marking lines done                 |
//! | [`inspect`]   | `inspect.js`   | the RO's inspection form: Green / Yellow / Red, values, forms |
//! | [`service`]   | `inspect.js`   | Add / Edit / Delete Service: the store's services, op codes   |
//! | [`chat`]      | `chat.js`      | Dealership chat: conversations, messages, unread count        |
//! | [`xml`]       | `xml.js`       | reading chat's messages (XMPP)                                |
//! | [`xmpp`]      | `chat.js`      | chat's server: sending messages, marking them read            |
//!
//! Each module has a pure part that reads Xtime's answer (tested without Xtime) and a method on [`Xtime`] that asks.
//! Customer information isn't kept beyond what the app shows, and only on this computer.

pub mod chat;
pub mod dashboard;
pub mod employee;
pub mod inspect;
mod json;
pub mod ro;
pub mod service;
pub mod stage;
pub mod xml;
pub mod xmpp;

use std::sync::{Arc, Mutex};

use serde_json::Value;
use workbench_net::{Http, Method, NetError, Request, Response, Url};

/// Where Xtime keeps things (the same for every store; the store itself comes from the app's settings)
pub mod paths {
    pub const DATA_HOST: &str = "www.inspect.app.coxautoinc.com";
    pub const DASHBOARD: &str = "/api/RepairOrderDashboard/?showCustomerROs=true&showInternalROs=true&showExpressROs=&showWaiterROs=";
    /// `{STORE}`: the store ID from settings
    pub const EMPLOYEES: &str = "/JS/Employees?storeID={STORE}";
    /// `{RO}`: the RO number
    pub const RO_DETAILS: &str = "/RepairOrder/Details/{RO}";

    /// The staff list's address for a store
    pub fn employees(store: &str) -> String {
        EMPLOYEES.replace("{STORE}", &crate::employee::encode(store.trim()))
    }
}

/// Signing in: the sign-in window starts here (Xtime sends it on to Cox's sign-in page when needed). After signing in,
/// Cox sends you to Xtime's Scheduling page (on [`signin::LANDING`]), not to Inspect, so the window then opens Inspect
/// the way Scheduling's menu does ([`signin::INSPECT`]). Inspect shows inside that page, so the window is done once
/// Inspect ([`paths::DATA_HOST`]) has given it a new sign-in cookie (or the window is on Inspect itself). The cookies
/// of these sites are taken (Cox's sign-in, Xtime's login and chat).
pub mod signin {
    pub const START: &str = "https://www.inspect.app.coxautoinc.com/";
    pub const DONE_HOST: &str = super::paths::DATA_HOST;
    pub const COOKIE_DOMAINS: &[&str] = &["coxautoinc.com", "xtime.com"];
    /// Where Cox sends you after signing in (Scheduling: x1.xtime.app.coxautoinc.com/panama/…)
    pub const LANDING: &str = "xtime.app.coxautoinc.com";
    /// Inspect, opened from there. `{DOMAIN}`: the landing page's address without its first part
    /// (xtime.app.coxautoinc.com); `{STORE}`: the store ID from settings
    pub const INSPECT: &str = "https://login.{DOMAIN}/?pt=svp&view=svp&StoreID={STORE}";
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The sign-in has run out: Xtime answered 401/403 or sent the request to its sign-in pages. The app signs in again.
    #[error("Xtime session expired")]
    SessionExpired,
    #[error("Xtime returned HTTP {0}")]
    Status(u16),
    #[error(transparent)]
    Net(#[from] NetError),
    /// Xtime answered, but not in a form the app understands
    #[error("{0}")]
    Format(String),
    #[error("{0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// One profile's Xtime: its requests (with that profile's cookies) and what's remembered between them
#[derive(Clone)]
pub struct Xtime {
    http: Http,
    base: Url,
    /// The store's staff list (names for advisor numbers), loaded once; cleared with [`Xtime::reset`]
    staff: Arc<Mutex<Option<Arc<Vec<Value>>>>>,
    /// What Xtime's scripts hold (the store's data, its forms…), by their address; cleared with [`Xtime::reset`]
    scripts: Arc<Mutex<std::collections::HashMap<String, Value>>>,
    /// Chat's token and settings (in memory only); the token is cleared with [`Xtime::reset`]
    chat: Arc<Mutex<chat::ChatMem>>,
}

impl Xtime {
    /// `data_host`: where Xtime's data is (normally [`paths::DATA_HOST`])
    pub fn new(http: Http, data_host: &str) -> Result<Self> {
        let base = Url::parse(&format!("https://{data_host}/")).map_err(|_| Error::Format(format!("not a host name: {data_host}")))?;
        Ok(Self {
            http,
            base,
            staff: Arc::default(),
            scripts: Arc::default(),
            chat: Arc::default(),
        })
    }

    pub fn base(&self) -> &Url {
        &self.base
    }

    /// Forget what's remembered (another store, or signed in as someone else)
    pub fn reset(&self) {
        *self.staff.lock().expect("staff lock") = None;
        self.scripts.lock().expect("scripts lock").clear();
        self.chat.lock().expect("chat lock").token = None;
    }

    /// Xtime's anti-forgery token, sent with every change as X-XSRF-TOKEN (Xtime's page does the same)
    pub fn xsrf(&self) -> Option<String> {
        self.http.cookie(&self.base, "XSRF-TOKEN").map(|v| percent_decode(&v))
    }

    /// A request to an Xtime address ("/api/…" or a full address), with what Xtime's page sends along
    pub(crate) fn request(&self, method: Method, path: &str) -> Result<Request> {
        let url = workbench_net::join(&self.base, path)?;
        let mut req = Http::request(method.clone(), url).header("Referer", self.base.as_str());
        if method != Method::GET && method != Method::HEAD {
            req = req.header("Origin", self.base.as_str().trim_end_matches('/'));
            if let Some(t) = self.xsrf() {
                req = req.header("X-XSRF-TOKEN", t);
            }
        }
        Ok(req)
    }

    pub(crate) async fn send(&self, req: Request) -> Result<Response> {
        Ok(self.http.send(req).await?)
    }

    /// The store's staff list, loaded once (for names); `None` when it can't be had
    pub(crate) async fn staff_list(&self, path: &str) -> Option<Arc<Vec<Value>>> {
        if let Some(s) = self.staff.lock().expect("staff lock").clone() {
            return Some(s);
        }
        let req = self.request(Method::GET, path).ok()?;
        let res = self.send(req).await.ok()?;
        if !res.ok() {
            return None;
        }
        let list = Arc::new(employee::parse_staff(&res.body)?);
        *self.staff.lock().expect("staff lock") = Some(list.clone());
        Some(list)
    }
}

/// "abc%3D" → "abc=" (cookie values are sent percent-encoded)
fn percent_decode(s: &str) -> String {
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_the_xsrf_cookie() {
        assert_eq!(super::percent_decode("abc%3D%3D"), "abc==");
        assert_eq!(super::percent_decode("plain"), "plain");
        assert_eq!(super::percent_decode("odd%2"), "odd%2");
    }
}
