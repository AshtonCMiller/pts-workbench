//! Feedback: bug reports and feature ideas, sent to the app's sync server (`sync-server/src/worker.js`, `/v1/reports`),
//! the same requests as the Electron app's `workbench/feedback.js`.
//!
//! A report carries the person's name, what they wrote, the app version, the computer's system and which screen they
//! were on: never VINs, RO numbers or customer details. Each computer has a random reporter ID ([`reporter_id`]) so
//! people can see their own reports' status and replies. One person (the admin) reads them all with a password.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use workbench_net::{Http, Method, NetError, Response, Url};

/// Where the sync server is
pub const SERVER: &str = "https://prodealer.works";
/// The employee number that sees everyone's reports (with the server's password)
pub const ADMIN_EMPLOYEE: &str = "95385";

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("couldn't reach the server: {0}")]
    Net(String),
    #[error("you've sent a lot in the last hour; try again later")]
    TooMany,
    #[error("that password was wrong")]
    BadPassword,
    #[error("the server has no admin password set up yet (REPORTS_ADMIN_KEY)")]
    NoAdminKey,
    #[error("the server said: {0}")]
    Server(String),
}

impl From<NetError> for Error {
    fn from(e: NetError) -> Self {
        Error::Net(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Bug,
    Feature,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Bug => "Bug",
            Kind::Feature => "Idea",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    New,
    Planned,
    InProgress,
    Done,
    WontDo,
}

impl Status {
    pub const ALL: [Status; 5] = [Status::New, Status::Planned, Status::InProgress, Status::Done, Status::WontDo];

    pub fn label(self) -> &'static str {
        match self {
            Status::New => "New",
            Status::Planned => "Planned",
            Status::InProgress => "Working on it",
            Status::Done => "Done",
            Status::WontDo => "Won't do",
        }
    }

    /// Done and Won't do are closed; the rest are open
    pub fn is_open(self) -> bool {
        !matches!(self, Status::Done | Status::WontDo)
    }
}

impl Report {
    /// "v0.5.0" when it says which version fixed it
    pub fn fixed_in_label(&self) -> Option<String> {
        self.fixed_in
            .as_deref()
            .map(|v| v.trim().trim_start_matches(['v', 'V']))
            .filter(|v| !v.is_empty())
            .map(|v| format!("v{v}"))
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// One report as the server lists it. "Mine" only has the first fields; the admin's list has them all.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Report {
    pub id: i64,
    /// Milliseconds since 1970
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: Option<i64>,
    pub kind: Kind,
    pub title: String,
    pub status: Status,
    #[serde(default)]
    pub reply: Option<String>,
    /// The version it was fixed in ("0.5.0"), when it's done (servers from before this have none)
    #[serde(default)]
    pub fixed_in: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub screen: Option<String>,
}

/// A report to send
#[derive(Debug, Clone, Serialize)]
pub struct NewReport {
    pub kind: Kind,
    /// Up to 140 characters (the server cuts the rest)
    pub title: String,
    /// Up to 6000
    pub body: String,
    pub name: String,
    pub reporter: String,
    pub version: String,
    pub platform: String,
    /// Where in the app ("Workbench › Service Manual › OASIS"): never which vehicle or RO
    pub screen: String,
}

/// What to change on a report (the admin); fields left out stay as they are
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Change {
    pub status: Option<Status>,
    pub reply: Option<String>,
    /// "0.5.0" (a leading "v" is dropped), or "" to clear it
    pub fixed_in: Option<String>,
}

/// The server's list, from `{ "reports": [...] }`
pub fn parse_reports(body: &str) -> Result<Vec<Report>> {
    #[derive(Deserialize)]
    struct List {
        #[serde(default)]
        reports: Vec<Report>,
    }
    serde_json::from_str::<List>(body)
        .map(|l| l.reports)
        .map_err(|e| Error::Server(format!("unreadable list ({e})")))
}

/// What a failed answer says (`{ "error": "…" }`), as an error
fn failed(res: &Response) -> Error {
    let said = serde_json::from_str::<Value>(&res.body)
        .ok()
        .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_string));
    match res.status.as_u16() {
        429 => Error::TooMany,
        401 => Error::BadPassword,
        503 => Error::NoAdminKey,
        s => Error::Server(said.unwrap_or_else(|| format!("{s}"))),
    }
}

/// This computer's reporter ID (32 hex characters), kept in `reporter.json` in the app's folder: the same file as the
/// Electron app's, so its "My reports" carry over. Made the first time.
pub fn reporter_id(app_dir: &std::path::Path) -> String {
    let file = app_dir.join("reporter.json");
    let valid = |s: &str| s.len() == 32 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if let Some(id) = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
        .filter(|id| valid(id))
    {
        return id;
    }
    let id = random_hex32();
    let _ = std::fs::create_dir_all(app_dir).and_then(|_| std::fs::write(&file, json!({ "id": id }).to_string()));
    id
}

/// 32 random hex characters (from the standard library's randomly keyed hasher; no other library needed)
fn random_hex32() -> String {
    use std::hash::{BuildHasher, Hasher};
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    (0..2u8)
        .map(|i| {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u128(nanos);
            h.write_u8(i);
            h.write_u32(std::process::id());
            format!("{:016x}", h.finish())
        })
        .collect()
}

/// The sync server's feedback requests
#[derive(Clone)]
pub struct Client {
    http: Http,
    base: Url,
}

