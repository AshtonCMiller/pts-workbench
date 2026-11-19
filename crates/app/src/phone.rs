//! Your phone: paired with **Pair phone** (bottom right), it gets the notifications you haven't seen here.
//!
//! Pairing: the card shows a QR code of the phone page's address (`<sync server>/phone/`) with this profile's pairing
//! code after `#pair=` (crates/sync `Keys::pair_code`: the part after # never reaches the server). The phone keeps
//! it, and registers itself with the server under the push ID and token worked out from it
//! (sync-server/src/push.js). This side asks the server how many phones are paired, and sends to them.
//!
//! What's sent: a notification still unread here some minutes after it came (Settings: how many, or never), once.
//! Only what's in its title when that's an RO number and what happened ("RO 820400: line approved"); never the
//! vehicle, the lines, a customer, or who wrote in chat and what.

use serde_json::{Value, json};
use workbench_net::{Http, Method, Url};
use workbench_sync::Keys;

use crate::notes::{Kind, Note, Notes};

/// Sent to the phone only while it's fresh (older ones unread from before pairing aren't)
const FRESH_MS: i64 = 2 * 3600 * 1000;
/// The minutes to wait, unless Settings says otherwise
pub const DEFAULT_AFTER: u32 = 5;
/// Choices for the wait (0: never)
pub const AFTER_CHOICES: [u32; 5] = [2, 5, 10, 15, 0];

/// A paired phone, as the server knows it
#[derive(Debug, Clone, PartialEq)]
pub struct PhoneInfo {
    /// "iPhone", "Android phone"
    pub name: String,
    /// When it was paired, and last opened (ms)
    pub since: i64,
    pub seen: i64,
}

/// What's known about this profile's phones
#[derive(Debug, Clone, Default)]
pub struct PhoneState {
    /// The paired phones (none: not asked yet)
    pub phones: Option<Vec<PhoneInfo>>,
    pub checking: bool,
    /// Why the server couldn't be asked
    pub error: Option<String>,
    /// A test being sent, and how the last one went
    pub testing: bool,
    pub tested: Option<Result<usize, String>>,
    /// Unpair pressed once (pressed again: done)
    pub unpair_asked: bool,
    /// When the server was last asked (s), and the QR code while the card's up (none: nothing to pair with)
    pub checked_at: Option<i64>,
    pub qr: Option<ui_kit::qr::Qr>,
}

impl PhoneState {
    pub fn paired(&self) -> bool {
        self.phones.as_ref().is_some_and(|p| !p.is_empty())
    }
}

/// The phone page's address, with the pairing code (what the QR code holds)
pub fn pair_link(keys: &Keys) -> String {
    format!("{}/phone/#pair={}", workbench_sync::URL.trim_end_matches('/'), keys.pair_code())
}

async fn call(http: &Http, path: &str, body: Value) -> Result<Value, String> {
    let url = Url::parse(&format!("{}{path}", workbench_sync::URL.trim_end_matches('/'))).map_err(|e| e.to_string())?;
    let req = Http::request(Method::POST, url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .body(body.to_string());
    let res = http.send(req).await.map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&res.body).unwrap_or(Value::Null);
    if res.ok() {
        Ok(v)
    } else {
        Err(v
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("the server said {}", res.status.as_u16())))
    }
}

fn ids(keys: &Keys) -> Value {
    let (pid, token) = keys.push_ids();
    json!({ "pid": pid, "token": token })
}

