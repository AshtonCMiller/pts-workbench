//! Xtime's Dealership chat, with the same requests Xtime's Dealership message center makes (the Electron app's
//! `chat.js`): your conversations, one conversation's messages, the unread count, and the people you can @mention.
//! Customer chats are left out. Sending a message and marking messages read go over Xtime's chat server ([`xmpp`]).
//!
//! Chat signs in with its own token, asked of Xtime's login service with your Xtime sign-in (as Xtime's page does). Its
//! settings (your dealer code and Xtime username) come from that token, or from the Electron app's saved copy
//! ([`ChatConfig`], `xtime-chat.json`). Messages are only kept in memory.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use workbench_net::Method;

use crate::xmpp::{self, Refused};
use crate::{Error, Result, Xtime, paths, xml};

/// Xtime's chat settings: your dealer code and Xtime username (the Electron app's `xtime-chat.json`, same fields)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChatConfig {
    pub dealer_code: String,
    pub user_name: String,
    /// "" for Xtime's live site
    pub env: String,
    /// The chat gateway's address ("" for the usual one)
    pub chatgateway_url: String,
    pub store_id: String,
}

impl ChatConfig {
    fn env_part(&self) -> &str {
        match self.env.as_str() {
            "" | "prod" | "production" => "",
            e => e,
        }
    }
    fn gateway(&self) -> String {
        let base = if self.chatgateway_url.is_empty() {
            format!("https://chatgateway{}.xtime.com", self.env_part())
        } else {
            self.chatgateway_url.clone()
        };
        format!("{}/", base.trim_end_matches('/'))
    }
}

/// One chat message, as the app shows it
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ChatMessage {
    pub id: String,
    /// The conversation it's in
    pub channel_id: String,
    /// The RO it's about ("" when none)
    pub ro: String,
    /// Who sent it (their name, from the store's staff list when it's there)
    pub sender: String,
    /// You sent it
    pub mine: bool,
    /// The text, with @mentions and links made plain ("@Brenton Skeeters")
    pub text: String,
    /// The names @mentioned in it
    pub mentions: Vec<String>,
    /// When (milliseconds since 1970)
    pub time: Option<i64>,
    /// Someone else sent it and you haven't read it
    pub unread: bool,
    /// Xtime's own notice (not a person's message)
    pub notification: bool,
}

/// Someone you can @mention: Xtime's id for them (`jid|name\40s3590`), and their name
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChatPerson {
    pub id: String,
    pub name: String,
}

/// What chat keeps between requests (in memory only)
#[derive(Debug, Default)]
pub(crate) struct ChatMem {
    pub token: Option<String>,
    pub config: Option<ChatConfig>,
    /// Where Xtime's chat server is (its WebSocket address), once asked
    pub server: Option<String>,
}

// ---------- names and text ----------

/// Strophe.escapeNode, as Xtime uses for usernames
pub fn escape_node(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        match c {
            '\\' => out.push_str("\\5c"),
            ' ' => out.push_str("\\20"),
            '"' => out.push_str("\\22"),
            '&' => out.push_str("\\26"),
            '\'' => out.push_str("\\27"),
            '/' => out.push_str("\\2f"),
            ':' => out.push_str("\\3a"),
            '<' => out.push_str("\\3c"),
            '>' => out.push_str("\\3e"),
            '@' => out.push_str("\\40"),
            c => out.push(c),
        }
    }
    out
}