impl Client {
    /// `http`: any of the app's web requests (the server needs no sign-in cookies)
    pub fn new(http: Http) -> Self {
        Self {
            http,
            base: Url::parse(SERVER).expect("the server's address"),
        }
    }

    async fn call(&self, method: Method, path: &str, body: Option<Value>, admin_key: Option<&str>) -> Result<Response> {
        let url = workbench_net::join(&self.base, path)?;
        let mut req = Http::request(method, url).header("Accept", "application/json");
        if let Some(k) = admin_key {
            req = req.header("Authorization", format!("Bearer {k}"));
        }
        if let Some(b) = body {
            req = req.json(&b);
        }
        let res = self.http.send(req).await?;
        if res.ok() { Ok(res) } else { Err(failed(&res)) }
    }

    /// Sends one; answers its number
    pub async fn send(&self, report: &NewReport) -> Result<i64> {
        let mut r = report.clone();
        r.title = r.title.trim().chars().take(140).collect();
        r.body = r.body.trim().chars().take(6000).collect();
        let res = self
            .call(
                Method::POST,
                "/v1/reports",
                Some(serde_json::to_value(&r).unwrap_or_default()),
                None,
            )
            .await?;
        Ok(serde_json::from_str::<Value>(&res.body)
            .ok()
            .and_then(|v| v.get("id").and_then(Value::as_i64))
            .unwrap_or(0))
    }

    /// What this computer sent, with each one's status and reply
    pub async fn mine(&self, reporter: &str) -> Result<Vec<Report>> {
        parse_reports(
            &self
                .call(Method::GET, &format!("/v1/reports/mine?reporter={reporter}"), None, None)
                .await?
                .body,
        )
    }

    /// Everyone's (the admin)
    pub async fn all(&self, admin_key: &str) -> Result<Vec<Report>> {
        parse_reports(&self.call(Method::GET, "/v1/reports", None, Some(admin_key)).await?.body)
    }

    /// Changes a report's status, reply and/or the version it was fixed in (the admin)
    pub async fn update(&self, admin_key: &str, id: i64, change: Change) -> Result<()> {
        let Change { status, reply, fixed_in } = change;
        let mut body = serde_json::Map::new();
        if let Some(s) = status {
            body.insert("status".into(), serde_json::to_value(s).unwrap_or_default());
        }
        if let Some(r) = reply {
            body.insert("reply".into(), Value::String(r.chars().take(3000).collect()));
        }
        if let Some(v) = fixed_in {
            body.insert(
                "fixed_in".into(),
                Value::String(v.trim().trim_start_matches(['v', 'V']).chars().take(40).collect()),
            );
        }
        self.call(
            Method::PATCH,
            &format!("/v1/reports/{id}"),
            Some(Value::Object(body)),
            Some(admin_key),
        )
        .await
        .map(|_| ())
    }

    /// Deletes a report (the admin)
    pub async fn delete(&self, admin_key: &str, id: i64) -> Result<()> {
        self.call(Method::DELETE, &format!("/v1/reports/{id}"), None, Some(admin_key))
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_servers_lists() {
        let mine = r#"{"reports":[{"id":7,"created_at":1763190000000,"updated_at":1763190000000,"kind":"bug","title":"Chat jumps","status":"in-progress","reply":"On it"}]}"#;
        let list = parse_reports(mine).unwrap();
        assert_eq!(list[0].kind, Kind::Bug);
        assert_eq!(list[0].status, Status::InProgress);
        assert_eq!(list[0].reply.as_deref(), Some("On it"));
        assert_eq!(list[0].body, None);
        assert_eq!(list[0].fixed_in_label(), None); // an older server: no fixed_in
        let done = r#"{"reports":[{"id":9,"created_at":1,"kind":"bug","title":"x","status":"done","reply":"","fixed_in":"0.5.0"}]}"#;
        assert_eq!(parse_reports(done).unwrap()[0].fixed_in_label().as_deref(), Some("v0.5.0"));
        let all = r#"{"reports":[{"id":8,"created_at":1,"kind":"feature","title":"x","body":"y","name":"A","version":"0.4.5","platform":"win32 10","screen":"Xtime","status":"wont-do","reply":null}]}"#;
        let list = parse_reports(all).unwrap();
        assert_eq!(
            (list[0].kind, list[0].status, list[0].name.as_deref()),
            (Kind::Feature, Status::WontDo, Some("A"))
        );
        assert!(!list[0].status.is_open());
    }

    #[test]
    fn statuses_go_back_as_the_server_names_them() {
        assert_eq!(serde_json::to_value(Status::InProgress).unwrap(), json!("in-progress"));
        assert_eq!(serde_json::to_value(Status::WontDo).unwrap(), json!("wont-do"));
        assert_eq!(serde_json::to_value(Kind::Feature).unwrap(), json!("feature"));
    }

    #[test]
    fn keeps_one_reporter_id() {
        let dir = std::env::temp_dir().join(format!("wb-feedback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let id = reporter_id(&dir);
        assert_eq!(id.len(), 32);
        assert!(id.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
        assert_eq!(reporter_id(&dir), id); // the same next time
        std::fs::write(dir.join("reporter.json"), r#"{"id":"0123456789abcdef0123456789abcdef"}"#).unwrap();
        assert_eq!(reporter_id(&dir), "0123456789abcdef0123456789abcdef"); // the Electron app's, when there
        std::fs::remove_dir_all(&dir).ok();
    }
}