/// The phones paired with this profile
pub async fn status(http: Http, keys: Keys) -> Result<Vec<PhoneInfo>, String> {
    let v = call(&http, "/v1/push/status", ids(&keys)).await?;
    Ok(v.get("phones")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|p| PhoneInfo {
                    name: p.get("name").and_then(Value::as_str).unwrap_or("Phone").to_string(),
                    since: p.get("since").and_then(Value::as_i64).unwrap_or(0),
                    seen: p.get("seen").and_then(Value::as_i64).unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Sends a notification to every paired phone; how many got it
pub async fn send(http: Http, keys: Keys, title: String, body: String, tag: String) -> Result<usize, String> {
    let mut b = ids(&keys);
    b["title"] = json!(title);
    b["body"] = json!(body);
    b["tag"] = json!(tag);
    let v = call(&http, "/v1/push/send", b).await?;
    Ok(v.get("sent").and_then(Value::as_u64).unwrap_or(0) as usize)
}

/// Unpairs every phone
pub async fn unpair(http: Http, keys: Keys) -> Result<(), String> {
    call(&http, "/v1/push/unpair", ids(&keys)).await.map(|_| ())
}

// ---------- what goes to the phone ----------

/// The notifications to send now: unread, not sent yet, `after_min` minutes old (but not older than two hours)
pub fn due(notes: &Notes, now_ms: i64, after_min: u32) -> Vec<u64> {
    if after_min == 0 {
        return Vec::new();
    }
    let wait = after_min as i64 * 60_000;
    notes
        .list
        .iter()
        .filter(|n| !n.read && !n.phoned && now_ms - n.at >= wait && now_ms - n.at < FRESH_MS)
        .map(|n| n.id)
        .collect()
}

/// One notification's words for the phone: what happened, never who or what's on the RO
fn line(n: &Note) -> String {
    match n.kind {
        Kind::Message if n.ro.is_empty() => "New chat message".to_string(),
        Kind::Message => format!("New chat message on RO {}", n.ro),
        // (these titles are only the RO number and what happened: "RO 820400 sent to Approval")
        _ => n.title.clone(),
    }
}

/// The phone's notification for these (title, body, tag): one says what it is, several how many
pub fn message(notes: &[&Note]) -> (String, String, String) {
    match notes {
        [one] => (
            line(one),
            "Still waiting on you in PTS Workbench.".to_string(),
            if one.ro.is_empty() {
                "pts-chat".to_string()
            } else {
                format!("ro-{}", one.ro)
            },
        ),
        many => {
            let mut lines: Vec<String> = many.iter().take(3).map(|n| line(n)).collect();
            if many.len() > 3 {
                lines.push(format!("and {} more", many.len() - 3));
            }
            (
                format!("{} things are waiting on you", many.len()),
                lines.join(" · "),
                "pts-waiting".to_string(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::New;

    fn notes() -> Notes {
        let mut n = Notes::default();
        let news = |kind, title: &str, body: &str, ro: &str| New {
            kind,
            title: title.into(),
            body: body.into(),
            ro: ro.into(),
            channel: String::new(),
        };
        n.add(
            vec![news(
                Kind::Approved,
                "RO 820400: line approved",
                "Brakes · 2022 Ford F-150",
                "820400",
            )],
            0,
        );
        n.add(
            vec![news(
                Kind::Message,
                "New message from Sam on RO 820400",
                "Call Mrs. Smith back",
                "820400",
            )],
            60_000,
        );
        n.add(vec![news(Kind::Stage, "RO 820411 sent to Review", "", "820411")], 9 * 60_000);
        n
    }

    #[test]
    fn waits_then_sends_once() {
        let mut n = notes();
        assert_eq!(due(&n, 4 * 60_000, 5).len(), 0);
        let d = due(&n, 6 * 60_000, 5);
        assert_eq!(d.len(), 2);
        assert!(due(&n, 6 * 60_000, 0).is_empty());
        for id in &d {
            n.mark_phoned(*id);
        }
        assert!(due(&n, 7 * 60_000, 5).is_empty());
        // read here first: never sent
        let last = n.list[0].id;
        n.mark_read(last);
        assert!(due(&n, 20 * 60_000, 5).is_empty());
        // too old by now
        assert!(due(&notes(), 3 * 3600 * 1000, 5).is_empty());
    }

    #[test]
    fn says_nothing_private() {
        let n = notes();
        let (t, b, tag) = message(&[n.list.iter().find(|x| x.kind == Kind::Approved).unwrap()]);
        assert_eq!((t.as_str(), tag.as_str()), ("RO 820400: line approved", "ro-820400"));
        assert!(!b.contains("F-150") && !b.contains("Brakes"));
        let all: Vec<&Note> = n.list.iter().collect();
        let (t, b, _) = message(&all);
        assert_eq!(t, "3 things are waiting on you");
        assert!(b.contains("New chat message on RO 820400"));
        assert!(!b.contains("Sam") && !b.contains("Smith"));
    }
}