pub fn unescape_node(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('\\') {
        out.push_str(&rest[..i]);
        let code = rest.get(i + 1..i + 3).map(str::to_ascii_lowercase);
        match code.as_deref() {
            Some(c @ ("5c" | "20" | "22" | "26" | "27" | "2f" | "3a" | "3c" | "3e" | "40")) => {
                out.push(u8::from_str_radix(c, 16).unwrap_or(b'?') as char);
                rest = &rest[i + 3..];
            }
            _ => {
                out.push('\\');
                rest = &rest[i + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// "bskeeters\40s3590@chat.xtime.com/res" → "bskeeters@s3590"
pub fn user_of(jid: &str) -> String {
    let no_res = jid.split('/').next().unwrap_or("");
    let local = no_res.rsplit_once('@').map(|(l, _)| l).unwrap_or(no_res);
    unescape_node(local)
}

/// A username compared bare: "amiller@s3590", "tbencsek_xtime" and "aking__S3590_xtime" → "amiller", "tbencsek", "aking"
pub fn bare_user(u: &str) -> String {
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
    let mut u = u.trim().to_lowercase();
    if let Some((head, _)) = u.rsplit_once("@s").filter(|(_, n)| digits(n)) {
        u = head.to_string();
    }
    if let Some((head, _)) = u
        .strip_suffix("_xtime")
        .and_then(|h| h.rsplit_once("__s"))
        .filter(|(_, n)| digits(n))
    {
        u = head.to_string();
    }
    u.strip_suffix("_xtime").map(str::to_string).unwrap_or(u)
}

/// Real names from the store's staff list, by bare username (usernames first; an email's name only when no username
/// has it)
pub fn staff_names(staff: &[Value]) -> HashMap<String, String> {
    let s = |e: &Value, k: &str| e.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let full = |e: &Value| {
        [s(e, "FirstName"), s(e, "LastName")]
            .into_iter()
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let active = |e: &Value| e.get("Active") != Some(&Value::Bool(false));
    let mut names = HashMap::new();
    for e in staff {
        let f = full(e);
        if f.is_empty() {
            continue;
        }
        for k in ["Username", "XtimeUsername"] {
            let u = s(e, k);
            if !u.is_empty() && (!names.contains_key(&bare_user(&u)) || active(e)) {
                names.insert(bare_user(&u), f.clone());
            }
        }
    }
    for e in staff {
        let f = full(e);
        let k = bare_user(s(e, "Email").split('@').next().unwrap_or(""));
        if !k.is_empty() && !f.is_empty() {
            names.entry(k).or_insert(f);
        }
    }
    names
}

/// A person's name: from the staff list, or made from their username ("john.smith" → "John Smith")
pub fn pretty_name(names: &HashMap<String, String>, user: &str) -> String {
    let bare = bare_user(user);
    if let Some(n) = names.get(&bare) {
        return n.clone();
    }
    bare.split(['.', '_', ' ', '\t'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Xtime's message text made plain: @mentions ("[Brenton\20Skeeters](jid|bskeeters\40s3590)") become "@Brenton
/// Skeeters", links their caption or address, markdown escapes and entities are undone. Returns the text and the
/// names mentioned.
pub fn plain(t: &str, names: &HashMap<String, String>) -> (String, Vec<String>) {
    let mut mentions = Vec::new();
    // @mentions: [shown](jid|user) or [shown](jid_group|group), with or without an @ before
    let text = links(t, |out, shown, target| {
        let (kind, who) = target.split_once('|')?;
        if kind != "jid" && kind != "jid_group" {
            return None;
        }
        if out.ends_with('@') {
            out.pop();
        }
        let shown = unescape_node(shown.trim_start_matches('@')).trim().to_string();
        let n = if shown.is_empty() {
            pretty_name(names, &user_of(who))
        } else {
            shown
        };
        mentions.push(n.clone());
        Some(format!("@{n}"))
    });
    // other links: their caption, or the address
    let text = links(&text, |_, shown, target| {
        if target.is_empty() {
            None
        } else if !shown.is_empty() && !shown.to_lowercase().starts_with("http") {
            Some(shown.to_string())
        } else {
            Some(target.rsplit('|').next().unwrap_or("").to_string())
        }
    });
    // markdown's escapes ("\*" → "*")
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match chars.peek() {
            Some(&n) if c == '\\' && "\\`*_{}[]()#+-.!>~|".contains(n) => {
                plain.push(n);
                chars.next();
            }
            _ => plain.push(c),
        }
    }
    (xml::decode(&plain).trim().to_string(), mentions)
}

/// Each markdown link "[shown](target)" in `t` given to `f` (with the text so far), which gives what to put in its
/// place, or none to leave it as it is
fn links(t: &str, mut f: impl FnMut(&mut String, &str, &str) -> Option<String>) -> String {
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    while let Some(i) = rest.find('[') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let found = after.find(']').and_then(|close| {
            let shown = &after[..close];
            let tail = after[close + 1..].strip_prefix('(')?;
            let end = tail.find(')')?;
            Some((shown, &tail[..end], close + 1 + 1 + end + 1))
        });
        match found {
            Some((shown, target, len)) => {
                let mut tried = out.clone();
                match f(&mut tried, shown, target) {
                    Some(r) => {
                        out = tried;
                        out.push_str(&r);
                        rest = &after[len..];
                    }
                    None => {
                        out.push('[');
                        rest = after;
                    }
                }
            }
            None => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The RO in a conversation's id ("3590-820367-1" → "820367")
pub fn ro_from_channel(id: &str) -> String {
    let b = id.as_bytes();
    let run = |from: usize| b[from..].iter().take_while(|c| c.is_ascii_digit()).count();
    for start in 0..b.len() {
        if !b[start].is_ascii_digit() || (start > 0 && b[start - 1].is_ascii_digit()) {
            continue;
        }
        let a = run(start);
        let m = start + a;
        if b.get(m) != Some(&b'-') {
            continue;
        }
        let r = run(m + 1);
        let e = m + 1 + r;
        if r > 0 && b.get(e) == Some(&b'-') && run(e + 1) > 0 {
            return id[m + 1..e].to_string();
        }
    }
    String::new()
}

/// Xtime's times: milliseconds, or "2026-11-15 14:05:03(.123)" (UTC unless it says otherwise)
pub fn parse_time(v: &Value) -> Option<i64> {
    let s = match v {
        Value::Number(n) => return n.as_i64(),
        Value::String(s) => s.trim(),
        _ => return None,
    };
    if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
        return s.parse().ok();
    }
    let n = |a: usize, b: usize| s.get(a..b).and_then(|x| x.parse::<i64>().ok());
    let (y, mo, d, h, mi, sec) = (
        n(0, 4)?,
        n(5, 7)?,
        n(8, 10)?,
        n(11, 13).unwrap_or(0),
        n(14, 16).unwrap_or(0),
        n(17, 19).unwrap_or(0),
    );
    // fraction and zone
    let rest = s.get(19..).unwrap_or("");
    let (frac, zone) = match rest.strip_prefix('.') {
        Some(r) => {
            let digits: String = r.chars().take_while(|c| c.is_ascii_digit()).collect();
            let ms = format!("{digits:0<3}")[..3].parse::<i64>().unwrap_or(0);
            (ms, &r[digits.len()..])
        }
        None => (0, rest),
    };
    let offset_min = match zone.chars().next() {
        Some('+') | Some('-') => {
            let sign = if zone.starts_with('-') { -1 } else { 1 };
            let z: String = zone[1..].chars().filter(|c| c.is_ascii_digit()).collect();
            let hh = z.get(0..2).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
            let mm = z.get(2..4).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
            sign * (hh * 60 + mm)
        }
        _ => 0,
    };
    // days since 1970 (Howard Hinnant's days-from-civil)
    let (yy, m) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    Some(((days * 24 + h) * 60 + mi - offset_min) * 60_000 + sec * 1000 + frac)
}

/// The list in one of chat's answers (it comes bare, or under content / messages / data…)
pub fn array_in(d: &Value) -> Vec<Value> {
    match d {
        Value::Array(a) => a.clone(),
        Value::Object(m) => ["content", "messages", "data", "results", "items", "history"]
            .iter()
            .find_map(|k| m.get(*k).and_then(Value::as_array).cloned())
            .or_else(|| m.values().find_map(|v| v.as_array().cloned()))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// One record from chat's history as a message. Records hold the XMPP message as text (chatmsg). `me`: your Xtime
/// username.
pub fn read_message(raw: &Value, me: &str, names: &HashMap<String, String>) -> ChatMessage {
    let s = |k: &str| match raw.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    let (mut from, mut body, mut ro, mut notification) = (String::new(), String::new(), String::new(), false);
    if let Some(doc) = raw.get("chatmsg").and_then(Value::as_str).and_then(xml::parse) {
        from = doc.attr("from").unwrap_or("").to_string();
        body = doc.child("body").map(|b| b.text()).unwrap_or_default();
        for c in doc.descendants("xtctx") {
            let kid = |n: &str| c.child(n).map(|x| x.text()).unwrap_or_default();
            let ns = c.attr("namespace").map(str::to_string).unwrap_or_else(|| kid("namespace"));
            let id = c.attr("contextId").map(str::to_string).unwrap_or_else(|| kid("contextId"));
            if ns == "RO" && !id.is_empty() {
                ro = id;
            }
        }
        notification = !doc.descendants("notification").is_empty();
    }
    // some bodies are JSON ({ body, attachments })
    if body.trim_start().starts_with('{') {
        if let Some(b) = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|j| j.get("body").and_then(Value::as_str).map(str::to_string))
        {
            body = b;
        }
    }
    let sender_user = user_of(&from);
    let mine = s("dir") == "send" || (!sender_user.is_empty() && bare_user(&sender_user) == bare_user(me));
    let status = ["status", "state", "readStatus"]
        .iter()
        .map(|k| s(k))
        .find(|v| !v.is_empty())
        .unwrap_or_default();
    let (text, mentions) = plain(&body, names);
    let channel_id = s("channelId");
    let sender = if sender_user.is_empty() {
        if notification { "Xtime".to_string() } else { String::new() }
    } else {
        pretty_name(names, &sender_user)
    };
    ChatMessage {
        id: [s("archiveId"), s("id")].into_iter().find(|v| !v.is_empty()).unwrap_or_default(),
        ro: if ro.is_empty() { ro_from_channel(&channel_id) } else { ro },
        channel_id,
        sender,
        mine,
        text,
        mentions,
        time: raw.get("createdAt").and_then(parse_time),
        unread: !mine && !matches!(status.to_lowercase().as_str(), "read" | "seen"),
        notification,
    }
}

/// One entry per conversation: its newest message (unread when any of its messages is), newest first
pub fn conversations(list: &[ChatMessage]) -> Vec<ChatMessage> {
    let mut by: Vec<ChatMessage> = Vec::new();
    for m in list {
        let key = |x: &ChatMessage| {
            if x.channel_id.is_empty() {
                format!("ro:{}", x.ro)
            } else {
                x.channel_id.clone()
            }
        };
        match by.iter_mut().find(|c| key(c) == key(m)) {
            Some(cur) => {
                let unread = cur.unread || m.unread;
                if m.time.unwrap_or(0) > cur.time.unwrap_or(0) {
                    *cur = m.clone();
                }
                cur.unread = unread;
            }
            None => by.push(m.clone()),
        }
    }
    by.sort_by(|a, b| b.time.unwrap_or(0).cmp(&a.time.unwrap_or(0)));
    by
}

/// A token's claims (the middle part of a JWT)
pub fn jwt_claims(token: &str) -> Option<Value> {
    let part = token.split('.').nth(1)?;
    serde_json::from_slice(&b64_decode(part)?).ok()
}

/// Base64, standard or URL-safe, with or without padding
pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::new();
    let (mut buf, mut bits) = (0u32, 0);
    for &c in s.trim().as_bytes().iter().filter(|c| **c != b'=') {
        buf = (buf << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// The settings from what the chat token says about you (dealer code and username); none when it doesn't say
pub fn config_from_claims(claims: &Value, store_id: &str) -> Option<ChatConfig> {
    let map = claims.as_object()?;
    let pick = |names: &[&str]| {
        names.iter().find_map(|n| {
            map.iter()
                .find(|(k, v)| k.eq_ignore_ascii_case(n) && !v.is_null() && v.as_str() != Some(""))
                .map(|(_, v)| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))
        })
    };
    let dealer = pick(&["dealerCode", "dealer_code", "dealer", "dlrCode"])?;
    let user = pick(&["userName", "username", "preferred_username", "user", "sub"])?;
    if user.chars().all(|c| c.is_ascii_digit()) {
        return None; // a number, not a username
    }
    Some(ChatConfig {
        dealer_code: dealer,
        user_name: user,
        store_id: store_id.to_string(),
        ..Default::default()
    })
}

/// "@Brenton Skeeters" → Xtime's mention markup ("[Brenton Skeeters](jid|bskeeters\40s3590)"), for the people picked
/// from the list (`picked`: name, id). Longer names first, so "@Al Smithson" isn't taken for "@Al Smith".
pub fn to_markup(text: &str, picked: &[(String, String)]) -> String {
    let mut by_length: Vec<&(String, String)> = picked.iter().collect();
    by_length.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    // each name swapped for a placeholder first, so a shorter name can't match inside a longer one's markup
    let mut out = text.to_string();
    for (i, (name, _)) in by_length.iter().enumerate() {
        out = out.replace(&format!("@{name}"), &format!("\u{1}{i}\u{2}"));
    }
    for (i, (name, id)) in by_length.iter().enumerate() {
        out = out.replace(&format!("\u{1}{i}\u{2}"), &format!("[{name}]({id})"));
    }
    out
}

/// The context a new message carries, as Xtime's page sends it: the conversation's latest message's (when it's for
/// this RO), else the RO and dealer code; with a new `sent_msg_uuid`. `records`: the conversation's latest records.
pub fn send_context(records: &[Value], ro: &str, dealer: &str, sent_uuid: &str) -> Vec<(String, String)> {
    let key = |r: &Value| {
        let at = match r.get("createdAt") {
            Some(Value::String(s)) => s.clone(),
            Some(v @ Value::Number(_)) => v.to_string(),
            _ => String::new(),
        };
        let id = r
            .get("archiveId")
            .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
            .unwrap_or(0);
        (at, id)
    };
    let mut recs: Vec<&Value> = records
        .iter()
        .filter(|r| r.get("chatmsg").and_then(Value::as_str).is_some())
        .collect();
    recs.sort_by_key(|r| key(r));
    let latest: Vec<(String, String)> = recs
        .last()
        .and_then(|r| xml::parse(r.get("chatmsg")?.as_str()?))
        .map(|doc| {
            doc.descendants("xtctx")
                .iter()
                .map(|c| {
                    let kid = |n: &str| c.child(n).map(|x| x.text()).unwrap_or_default();
                    let ns = c.attr("namespace").map(str::to_string).unwrap_or_else(|| kid("namespace"));
                    let id = c.attr("contextId").map(str::to_string).unwrap_or_else(|| kid("contextId"));
                    (ns, id)
                })
                .filter(|(ns, _)| !ns.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let fits = latest.iter().any(|(ns, id)| ns == "RO" && (ro.is_empty() || id == ro));
    let mut ctx = if fits {
        latest
    } else {
        vec![("RO".to_string(), ro.to_string()), ("DEALER_CODE".to_string(), dealer.to_string())]
    };
    ctx.retain(|(ns, _)| !matches!(ns.as_str(), "CHAT_MSG_ID" | "src_msgid" | "sent_msg_uuid"));
    ctx.push(("sent_msg_uuid".to_string(), sent_uuid.to_string()));
    ctx
}

/// The message ids in Xtime's list of unread ones: [101, 102] · [{ archiveId: 101, … }] · { channel: [101, …] } ·
/// { data: … }. Only numbers; each once.
pub fn unread_ids(d: &Value) -> Vec<String> {
    fn walk(d: &Value, out: &mut Vec<String>) {
        match d {
            Value::Array(a) => {
                for x in a {
                    match x {
                        Value::Object(o) => {
                            match ["archiveId", "id", "messageId", "archive_id"]
                                .iter()
                                .find_map(|k| o.get(*k).filter(|v| !v.is_null()))
                            {
                                Some(id) => walk(id, out),
                                None => walk(x, out),
                            }
                        }
                        other => walk(other, out),
                    }
                }
            }
            Value::Object(o) => o.values().for_each(|v| walk(v, out)),
            Value::Number(n) => out.push(n.to_string()),
            Value::String(s) => out.push(s.clone()),
            _ => {}
        }
    }
    let mut all = Vec::new();
    walk(d, &mut all);
    let mut out: Vec<String> = Vec::new();
    for id in all {
        if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) && !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// What the app says it is to Xtime's chat server (a desktop browser, as Xtime's page is)
const CHAT_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

// ---------- asking Xtime ----------

impl Xtime {
    /// Chat settings saved earlier (the Electron app's `xtime-chat.json`, or from a token); used before asking
    pub fn set_chat_config(&self, c: ChatConfig) {
        self.chat.lock().expect("chat lock").config = Some(c);
    }

    /// The chat settings in use (to keep for next time)
    pub fn chat_config(&self) -> Option<ChatConfig> {
        self.chat.lock().expect("chat lock").config.clone()
    }

    fn env_part(&self) -> String {
        self.chat_config().map(|c| c.env_part().to_string()).unwrap_or_default()
    }

    /// The chat token: kept while it's good; otherwise asked of Xtime's login service, as Xtime's page does
    async fn chat_token(&self, fresh: bool) -> Result<String> {
        if !fresh {
            let kept = self.chat.lock().expect("chat lock").token.clone();
            if let Some(t) = kept.filter(|t| !token_expired(t)) {
                return Ok(t);
            }
        }
        let url = format!("https://login{}.xtime.app.coxautoinc.com/rest/login/jwt/xsession", self.env_part());
        let req = chat_request(Method::POST, &url)?
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body("null");
        let res = self.send(req).await?;
        // a run-out sign-in: turned away, or sent to sign in
        let to_signin = res.url.host_str().is_some_and(|h| h.contains("signin.coxautoinc.com"))
            || (res.url.path().to_lowercase().contains("/login") && !res.url.path().contains("/rest/login/"));
        if matches!(res.status.as_u16(), 401 | 403) || to_signin {
            return Err(Error::SessionExpired);
        }
        if !res.ok() {
            return Err(Error::Status(res.status.as_u16()));
        }
        let j: Value = serde_json::from_str(&res.body).map_err(|_| Error::SessionExpired)?;
        let t = j
            .pointer("/data/accessToken")
            .or_else(|| j.pointer("/content/data/accessToken"))
            .or_else(|| j.get("accessToken"))
            .and_then(Value::as_str)
            .ok_or(Error::SessionExpired)?
            .to_string();
        self.chat.lock().expect("chat lock").token = Some(t.clone());
        Ok(t)
    }

    /// The token and the settings (from the token's claims when none were saved)
    async fn chat_setup(&self, store: &str) -> Result<(String, ChatConfig)> {
        let token = self.chat_token(false).await?;
        if let Some(c) = self.chat_config().filter(|c| !c.dealer_code.is_empty() && !c.user_name.is_empty()) {
            return Ok((token, c));
        }
        let c = jwt_claims(&token)
            .and_then(|cl| config_from_claims(&cl, store.trim()))
            .ok_or_else(|| Error::Format("Xtime's chat settings aren't known yet (open Xtime chat once in the old app)".into()))?;
        self.set_chat_config(c.clone());
        Ok((token, c))
    }

    /// A request to the chat gateway (POST with a JSON body, or GET); a 401 gets a new token once
    async fn chat_call(&self, store: &str, path_query: &str, body: Option<Value>) -> Result<Value> {
        let (mut token, cfg) = self.chat_setup(store).await?;
        let url = cfg.gateway() + path_query;
        for attempt in 0..2 {
            let method = if body.is_some() { Method::POST } else { Method::GET };
            let mut req = chat_request(method, &url)?
                .header("Authorization", format!("Bearer {token}"))
                .header("Accept", "application/json");
            if let Some(b) = &body {
                req = req.header("Content-Type", "application/json").body(b.to_string());
            }
            let res = self.send(req).await?;
            if res.status.as_u16() == 401 {
                if attempt == 0 {
                    token = self.chat_token(true).await?;
                    continue;
                }
                return Err(Error::SessionExpired);
            }
            if !res.ok() {
                return Err(Error::Status(res.status.as_u16()));
            }
            return serde_json::from_str(&res.body).map_err(|_| Error::Format("Chat didn't send readable data".into()));
        }
        Err(Error::SessionExpired)
    }

    async fn chat_names(&self, store: &str) -> HashMap<String, String> {
        match self.staff_list(&paths::employees(store)).await {
            Some(s) => staff_names(&s),
            None => HashMap::new(),
        }
    }

    /// Your dealership messages from the last `days` days, plus older ones (back `ro_days`) on the ROs in `ros`
    pub async fn chat_list(&self, store: &str, days: i64, ro_days: i64, ros: &[String]) -> Result<Vec<ChatMessage>> {
        let (_, cfg) = self.chat_setup(store).await?;
        let dealer = &cfg.dealer_code;
        let body = json!({
            "usernames": [escape_node(&cfg.user_name.to_lowercase())],
            "context": { "DEALER_CODE": dealer },
            "days": ro_days.max(days),
            "includeNotification": true,
            "chatType": "dealerChat",
        });
        let data = self
            .chat_call(
                store,
                &format!("chatgateway/rest/data/history?uri=/chat/channel/history?dealerCode={}", enc(dealer)),
                Some(body),
            )
            .await?;
        let names = self.chat_names(store).await;
        let since = now_ms() - days.max(1) * 86_400_000;
        Ok(array_in(&data)
            .iter()
            .map(|r| read_message(r, &cfg.user_name, &names))
            .filter(|m| !m.text.is_empty() && (m.time.unwrap_or(0) >= since || ros.contains(&m.ro)))
            .collect())
    }

    /// One conversation's messages, oldest first
    pub async fn chat_history(&self, store: &str, channel: &str) -> Result<Vec<ChatMessage>> {
        let (_, cfg) = self.chat_setup(store).await?;
        let dealer = &cfg.dealer_code;
        let body = json!({
            "context": { "DEALER_CODE": dealer },
            "includeNotification": true,
            "channelIds": [channel],
            "chatType": "dealerChat",
        });
        let path = format!(
            "chatgateway/rest/data/history?uri=/chat/channel/history/pages?dealerCode={}{}",
            enc(dealer),
            enc("&limit=200")
        );
        let data = self.chat_call(store, &path, Some(body)).await?;
        let names = self.chat_names(store).await;
        let mut list: Vec<ChatMessage> = array_in(&data)
            .iter()
            .map(|r| read_message(r, &cfg.user_name, &names))
            .filter(|m| !m.text.is_empty())
            .collect();
        list.sort_by_key(|m| m.time.unwrap_or(0));
        Ok(list)
    }

    /// Xtime's own count of dealership messages you haven't read
    pub async fn chat_count(&self, store: &str) -> Result<u64> {
        let (_, cfg) = self.chat_setup(store).await?;
        let body = json!({
            "days": 14,
            "context": { "DEALER_CODE": cfg.dealer_code },
            "usernames": [escape_node(&cfg.user_name.to_lowercase())],
            "state": "NEW",
            "dir": "recv",
            "chatType": "dealerChat",
        });
        let d = self
            .chat_call(
                store,
                &format!(
                    "chatgateway/rest/data/history?uri=/chat/channel/history/count?dealerCode={}",
                    enc(&cfg.dealer_code)
                ),
                Some(body),
            )
            .await?;
        fn sum(v: &Value) -> u64 {
            match v {
                Value::Number(n) => n.as_u64().unwrap_or(0),
                Value::Object(m) => m.values().filter_map(Value::as_u64).sum(),
                _ => 0,
            }
        }
        Ok(match &d {
            Value::Number(n) => n.as_u64().unwrap_or(0),
            Value::Object(m) if m.get("count").is_some_and(Value::is_number) => m["count"].as_u64().unwrap_or(0),
            Value::Object(m) => sum(m.get("data").or_else(|| m.get("content")).unwrap_or(&d)),
            _ => 0,
        })
    }

    /// The people you can @mention (Xtime's chat roster; the store's staff list when it's empty)
    pub async fn chat_roster(&self, store: &str) -> Result<Vec<ChatPerson>> {
        let (_, cfg) = self.chat_setup(store).await?;
        let me = bare_user(&cfg.user_name);
        let mut people: Vec<ChatPerson> = Vec::new();
        let add = |people: &mut Vec<ChatPerson>, local: &str, name: String| {
            let local = local.to_lowercase();
            let local = local.trim_end_matches("@chat.xtime.com");
            if local.is_empty() || name.is_empty() || bare_user(local) == me {
                return;
            }
            let id = format!("jid|{}", escape_node(local));
            if !people.iter().any(|p| p.id == id) {
                people.push(ChatPerson { id, name });
            }
        };
        let names = self.chat_names(store).await;
        if let Ok(d) = self
            .chat_call(
                store,
                &format!("chatgateway/rest/roster/users?dealerCode={}", enc(&cfg.dealer_code)),
                None,
            )
            .await
        {
            for u in array_in(&d) {
                let jid = ["jid", "username", "userName", "user"]
                    .iter()
                    .find_map(|k| u.get(*k).and_then(Value::as_str))
                    .map(unescape_node)
                    .unwrap_or_default();
                let local = jid.trim_end_matches("@chat.xtime.com").to_string();
                let name = ["name", "displayName", "fullName"]
                    .iter()
                    .find_map(|k| {
                        u.get(*k)
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| pretty_name(&names, &local));
                add(&mut people, &local, name);
            }
        }
        if people.is_empty() {
            if let Some(staff) = self.staff_list(&paths::employees(store)).await {
                for e in staff.iter() {
                    let s = |k: &str| e.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
                    let kind = e
                        .get("EmployeeType")
                        .and_then(|t| t.get("Name"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_lowercase();
                    if e.get("Active") == Some(&Value::Bool(false))
                        || s("Username").is_empty()
                        || kind.contains("api")
                        || kind.contains("bdc")
                    {
                        continue;
                    }
                    let full = [s("FirstName"), s("LastName")]
                        .into_iter()
                        .filter(|x| !x.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let store_id = e
                        .get("StoreID")
                        .map(|v| crate::json::text(Some(v)))
                        .filter(|v| !v.is_empty())
                        .unwrap_or_else(|| store.trim().to_string());
                    add(&mut people, &format!("{}@s{store_id}", bare_user(&s("Username"))), full);
                }
            }
        }
        people.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(people)
    }

    /// Where Xtime's chat server is (asked of Xtime's login service once, as Xtime's page does)
    async fn chat_server(&self, token: &str) -> Result<String> {
        if let Some(s) = self.chat.lock().expect("chat lock").server.clone() {
            return Ok(s);
        }
        let url = format!("https://login{}.xtime.app.coxautoinc.com/rest/login/ejdUrl", self.env_part());
        let req = chat_request(Method::GET, &url)?.header("Authorization", format!("Bearer {token}"));
        let res = self.send(req).await?;
        if !res.ok() {
            return Err(Error::Status(res.status.as_u16()));
        }
        let server = xmpp::websocket_address(&res.body).ok_or_else(|| Error::Format("couldn't find Xtime's chat server".into()))?;
        self.chat.lock().expect("chat lock").server = Some(server.clone());
        Ok(server)
    }

    /// Sends messages over Xtime's chat server (a short connection: sign in, send, close). A refused sign-in gets a
    /// new chat token and one more try; refused again, Xtime's sign-in has run out.
    async fn chat_deliver(&self, store: &str, stanzas: &[String], watch: &[String]) -> Result<()> {
        let (token, cfg) = self.chat_setup(store).await?;
        let server = self.chat_server(&token).await?;
        let origin = self.base.as_str().trim_end_matches('/').to_string();
        let go = |token: String| {
            let (server, origin, user) = (server.clone(), origin.clone(), cfg.user_name.clone());
            async move { xmpp::deliver(&server, &origin, CHAT_AGENT, &user, &token, stanzas, watch).await }
        };
        match go(token).await {
            Ok(()) => Ok(()),
            Err(Refused::Other(e)) => Err(e),
            Err(Refused::SignIn) => match go(self.chat_token(true).await?).await {
                Ok(()) => Ok(()),
                Err(Refused::Other(e)) => Err(e),
                Err(Refused::SignIn) => Err(Error::SessionExpired),
            },
        }
    }

    /// Sends a message to a conversation (its channel id; `ro`: its RO, "" to read it from the id). `text` may hold
    /// @mentions as Xtime's markup ([`to_markup`]). A new RO conversation starts the same way, as Xtime's page starts
    /// one: the first message sent to the RO's channel ([`crate::ro::RoView::chat_channel`]), with the RO and dealer
    /// code as its context.
    pub async fn chat_send(&self, store: &str, channel: &str, ro: &str, text: &str) -> Result<()> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::Format("nothing to send".into()));
        }
        if channel.is_empty() {
            return Err(Error::Format("Xtime has no chat for this RO".into()));
        }
        let (_, cfg) = self.chat_setup(store).await?;
        let dealer = cfg.dealer_code.clone();
        // the conversation's latest messages, for the context Xtime's page sends along
        let body = json!({
            "context": { "DEALER_CODE": dealer },
            "includeNotification": false,
            "channelIds": [channel],
            "chatType": "dealerChat",
        });
        let path = format!(
            "chatgateway/rest/data/history?uri=/chat/channel/history/pages?dealerCode={}{}",
            enc(&dealer),
            enc("&limit=20")
        );
        let records = self
            .chat_call(store, &path, Some(body))
            .await
            .map(|d| array_in(&d))
            .unwrap_or_default();
        let ro = if ro.is_empty() { ro_from_channel(channel) } else { ro.to_string() };
        let ctx = send_context(&records, &ro, &dealer, &xmpp::uuid());
        let id = xmpp::uuid();
        let stanza = xmpp::message_stanza(channel, &dealer, &ctx, text, &id);
        self.chat_deliver(store, &[stanza], &[id]).await
    }

    /// Marks your unread messages in a conversation read (what Xtime's page does when you open it): asks Xtime which
    /// are unread, then tells the chat server each one's been read. How many were marked.
    pub async fn chat_mark_read(&self, store: &str, channel: &str) -> Result<usize> {
        if channel.is_empty() {
            return Ok(0);
        }
        let (_, cfg) = self.chat_setup(store).await?;
        let dealer = cfg.dealer_code.clone();
        let body = json!({
            "context": { "DEALER_CODE": dealer },
            "usernames": [escape_node(&cfg.user_name.to_lowercase())],
            "channelIds": [channel],
            "days": 120,
        });
        let path = format!(
            "chatgateway/rest/data/history?uri=/chat/getUnreadArchiveIds?dealerCode={}",
            enc(&dealer)
        );
        let ids = unread_ids(&self.chat_call(store, &path, Some(body)).await?);
        if ids.is_empty() {
            return Ok(0);
        }
        let stanzas: Vec<String> = ids.iter().map(|id| xmpp::read_stanza(id, channel, &dealer)).collect();
        self.chat_deliver(store, &stanzas, &[]).await?;
        Ok(ids.len())
    }
}

impl Xtime {
    /// A picture sent in chat: the message is its address, on Amazon's storage (S3), where Xtime's app puts them
    /// ("https://sdkmobileapi-….s3.us-west-2.amazonaws.com/media/…/….jpg"). Its bytes. Only from there (a
    /// message's words aren't sent anywhere else).
    pub async fn chat_picture(&self, url: &str) -> Result<Vec<u8>> {
        let u = workbench_net::Url::parse(url).map_err(|_| Error::Format(format!("not an address: {url}")))?;
        let host = u.host_str().unwrap_or_default().to_ascii_lowercase();
        if u.scheme() != "https" || !host.ends_with(".amazonaws.com") {
            return Err(Error::Format("Not a chat picture".into()));
        }
        let res = self.http.send_bytes(workbench_net::Http::request(Method::GET, u)).await?;
        if !res.status.is_success() {
            return Err(Error::Status(res.status.as_u16()));
        }
        Ok(res.bytes)
    }
}

/// A chat message that's a picture: its address ("https://….amazonaws.com/…/x.jpg", on its own or in the words)
pub fn picture_urls(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|w| {
            let lower = w.to_ascii_lowercase();
            let path = lower.split(['?', '#']).next().unwrap_or("");
            lower.starts_with("https://")
                && lower.contains(".amazonaws.com/")
                && [".jpg", ".jpeg", ".png", ".gif", ".webp"].iter().any(|e| path.ends_with(e))
        })
        .map(str::to_string)
        .collect()
}

/// A request to chat's own sites (no Inspect headers: those are for Inspect only)
fn chat_request(method: Method, url: &str) -> Result<workbench_net::Request> {
    let url = workbench_net::Url::parse(url).map_err(|_| Error::Format(format!("not an address: {url}")))?;
    Ok(workbench_net::Http::request(method, url))
}

/// The token runs out within the minute (its "exp" claim)
fn token_expired(t: &str) -> bool {
    jwt_claims(t)
        .and_then(|c| c.get("exp").and_then(Value::as_i64))
        .is_none_or(|exp| exp * 1000 < now_ms() + 60_000)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// encodeURIComponent
fn enc(s: &str) -> String {
    crate::employee::encode(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_pictures() {
        let url = "https://sdkmobileapi-969674f0-5c81-475e-8a94-027769595a05.s3.us-west-2.amazonaws.com/media/d65381eb/e626e934.jpg";
        assert_eq!(picture_urls(url), [url]);
        assert_eq!(picture_urls(&format!("Look at this {url} ok")), [url]);
        assert!(picture_urls("https://example.com/x.jpg").is_empty());
        assert!(picture_urls("https://x.s3.amazonaws.com/doc.pdf").is_empty());
        assert!(picture_urls("RO 820326 is done").is_empty());
    }

    fn names() -> HashMap<String, String> {
        staff_names(&[
            json!({ "FirstName": "Brenton", "LastName": "Skeeters", "Username": "bskeeters_xtime", "Active": true }),
            json!({ "FirstName": "Ashton", "LastName": "M", "Username": "amiller" }),
        ])
    }

    #[test]
    fn names_people_like_xtime() {
        assert_eq!(escape_node("a miller@s3590"), r"a\20miller\40s3590");
        assert_eq!(unescape_node(r"bskeeters\40s3590"), "bskeeters@s3590");
        assert_eq!(user_of(r"bskeeters\40s3590@chat.xtime.com/web"), "bskeeters@s3590");
        assert_eq!(bare_user("aking__S3590_xtime"), "aking");
        assert_eq!(bare_user("amiller@s3590"), "amiller");
        assert_eq!(pretty_name(&names(), "bskeeters@s3590"), "Brenton Skeeters");
        assert_eq!(pretty_name(&names(), "john.smith"), "John Smith");
    }

    #[test]
    fn makes_text_plain() {
        let (t, m) = plain(
            r"Hey @[Brenton\20Skeeters](jid|bskeeters\40s3590), see [photo](img:1|https://x/y.jpg) \*now\* &amp; go",
            &names(),
        );
        assert_eq!(t, "Hey @Brenton Skeeters, see photo *now* & go");
        assert_eq!(m, ["Brenton Skeeters"]);
        let (t, m) = plain(r"[](jid|bskeeters\40s3590) ok", &names());
        assert_eq!((t.as_str(), m), ("@Brenton Skeeters ok", vec!["Brenton Skeeters".to_string()]));
    }

    #[test]
    fn reads_a_history_record() {
        let raw = json!({
            "archiveId": 101, "channelId": "3590-820367-1", "createdAt": "2026-11-15 14:05:03.5", "status": "NEW",
            "chatmsg": r#"<message from="bskeeters\40s3590@chat.xtime.com/web"><body>{"body":"Parts are in"}</body></message>"#
        });
        let m = read_message(&raw, "amiller", &names());
        assert_eq!(
            (m.id.as_str(), m.ro.as_str(), m.sender.as_str(), m.text.as_str()),
            ("101", "820367", "Brenton Skeeters", "Parts are in")
        );
        assert!(m.unread && !m.mine);
        assert_eq!(m.time, Some(1_794_751_503_500));
        let mine = read_message(
            &json!({ "dir": "send", "chatmsg": "<message><body>hi</body></message>" }),
            "amiller",
            &names(),
        );
        assert!(mine.mine && !mine.unread);
    }

    #[test]
    fn groups_conversations() {
        let m = |ch: &str, t: i64, unread: bool| ChatMessage {
            channel_id: ch.into(),
            time: Some(t),
            unread,
            text: format!("{ch}{t}"),
            ..Default::default()
        };
        let c = conversations(&[m("a", 1, true), m("a", 5, false), m("b", 3, false)]);
        assert_eq!(
            c.iter().map(|x| (x.text.as_str(), x.unread)).collect::<Vec<_>>(),
            [("a5", true), ("b3", false)]
        );
    }

    #[test]
    fn writes_mentions_as_xtime_does() {
        let picked = vec![
            ("Al Smith".to_string(), r"jid|asmith\40s1".to_string()),
            ("Al Smithson".to_string(), r"jid|asmithson\40s1".to_string()),
        ];
        assert_eq!(
            to_markup("@Al Smithson and @Al Smith, look", &picked),
            r"[Al Smithson](jid|asmithson\40s1) and [Al Smith](jid|asmith\40s1), look"
        );
        assert_eq!(to_markup("no one", &picked), "no one");
    }

    #[test]
    fn picks_the_context_and_unread_ids() {
        let rec = |at: &str, id: i64, ro: &str| -> Value {
            let msg = format!(
                "<message><body>x</body><xtmsg><xtctx namespace='RO' contextId='{ro}'/><xtctx namespace='CHAT_MSG_ID' contextId='9'/><xtctx namespace='TECH' contextId='77'/></xtmsg></message>"
            );
            serde_json::from_str::<Value>(&format!(r#"{{"createdAt":"{at}","archiveId":{id},"chatmsg":"{msg}"}}"#)).unwrap()
        };
        let recs = vec![rec("2026-11-15 10:00:00", 2, "820367"), rec("2026-11-15 09:00:00", 1, "1")];
        let ctx = send_context(&recs, "820367", "F1", "u1");
        let names: Vec<&str> = ctx.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["RO", "TECH", "sent_msg_uuid"]);
        // the latest is for another RO: just the RO and dealer code
        let ctx = send_context(&recs, "555", "F1", "u1");
        assert_eq!(ctx[0], ("RO".to_string(), "555".to_string()));
        assert_eq!(ctx[1], ("DEALER_CODE".to_string(), "F1".to_string()));
        assert_eq!(send_context(&[], "555", "F1", "u1").len(), 3);

        assert_eq!(unread_ids(&json!([101, "102", 101])), ["101", "102"]);
        assert_eq!(unread_ids(&json!([{ "archiveId": 7, "x": 1 }])), ["7"]);
        assert_eq!(unread_ids(&json!({ "data": { "3590-1-1": [5, 6] } })), ["5", "6"]);
        assert!(unread_ids(&json!({ "status": "ok" })).is_empty());
    }

    #[test]
    fn reads_times_tokens_and_settings() {
        assert_eq!(parse_time(&json!("1970-01-02T00:00:00Z")), Some(86_400_000));
        assert_eq!(parse_time(&json!("1970-01-01 01:00:00+01:00")), Some(0));
        assert_eq!(parse_time(&json!(1234)), Some(1234));
        assert_eq!(parse_time(&json!("1234")), Some(1234));
        assert_eq!(b64_decode("aGk_").unwrap(), b"hi?");
        assert_eq!(b64_decode("aGk/").unwrap(), b"hi?");
        // header.{"dealerCode":"F1","preferred_username":"amiller","exp":1}.sig
        let token = "x.eyJkZWFsZXJDb2RlIjoiRjEiLCJwcmVmZXJyZWRfdXNlcm5hbWUiOiJhbWlsbGVyIiwiZXhwIjoxfQ.y";
        let c = config_from_claims(&jwt_claims(token).unwrap(), "3590").unwrap();
        assert_eq!((c.dealer_code.as_str(), c.user_name.as_str()), ("F1", "amiller"));
        assert!(token_expired(token));
        assert_eq!(c.gateway(), "https://chatgateway.xtime.com/");
        assert!(config_from_claims(&json!({ "dealerCode": "F1", "sub": "12345" }), "1").is_none());
        let saved: ChatConfig =
            serde_json::from_str(r#"{"dealerCode":"F1","userName":"amiller","env":"","chatgatewayUrl":"","storeId":"3590"}"#).unwrap();
        assert_eq!(saved.user_name, "amiller");
    }
}
