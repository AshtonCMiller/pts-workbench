//! Notifications (the Electron app's 🔔, notifications.js): what happened on your ROs and in chat while you were
//! busy. Each is kept in the app's list (the bell), on this computer only: the newest 100 from the last 7 days, in
//! `notifications.json`. A new one also pops up in the corner for a few seconds and flashes the app's taskbar button.
//! Clicking one that's about an RO goes to that RO.
//!
//! What's noticed, by comparing what Xtime says now with what it said last time (nothing on the first look, so
//! opening the app doesn't announce everything at once):
//!   your list        an RO assigned to you
//!   an RO's stage    sent to Approval, approved for Repair, sent to Review (not the moves you made in the app)
//!   its lines        lines approved or declined
//!   parts            parts in (no longer waiting on parts), parts info complete
//!   waiting          the customer answered, warranty answered
//!   chat             a new message from someone else (a mention of you says so)

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Kept this long (ms) and this many
const KEEP_MS: i64 = 7 * 24 * 3600 * 1000;
const KEEP: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Assigned,
    Stage,
    Approved,
    Declined,
    Parts,
    Customer,
    Warranty,
    Message,
}

impl Kind {
    /// Its colour in the list
    pub fn color(self) -> [u8; 3] {
        match self {
            Kind::Assigned => [0x1a, 0x5f, 0xb4],
            Kind::Stage => [0x7a, 0x5a, 0xf5],
            Kind::Approved => [0x15, 0x80, 0x3d],
            Kind::Declined => [0xb4, 0x23, 0x18],
            Kind::Parts => [0xd9, 0x77, 0x06],
            Kind::Customer => [0xb4, 0x53, 0x09],
            Kind::Warranty => [0x7c, 0x3a, 0xed],
            Kind::Message => [0x0e, 0x74, 0x90],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: u64,
    /// When (ms since 1970)
    pub at: i64,
    pub kind: Kind,
    pub title: String,
    pub body: String,
    /// The RO it's about ("" when none)
    #[serde(default)]
    pub ro: String,
    /// The chat conversation it's about ("" when none)
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub read: bool,
}

/// A notification to add (the list gives it its ID and time)
#[derive(Debug, Clone, PartialEq)]
pub struct New {
    pub kind: Kind,
    pub title: String,
    pub body: String,
    pub ro: String,
    pub channel: String,
}

fn new(kind: Kind, ro: &str, title: String, body: String) -> New {
    New {
        kind,
        title,
        body,
        ro: ro.to_string(),
        channel: String::new(),
    }
}

/// The list, newest first
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Notes {
    pub list: Vec<Note>,
    #[serde(default)]
    next: u64,
}

fn file() -> PathBuf {
    crate::settings::app_dir().join("notifications.json")
}

impl Notes {
    pub fn load(now_ms: i64) -> Self {
        let mut n: Notes = std::fs::read_to_string(file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        n.trim(now_ms);
        n
    }

    pub fn save(&self) {
        let f = file();
        let tmp = f.with_extension("json.tmp");
        if let Ok(t) = serde_json::to_string(self) {
            let _ = std::fs::write(&tmp, t).and_then(|_| std::fs::rename(&tmp, &f));
        }
    }

    fn trim(&mut self, now_ms: i64) {
        self.list.retain(|n| now_ms - n.at < KEEP_MS);
        self.list.truncate(KEEP);
        self.next = self.next.max(self.list.iter().map(|n| n.id + 1).max().unwrap_or(1));
    }

    /// Adds them (newest first); their IDs
    pub fn add(&mut self, news: Vec<New>, now_ms: i64) -> Vec<u64> {
        let mut ids = Vec::new();
        for n in news {
            self.next = self.next.max(1);
            let id = self.next;
            self.next += 1;
            self.list.insert(
                0,
                Note {
                    id,
                    at: now_ms,
                    kind: n.kind,
                    title: n.title,
                    body: n.body,
                    ro: n.ro,
                    channel: n.channel,
                    read: false,
                },
            );
            ids.push(id);
        }
        self.trim(now_ms);
        ids
    }

    pub fn get(&self, id: u64) -> Option<&Note> {
        self.list.iter().find(|n| n.id == id)
    }

    pub fn unread(&self) -> usize {
        self.list.iter().filter(|n| !n.read).count()
    }

    pub fn mark_read(&mut self, id: u64) {
        if let Some(n) = self.list.iter_mut().find(|n| n.id == id) {
            n.read = true;
        }
    }

    pub fn mark_all_read(&mut self) {
        for n in &mut self.list {
            n.read = true;
        }
    }

    /// An RO's notifications read (it's been opened)
    pub fn read_ro(&mut self, ro: &str) -> bool {
        let mut changed = false;
        for n in self.list.iter_mut().filter(|n| n.ro == ro && !n.read && n.kind != Kind::Message) {
            n.read = true;
            changed = true;
        }
        changed
    }
}

// ---------- watching ----------

/// One of your ROs as the dashboard has it now
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoNow {
    pub ro: String,
    pub tag: String,
    /// "2023 Ford Bronco Sport"
    pub vehicle: String,
    pub mode: Option<i64>,
    pub waiting_parts: bool,
    pub not_in_stock: bool,
    pub waiting_customer: bool,
    pub waiting_warranty: bool,
    /// Xtime's parts status (2 waiting at parts, 3 working, 4 complete; 0 none)
    pub parts_status: i64,
}

/// One of an RO's lines now
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LineNow {
    pub key: String,
    /// "B  Replace front brake pads"
    pub label: String,
    pub approved: bool,
    pub declined: bool,
}

/// A chat message as it's listed
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatNow {
    pub id: String,
    pub channel: String,
    pub ro: String,
    pub sender: String,
    pub text: String,
    pub mine: bool,
    /// Names @mentioned in it
    pub mentions: Vec<String>,
}

/// What was seen last time (in memory: a restart starts looking again)
#[derive(Debug, Default)]
pub struct Watch {
    mine: Option<HashSet<String>>,
    seen: HashMap<String, RoNow>,
    lines: HashMap<String, (HashSet<String>, HashSet<String>)>,
    chat: Option<HashSet<String>>,
    /// Moves made in this app (RO → the stage, when, ms): not announced back
    moved: HashMap<String, (i64, i64)>,
}

fn joined(parts: &[&str]) -> String {
    parts.iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" · ")
}

fn short(s: &str, n: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= n {
        s
    } else {
        format!("{}…", s.chars().take(n.saturating_sub(1)).collect::<String>().trim_end())
    }
}

impl Watch {
    /// A stage change made here (it isn't announced when it shows up in Xtime)
    pub fn moved_here(&mut self, ro: &str, mode: i64, now_ms: i64) {
        self.moved.insert(ro.to_string(), (mode, now_ms));
    }

    /// Your list as the dashboard has it now
    pub fn dashboard(&mut self, list: &[RoNow], now_ms: i64) -> Vec<New> {
        let mut out = Vec::new();
        let now: HashSet<String> = list.iter().map(|r| r.ro.clone()).collect();
        for r in list {
            let tag = if r.tag.is_empty() {
                String::new()
            } else {
                format!("Tag {}", r.tag)
            };
            // a new RO on your list (not on the first look)
            if self.mine.as_ref().is_some_and(|m| !m.contains(&r.ro)) {
                out.push(new(Kind::Assigned, &r.ro, format!("RO {} assigned to you", r.ro), {
                    let b = joined(&[&r.vehicle, &tag]);
                    if b.is_empty() { "New RO on your list".into() } else { b }
                }));
            }
            let Some(was) = self.seen.get(&r.ro) else { continue };
            // its stage
            if let (Some(before), Some(after)) = (was.mode, r.mode) {
                let mine_here = self
                    .moved
                    .get(&r.ro)
                    .is_some_and(|(m, at)| *m == after && now_ms - at < 10 * 60_000);
                if before != after && !mine_here {
                    let said = match after {
                        4 => Some(format!("RO {} sent to Approval", r.ro)),
                        6 => Some(format!("RO {} approved: ready for repair", r.ro)),
                        7 => Some(format!("RO {} sent to Review", r.ro)),
                        3 if before == 4 || before == 6 => Some(format!("RO {} sent back to Inspection", r.ro)),
                        _ => None,
                    };
                    if let Some(title) = said {
                        out.push(new(Kind::Stage, &r.ro, title, joined(&[&r.vehicle, &tag])));
                    }
                }
            }
            // parts in, and parts info complete
            let parts_before = was.waiting_parts || was.not_in_stock;
            let parts_now = r.waiting_parts || r.not_in_stock;
            if parts_before && !parts_now {
                out.push(new(Kind::Parts, &r.ro, format!("RO {}: parts are in", r.ro), {
                    let b = joined(&[&r.vehicle, &tag]);
                    if b.is_empty() { "No longer waiting for parts".into() } else { b }
                }));
            } else if was.parts_status != 4 && r.parts_status == 4 {
                out.push(new(
                    Kind::Parts,
                    &r.ro,
                    format!("RO {}: parts info complete", r.ro),
                    joined(&[&r.vehicle, "Parts has finished with it"]),
                ));
            }
            if was.waiting_customer && !r.waiting_customer {
                out.push(new(
                    Kind::Customer,
                    &r.ro,
                    format!("RO {}: customer answered", r.ro),
                    joined(&[&r.vehicle, "No longer waiting on the customer"]),
                ));
            }
            if was.waiting_warranty && !r.waiting_warranty {
                out.push(new(
                    Kind::Warranty,
                    &r.ro,
                    format!("RO {}: warranty answered", r.ro),
                    joined(&[&r.vehicle, "No longer waiting on warranty"]),
                ));
            }
        }
        for r in list {
            self.seen.insert(r.ro.clone(), r.clone());
        }
        self.mine = Some(now);
        out
    }

    /// An RO's lines as Xtime has them now (approved / declined since the last look)
    pub fn lines(&mut self, ro: &str, lines: &[LineNow]) -> Vec<New> {
        let approved: HashSet<String> = lines.iter().filter(|l| l.approved && !l.declined).map(|l| l.key.clone()).collect();
        let declined: HashSet<String> = lines.iter().filter(|l| l.declined).map(|l| l.key.clone()).collect();
        let before = self.lines.insert(ro.to_string(), (approved.clone(), declined.clone()));
        let Some((was_ok, was_no)) = before else { return Vec::new() };
        let mut out = Vec::new();
        let fresh: Vec<&LineNow> = lines
            .iter()
            .filter(|l| approved.contains(&l.key) && !was_ok.contains(&l.key))
            .collect();
        let lost: Vec<&LineNow> = lines
            .iter()
            .filter(|l| declined.contains(&l.key) && !was_no.contains(&l.key))
            .collect();
        let labels = |ls: &[&LineNow]| short(&ls.iter().map(|l| l.label.as_str()).collect::<Vec<_>>().join("; "), 160);
        if !fresh.is_empty() {
            let title = if fresh.len() == 1 {
                format!("RO {ro}: line approved")
            } else {
                format!("RO {ro}: {} lines approved", fresh.len())
            };
            out.push(new(Kind::Approved, ro, title, labels(&fresh)));
        }
        if !lost.is_empty() {
            let title = if lost.len() == 1 {
                format!("RO {ro}: line declined")
            } else {
                format!("RO {ro}: {} lines declined", lost.len())
            };
            out.push(new(Kind::Declined, ro, title, labels(&lost)));
        }
        out
    }

    /// The chat list now: messages from others not seen before. `me`: your name (a mention of you says so)
    pub fn chat(&mut self, list: &[ChatNow], me: &str) -> Vec<New> {
        let ids: HashSet<String> = list.iter().map(|m| m.id.clone()).collect();
        let Some(seen) = self.chat.replace(ids.clone()) else {
            return Vec::new();
        };
        let me = me.trim().to_lowercase();
        let mut out = Vec::new();
        for m in list.iter().filter(|m| !m.mine && !m.id.is_empty() && !seen.contains(&m.id)) {
            let sender = if m.sender.trim().is_empty() {
                "Someone".to_string()
            } else {
                m.sender.trim().to_string()
            };
            let mentioned = !me.is_empty() && m.mentions.iter().any(|n| n.trim().to_lowercase() == me);
            let on = if m.ro.is_empty() {
                String::new()
            } else {
                format!(" on RO {}", m.ro)
            };
            let title = if mentioned {
                format!("{sender} mentioned you{on}")
            } else {
                format!("New message from {sender}{on}")
            };
            out.push(New {
                kind: Kind::Message,
                title,
                body: short(&m.text, 140),
                ro: m.ro.clone(),
                channel: m.channel.clone(),
            });
        }
        // (a pile at once, after a long time away: one note says how many)
        if out.len() > 4 {
            let n = out.len();
            out = vec![New {
                kind: Kind::Message,
                title: format!("{n} new chat messages"),
                body: "Open Chat to read them.".into(),
                ro: String::new(),
                channel: String::new(),
            }];
        }
        out
    }
}

/// "just now", "4 min ago", "2 h ago", "Tue 3:15 PM"
pub fn when(at_ms: i64, now_ms: i64) -> String {
    let secs = (now_ms - at_ms).max(0) / 1000;
    match secs {
        0..60 => "just now".into(),
        s @ 60..3600 => format!("{} min ago", s / 60),
        s @ 3600..86_400 => format!("{} h ago", s / 3600),
        _ => {
            let t = chrono::DateTime::from_timestamp_millis(at_ms).map(|d| d.with_timezone(&chrono::Local));
            t.map(|d| d.format("%a %-I:%M %p").to_string()).unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ro(n: &str, mode: i64) -> RoNow {
        RoNow {
            ro: n.into(),
            vehicle: "2023 Ford Bronco Sport".into(),
            mode: Some(mode),
            ..Default::default()
        }
    }

    #[test]
    fn nothing_on_the_first_look() {
        let mut w = Watch::default();
        assert!(w.dashboard(&[ro("1", 3)], 0).is_empty());
        // then a new one on the list
        let n = w.dashboard(&[ro("1", 3), ro("2", 2)], 0);
        assert_eq!(n.len(), 1);
        assert_eq!(
            (n[0].kind, n[0].title.as_str(), n[0].ro.as_str()),
            (Kind::Assigned, "RO 2 assigned to you", "2")
        );
    }

    #[test]
    fn stages_parts_and_waiting() {
        let mut w = Watch::default();
        let mut a = ro("1", 3);
        a.waiting_parts = true;
        a.waiting_customer = true;
        w.dashboard(&[a.clone()], 0);
        a.mode = Some(4);
        a.waiting_parts = false;
        a.waiting_customer = false;
        let n = w.dashboard(&[a.clone()], 0);
        let titles: Vec<&str> = n.iter().map(|x| x.title.as_str()).collect();
        assert_eq!(titles, ["RO 1 sent to Approval", "RO 1: parts are in", "RO 1: customer answered"]);
        // moved here: not announced back
        w.moved_here("1", 6, 1000);
        a.mode = Some(6);
        assert!(w.dashboard(&[a.clone()], 2000).is_empty());
        // parts info complete
        a.parts_status = 4;
        assert_eq!(w.dashboard(&[a.clone()], 3000)[0].title, "RO 1: parts info complete");
        // to Repair by someone else
        a.mode = Some(4);
        w.dashboard(&[a.clone()], 4000);
        a.mode = Some(6);
        assert_eq!(w.dashboard(&[a], 20 * 60_000)[0].title, "RO 1 approved: ready for repair");
    }

    #[test]
    fn approvals() {
        let mut w = Watch::default();
        let line = |k: &str, ok: bool, no: bool| LineNow {
            key: k.into(),
            label: format!("{k} work"),
            approved: ok,
            declined: no,
        };
        assert!(
            w.lines("1", &[line("A", true, false), line("B", false, false), line("C", false, false)])
                .is_empty()
        );
        let n = w.lines("1", &[line("A", true, false), line("B", true, false), line("C", false, true)]);
        assert_eq!(n.len(), 2);
        assert_eq!(
            (n[0].kind, n[0].title.as_str(), n[0].body.as_str()),
            (Kind::Approved, "RO 1: line approved", "B work")
        );
        assert_eq!((n[1].kind, n[1].title.as_str()), (Kind::Declined, "RO 1: line declined"));
    }

    #[test]
    fn chat_messages() {
        let mut w = Watch::default();
        let m = |id: &str, mine: bool, mentions: &[&str]| ChatNow {
            id: id.into(),
            channel: "c1".into(),
            ro: "820367".into(),
            sender: "Tony B".into(),
            text: "Parts   are here".into(),
            mine,
            mentions: mentions.iter().map(|s| s.to_string()).collect(),
        };
        assert!(w.chat(&[m("1", false, &[])], "Ashton M").is_empty());
        let n = w.chat(&[m("1", false, &[]), m("2", false, &["Ashton M"]), m("3", true, &[])], "ashton m");
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].title, "Tony B mentioned you on RO 820367");
        assert_eq!((n[0].body.as_str(), n[0].channel.as_str()), ("Parts are here", "c1"));
    }

    #[test]
    fn the_list() {
        let mut l = Notes::default();
        let ids = l.add(
            vec![
                new(Kind::Parts, "1", "a".into(), "".into()),
                new(Kind::Stage, "2", "b".into(), "".into()),
            ],
            1000,
        );
        assert_eq!(ids, [1, 2]);
        assert_eq!(l.list[0].title, "b");
        assert_eq!(l.unread(), 2);
        assert!(l.read_ro("1"));
        assert_eq!(l.unread(), 1);
        l.add(vec![new(Kind::Parts, "", "old".into(), "".into())], 0);
        l.trim(KEEP_MS + 500);
        assert_eq!(l.list.len(), 2);
        assert_eq!(when(0, 90_000), "1 min ago");
    }
}
