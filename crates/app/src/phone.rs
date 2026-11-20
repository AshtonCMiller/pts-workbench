//! Your phone: paired with **Pair phone** (bottom right), it gets the notifications you haven't seen here.
//!
//! Pairing: the card shows a QR code of the phone page's address (`<sync server>/phone/`) with this profile's pairing
//! code after `#pair=` (crates/sync `Keys::pair_code`: the part after # never reaches the server). The phone keeps
//! it, and registers itself with the server under the push ID and token worked out from it
//! (sync-server/src/push.js). This side asks the server how many phones are paired, and sends to them.
//!
//! Your RO list goes to the phone too (`list_item`, sent by the app: crates/sync `Keys::ros`): each RO's number, tag,
//! vehicle and where it is, as the sidebar has them; the phone reads your hours from what sync already carries.
//!
//! What's sent: a notification still unread here some minutes after it came (its card: how many, or never), once.
//! The RO and what happened ("RO 820400: 2 lines approved"), the lines approved or declined, and a chat message's own
//! words; not the vehicle, the tag, or who wrote. VINs are hidden. It's encrypted for the phone itself: the sync
//! server and Apple's / Google's push services pass it on without being able to read it, and nothing is kept.

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
    /// Your RO list as last sent for the phone (its text), when, and whether it's being sent
    pub list_sent: Option<String>,
    pub list_sent_at: Option<i64>,
    pub list_sending: bool,
}

/// How often the RO list is sent again when nothing changed (so the phone can say how fresh it is), in seconds
pub const LIST_EVERY: i64 = 15 * 60;

/// Xtime's colour for a pill, as the phone shows it ("#15803d")
fn hex_color(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// One RO of your list, for the phone: its number, tag, vehicle and where it is (no VIN, no customer)
pub fn list_item(r: &xtime::dashboard::AssignedRo, mine: bool, open: bool) -> Value {
    let ymm = [r.year.as_str(), r.make.as_str(), r.model.as_str()]
        .iter()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim())
        .collect::<Vec<_>>()
        .join(" ");
    let tags: Vec<Value> = xtime_ui::my_ros::status_tags(r)
        .into_iter()
        .map(|(label, color)| json!([label, hex_color(color)]))
        .collect();
    json!({ "ro": r.ro, "tag": r.tag, "ymm": ymm, "mine": mine, "open": open, "tags": tags })
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

/// Each one on its own up to this many at once; more than that, one that says how many
const SEPARATE: usize = 4;

/// VINs in what's sent are hidden ("1FTFW1E85MFA00000" → "[VIN]"): a 17-character run of letters and digits (no I, O
/// or Q) with digits in it
pub fn hide_vins(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        let vin = word.len() == 17
            && word
                .chars()
                .all(|c| c.is_ascii_alphanumeric() && !matches!(c.to_ascii_uppercase(), 'I' | 'O' | 'Q'))
            && word.chars().filter(char::is_ascii_digit).count() >= 4
            && word.chars().any(|c| c.is_ascii_alphabetic());
        out.push_str(if vin { "[VIN]" } else { word.as_str() });
        word.clear();
    };
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// One notification for the phone (title, body, tag): the RO and what happened; the lines approved or declined; a
/// chat message's own words. (Not the vehicle, the tag or who wrote.)
pub fn phone_note(n: &Note) -> (String, String, String) {
    let tag = if n.ro.is_empty() {
        format!("note-{}", n.id)
    } else {
        format!("ro-{}-{}", n.ro, n.id)
    };
    let (title, body) = match n.kind {
        // "RO 820400: 2 lines approved" / "Brakes; Front tires"
        Kind::Approved | Kind::Declined => (n.title.clone(), n.body.clone()),
        Kind::Message => {
            let mentioned = n.title.contains(" mentioned you");
            let title = match (n.ro.is_empty(), mentioned) {
                (true, false) => "New chat message".to_string(),
                (true, true) => "You were mentioned in chat".to_string(),
                (false, false) => format!("Chat on RO {}", n.ro),
                (false, true) => format!("You were mentioned on RO {}", n.ro),
            };
            (title, n.body.clone())
        }
        // the rest: the RO and what happened ("RO 820400 sent to Review")
        _ => (n.title.clone(), "Still waiting on you in PTS Workbench.".to_string()),
    };
    (hide_vins(&title), hide_vins(&body), tag)
}

/// What to send for these: each one on its own, or (many at once) one that says how many
pub fn messages(notes: &[&Note]) -> Vec<(String, String, String)> {
    if notes.len() <= SEPARATE {
        return notes.iter().map(|n| phone_note(n)).collect();
    }
    let ros: Vec<String> = {
        let mut r: Vec<String> = Vec::new();
        for n in notes {
            if !n.ro.is_empty() && !r.contains(&n.ro) {
                r.push(n.ro.clone());
            }
        }
        r
    };
    let body = if ros.is_empty() {
        "Open PTS Workbench to see them.".to_string()
    } else if ros.len() <= 4 {
        format!("On RO {}", ros.join(", "))
    } else {
        format!("On RO {} and {} more", ros[..3].join(", "), ros.len() - 3)
    };
    vec![(
        format!("{} things are waiting on you", notes.len()),
        body,
        "pts-waiting".to_string(),
    )]
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
                "RO 820400: 2 lines approved",
                "Front brakes; Cabin filter",
                "820400",
            )],
            0,
        );
        n.add(
            vec![news(
                Kind::Message,
                "New message from Sam on RO 820400",
                "Customer says it's 1FTFW1E85MFA12345, check the recall",
                "820400",
            )],
            60_000,
        );
        n.add(
            vec![news(Kind::Stage, "RO 820411 sent to Review", "2022 Ford F-150 · Tag 51", "820411")],
            9 * 60_000,
        );
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
    fn says_what_happened() {
        let n = notes();
        let get = |k: Kind| n.list.iter().find(|x| x.kind == k).unwrap();
        let (t, b, _) = phone_note(get(Kind::Approved));
        assert_eq!(
            (t.as_str(), b.as_str()),
            ("RO 820400: 2 lines approved", "Front brakes; Cabin filter")
        );
        let (t, b, tag) = phone_note(get(Kind::Message));
        assert_eq!(t, "Chat on RO 820400");
        assert_eq!(b, "Customer says it's [VIN], check the recall");
        assert!(tag.starts_with("ro-820400-"));
        let (t, b, _) = phone_note(get(Kind::Stage));
        assert_eq!(t, "RO 820411 sent to Review");
        assert!(!b.contains("F-150") && !b.contains("Tag"));
        let all: Vec<&Note> = n.list.iter().collect();
        assert_eq!(messages(&all).len(), 3);
        let many: Vec<&Note> = all.iter().cycle().take(6).copied().collect();
        let m = messages(&many);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].0, "6 things are waiting on you");
        assert_eq!(m[0].1, "On RO 820411, 820400");
        assert_eq!(hide_vins("RO 820400 and 12345678901234567"), "RO 820400 and 12345678901234567");
    }
}
