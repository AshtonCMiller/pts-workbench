//! Your hours: the shift clock, the lines you've flagged (paid hours) and the spiffs you've turned in, with the
//! pay-period report. The Electron app's `tracker.js`, ported; the file is the same shape as its `tracking.json`
//! (field names and all), so hours carry over from it ([`Hours::import_electron`]). Nothing here is sent anywhere.
//!
//! Times are milliseconds since 1970 (as in the Electron app). Days and pay periods are the computer's local ones;
//! the functions that need them take the time zone, so tests can use UTC.

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{Datelike, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const DAY: i64 = 24 * 60 * 60 * 1000;

fn round2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

// ---------- the file ----------

/// Everything kept (the Electron app's tracking.json, plus spiffs)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Hours {
    /// ROs whose lines the app has read, by RO number
    pub ros: BTreeMap<String, RoRec>,
    pub flags: Vec<Flag>,
    pub shifts: Vec<Shift>,
    /// Last time the app was seen running, and closed (for "forgot to clock out")
    pub last_seen: Option<i64>,
    pub last_closed: Option<i64>,
    /// The latest any of your computers closed the app (from sync)
    pub last_closed_all: Option<i64>,
    pub next_flag_id: u64,
    /// Spiffs turned in, by RO number (the Rust app's; the Electron app keeps this field as it is)
    pub spiffs: BTreeMap<String, Spiff>,
    /// The ROs open in the Workbench, by RO number (no VIN): what the Electron app's sync carries between your
    /// computers (`w/RO` records), as it names it
    #[serde(rename = "openROs")]
    pub open_ros: BTreeMap<String, OpenRo>,
    /// Who closed an RO that was open (from sync: "app", or "old" for an app that didn't know about open ROs)
    pub open_closed_by: BTreeMap<String, String>,
    /// The Electron app's hours have been brought over (once)
    pub imported_from_electron: bool,
    /// Anything else the file has (kept as it is)
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// An RO open in the Workbench, as sync carries it
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenRo {
    /// "2023 Ford Bronco Sport"
    pub ymm: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RoRec {
    pub lines: BTreeMap<String, TLine>,
    /// The lines' keys, in the order shown
    pub order: Vec<String>,
    /// "2026 Ford Bronco"
    pub ymm: String,
    pub touched: Option<i64>,
    pub completed_at: Option<i64>,
    /// Review / Completed (not in the Rust app yet; kept)
    pub review: Option<Value>,
    /// What its approved recommendations came to when last read (dollars; for which ROs could be spiffed)
    pub spiff_total: Option<f64>,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// One of an RO's lines, as the hours see it
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TLine {
    /// "B", or "B:diag" / "B:2" when a letter has more than one line (see [`line_keys`])
    pub key: String,
    pub dms: String,
    pub complaint: String,
    pub service: String,
    /// Xtime's hours for it
    pub hours: f64,
    /// Your hours for it, when they differ
    #[serde(rename = "override")]
    pub override_hours: Option<f64>,
    /// Your own name for the line
    pub name: Option<String>,
    pub approved: bool,
    pub declined: bool,
    pub completed: bool,
    pub primary: bool,
    pub diag: bool,
    pub manual: bool,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

impl TLine {
    /// Whether it counts toward your hours: approved, a concern (pre-approved) or done (closed tickets often don't
    /// mark done lines approved), and not declined
    pub fn counts(&self) -> bool {
        (self.approved || self.primary || self.completed) && !self.declined
    }
    /// The hours paid for it
    pub fn paid(&self) -> f64 {
        self.override_hours.unwrap_or(self.hours)
    }
    /// What it's called: your name for it, the concern, or Xtime's service
    pub fn text(&self) -> &str {
        [self.name.as_deref().unwrap_or(""), &self.complaint, &self.service]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or("")
    }
    /// What a flag for it says
    pub fn flag_label(&self) -> String {
        format!("{}{}", if self.diag { "Diag: " } else { "" }, self.text())
    }
}

/// Where a line is: still to do, done in Xtime, or flagged by you
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStatus {
    Todo,
    Done,
    Flagged,
}

/// A line's flag, as a line shows it
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlagInfo {
    pub id: u64,
    pub flagged_at: i64,
    pub confirmed: bool,
}

/// One of an RO's lines, as the sidebar shows it ([`Hours::ro_summary`])
#[derive(Debug, Clone, PartialEq)]
pub struct LineRow {
    pub key: String,
    pub dms: String,
    /// What it's called (your name for it, or Xtime's)
    pub text: String,
    pub service: String,
    /// Xtime's own wording, when you've renamed it
    pub original: String,
    /// Xtime's hours, and yours when they differ
    pub hours: f64,
    pub override_hours: Option<f64>,
    /// What it pays (a flag's hours, once flagged)
    pub paid: f64,
    pub primary: bool,
    pub diag: bool,
    pub status: LineStatus,
    pub flag: Option<FlagInfo>,
}

/// An RO's lines and what they come to
#[derive(Debug, Clone, PartialEq)]
pub struct RoSummary {
    pub lines: Vec<LineRow>,
    pub total: f64,
    pub flagged: f64,
    pub flagged_count: usize,
}

/// A flagged line: its paid hours, and when you flagged it (which pay period it's in)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Flag {
    pub id: u64,
    pub ro: String,
    pub key: String,
    pub dms: String,
    pub label: String,
    pub ymm: String,
    pub hours: f64,
    pub flagged_at: i64,
    /// You've seen it on your time sheet
    pub confirmed: bool,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// Clocked in (and out)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Shift {
    pub start: i64,
    pub end: Option<i64>,
    /// Ended by the app (you forgot to clock out)
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub auto: bool,
}

/// A spiff you've turned in
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spiff {
    /// When you marked it
    pub at: i64,
    /// What the approved recommendations came to then (dollars)
    pub total: f64,
    pub tag: String,
    /// "2026 Ford Bronco"
    pub vehicle: String,
}

// ---------- reading Xtime's lines ----------

/// A line from Xtime, for [`line_keys`] and [`Hours::update_lines`]
#[derive(Debug, Clone, Default)]
pub struct XLine {
    /// Where it is in Xtime's list (for lines with no letter: "#3")
    pub index: usize,
    pub dms: String,
    pub complaint: String,
    pub service: String,
    pub hours: f64,
    pub approved: bool,
    pub declined: bool,
    pub completed: bool,
    pub primary: bool,
}

/// The hours' lines for an RO's lines from Xtime, keyed as the Electron app keys them: the line letter; when a letter
/// has more than one line, "B:diag" for its concern (the diag) and "B", "B:2"… for the rest, so each can be flagged
/// on its own and keys stay the same. In Xtime's order, each letter's diag first.
pub fn line_keys(lines: &[XLine]) -> Vec<TLine> {
    line_keys_indexed(lines).into_iter().map(|(_, l)| l).collect()
}

/// [`line_keys`], each with where it was in `lines`
pub fn line_keys_indexed(lines: &[XLine]) -> Vec<(usize, TLine)> {
    let mut out: Vec<TLine> = lines
        .iter()
        .map(|l| TLine {
            key: if l.dms.is_empty() {
                format!("#{}", l.index + 1)
            } else {
                l.dms.clone()
            },
            dms: l.dms.clone(),
            complaint: l.complaint.clone(),
            service: l.service.clone(),
            hours: if l.hours.is_finite() { l.hours } else { 0.0 },
            approved: l.approved,
            declined: l.declined,
            completed: l.completed,
            primary: l.primary,
            ..Default::default()
        })
        .collect();
    // letters used more than once
    let mut by_letter: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, l) in out.iter().enumerate() {
        by_letter.entry(l.key.clone()).or_default().push(i);
    }
    for (letter, group) in by_letter {
        if group.len() < 2 {
            continue;
        }
        let ordered: Vec<usize> = group
            .iter()
            .copied()
            .filter(|&i| out[i].primary)
            .chain(group.iter().copied().filter(|&i| !out[i].primary))
            .collect();
        let (mut diag, mut other) = (0, 0);
        for i in ordered {
            if out[i].primary {
                diag += 1;
                out[i].key = if diag == 1 {
                    format!("{letter}:diag")
                } else {
                    format!("{letter}:diag{diag}")
                };
                out[i].diag = true;
            } else {
                other += 1;
                out[i].key = if other == 1 { letter.clone() } else { format!("{letter}:{other}") };
            }
        }
    }
    // each letter where it first shows, its diag first
    let group_of = |l: &TLine| if l.dms.is_empty() { l.key.clone() } else { l.dms.clone() };
    let mut first: BTreeMap<String, usize> = BTreeMap::new();
    for (i, l) in out.iter().enumerate() {
        first.entry(group_of(l)).or_insert(i);
    }
    let mut indexed: Vec<(usize, TLine)> = out.into_iter().enumerate().collect();
    indexed.sort_by_key(|(i, l)| (first[&group_of(l)], !l.primary, *i));
    indexed
}

// ---------- pay periods and days (local) ----------

/// A pay period: the 1st–15th or the 16th–end of the month. `end` is the start of the next one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Period {
    pub offset: i32,
    pub start: i64,
    pub end: i64,
}

fn midnight<Tz: TimeZone>(tz: &Tz, y: i32, m: u32, d: u32) -> i64 {
    tz.with_ymd_and_hms(y, m, d, 0, 0, 0)
        .earliest()
        .map(|t| t.timestamp_millis())
        .unwrap_or_else(|| {
            chrono::NaiveDate::from_ymd_opt(y, m, d).map_or(0, |d| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis())
        })
}

/// The pay period `t` is in, moved by `offset` (-1: the one before)
pub fn pay_period<Tz: TimeZone>(tz: &Tz, t: i64, offset: i32) -> Period {
    let d = tz.timestamp_millis_opt(t).earliest().map(|d| d.date_naive()).unwrap_or_default();
    let idx = d.year() * 24 + d.month0() as i32 * 2 + if d.day() <= 15 { 0 } else { 1 } + offset;
    let y = idx.div_euclid(24);
    let r = idx.rem_euclid(24);
    let m = (r / 2) as u32 + 1;
    let second = r % 2 == 1;
    let start = midnight(tz, y, m, if second { 16 } else { 1 });
    let end = if second {
        if m == 12 {
            midnight(tz, y + 1, 1, 1)
        } else {
            midnight(tz, y, m + 1, 1)
        }
    } else {
        midnight(tz, y, m, 16)
    };
    Period { offset, start, end }
}

pub fn start_of_day<Tz: TimeZone>(tz: &Tz, t: i64) -> i64 {
    let d = tz.timestamp_millis_opt(t).earliest().map(|d| d.date_naive()).unwrap_or_default();
    midnight(tz, d.year(), d.month(), d.day())
}

fn overlap(s1: i64, e1: i64, s2: i64, e2: i64) -> i64 {
    (e1.min(e2) - s1.max(s2)).max(0)
}

/// The report for one pay period
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub period: Period,
    /// Flagged in it, in order
    pub flags: Vec<Flag>,
    pub total: f64,
    pub confirmed: f64,
    pub unconfirmed: f64,
    /// Clocked in during it (milliseconds)
    pub worked: i64,
    /// The ROs worked on in it (flagged in it, or spiffed in it) that could be spiffed, and how many were
    pub spiffs: SpiffCount,
    pub is_current: bool,
}

/// Of the ROs worked on in a pay period: how many could be spiffed, how many were, and the ones that weren't
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpiffCount {
    pub eligible: usize,
    pub spiffed: usize,
    /// Could have been but weren't: RO, vehicle, what the recommendations came to
    pub missed: Vec<(String, String, f64)>,
    /// ROs whose recommendations the app hasn't read (from before it kept them): not counted
    pub unknown: usize,
}

impl Report {
    /// Flagged hours ÷ clocked hours, as a percentage
    pub fn efficiency(&self) -> Option<f64> {
        (self.worked > 0).then(|| self.total / (self.worked as f64 / 3_600_000.0) * 100.0)
    }
}

// ---------- what the app does with them ----------

impl Hours {
    /// The file; nothing (an empty start) when it's missing or can't be read
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_string(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, path)
    }

    /// Brings the Electron app's hours over, once: its ROs, flags and shifts (spiffs and anything already here stay).
    /// Returns how many flags came over; `None` when there was nothing to bring (or it's been done).
    pub fn import_electron(&mut self, electron_file: &Path) -> Option<usize> {
        if self.imported_from_electron {
            return None;
        }
        let old: Hours = serde_json::from_str(&std::fs::read_to_string(electron_file).ok()?).ok()?;
        self.imported_from_electron = true;
        let count = old.flags.len();
        for (ro, rec) in old.ros {
            self.ros.entry(ro).or_insert(rec);
        }
        // flags get new numbers after any already here, so none clash
        let mut next = self.next_flag_id.max(1);
        for mut f in old.flags {
            if self.flag_for(&f.ro, &f.key).is_some() {
                continue;
            }
            f.id = next;
            next += 1;
            self.flags.push(f);
        }
        self.next_flag_id = next;
        let mut shifts = old.shifts;
        shifts.extend(std::mem::take(&mut self.shifts));
        shifts.sort_by_key(|s| s.start);
        self.shifts = shifts;
        self.last_seen = self.last_seen.max(old.last_seen);
        self.last_closed = self.last_closed.max(old.last_closed);
        for (ro, o) in old.open_ros {
            self.open_ros.entry(ro).or_insert(o);
        }
        Some(count)
    }

    // ----- the shift clock -----

    pub fn open_shift(&self) -> Option<&Shift> {
        self.shifts.last().filter(|s| s.end.is_none())
    }

    pub fn clock_in(&mut self, now: i64) {
        if self.open_shift().is_none() {
            self.shifts.push(Shift {
                start: now,
                end: None,
                auto: false,
            });
        }
    }

    pub fn clock_out(&mut self, now: i64) {
        if let Some(s) = self.shifts.last_mut().filter(|s| s.end.is_none()) {
            s.end = Some(now.max(s.start));
        }
    }

    /// The app is running (kept every few minutes, for [`Hours::startup_check`])
    pub fn heartbeat(&mut self, now: i64) {
        self.last_seen = Some(now);
    }

    /// On start: a shift still open from an earlier day means you forgot to clock out, so it ends when the app was
    /// last seen running. Returns when it was ended.
    pub fn startup_check<Tz: TimeZone>(&mut self, tz: &Tz, now: i64) -> Option<i64> {
        let last = self
            .last_closed
            .unwrap_or(0)
            .max(self.last_seen.unwrap_or(0))
            .max(self.last_closed_all.unwrap_or(0));
        let s = self.shifts.last_mut().filter(|s| s.end.is_none())?;
        if last == 0 || start_of_day(tz, last) >= start_of_day(tz, now) {
            return None; // the same day: keep going
        }
        let end = s.start.max(last);
        s.end = Some(end);
        s.auto = true;
        Some(end)
    }

    pub fn worked_between(&self, from: i64, to: i64, now: i64) -> i64 {
        self.shifts.iter().map(|s| overlap(s.start, s.end.unwrap_or(now), from, to)).sum()
    }

    // ----- lines and flags -----

    /// An RO's lines as Xtime has them now (your hours and names for them stay)
    pub fn update_lines(&mut self, ro: &str, lines: Vec<TLine>, ymm: &str, now: i64) {
        let rec = self.ros.entry(ro.to_string()).or_default();
        rec.touched = Some(now);
        if !ymm.is_empty() {
            rec.ymm = ymm.to_string();
        }
        let manual: Vec<String> = rec
            .order
            .iter()
            .filter(|k| rec.lines.get(*k).is_some_and(|l| l.manual))
            .cloned()
            .collect();
        rec.order = lines.iter().map(|l| l.key.clone()).chain(manual).collect();
        for mut l in lines {
            if let Some(prev) = rec.lines.get(&l.key) {
                l.override_hours = prev.override_hours;
                l.name = prev.name.clone();
            }
            rec.lines.insert(l.key.clone(), l);
        }
        let keep: Vec<String> = rec.order.clone();
        rec.lines.retain(|k, _| keep.contains(k));
    }

    pub fn flag_for(&self, ro: &str, key: &str) -> Option<&Flag> {
        self.flags.iter().find(|f| f.ro == ro && f.key == key)
    }

    /// Flags a line (once): its paid hours, now
    pub fn flag_line(&mut self, ro: &str, key: &str, now: i64) -> bool {
        if self.flag_for(ro, key).is_some() {
            return false;
        }
        let Some(rec) = self.ros.get(ro) else { return false };
        let Some(line) = rec.lines.get(key) else { return false };
        let flag = Flag {
            id: self.next_flag_id.max(1),
            ro: ro.to_string(),
            key: key.to_string(),
            dms: line.dms.clone(),
            label: line.flag_label(),
            ymm: rec.ymm.clone(),
            hours: line.paid(),
            flagged_at: now,
            confirmed: false,
            other: Map::new(),
        };
        self.next_flag_id = flag.id + 1;
        self.flags.push(flag);
        true
    }

    pub fn unflag_line(&mut self, ro: &str, key: &str) {
        self.flags.retain(|f| !(f.ro == ro && f.key == key));
    }

    pub fn delete_flag(&mut self, id: u64) {
        self.flags.retain(|f| f.id != id);
    }

    pub fn set_confirmed(&mut self, id: u64, confirmed: bool) {
        if let Some(f) = self.flags.iter_mut().find(|f| f.id == id) {
            f.confirmed = confirmed;
        }
    }

    /// Changes a flag's hours (and the line's, so it shows the same)
    pub fn set_flag_hours(&mut self, id: u64, hours: f64) {
        if !hours.is_finite() || hours < 0.0 {
            return;
        }
        let Some(f) = self.flags.iter_mut().find(|f| f.id == id) else {
            return;
        };
        f.hours = round2(hours);
        let (ro, key, h) = (f.ro.clone(), f.key.clone(), f.hours);
        if let Some(line) = self.ros.get_mut(&ro).and_then(|r| r.lines.get_mut(&key)) {
            line.override_hours = if h == line.hours { None } else { Some(h) };
        }
    }

    /// Your hours for a line (`None`: back to Xtime's). An unconfirmed flag keeps in step with them.
    pub fn set_override(&mut self, ro: &str, key: &str, hours: Option<f64>) {
        let Some(line) = self.ros.get_mut(ro).and_then(|r| r.lines.get_mut(key)) else {
            return;
        };
        line.override_hours = hours.filter(|h| h.is_finite() && *h >= 0.0).map(round2);
        let paid = line.paid();
        if let Some(f) = self.flags.iter_mut().find(|f| f.ro == ro && f.key == key && !f.confirmed) {
            f.hours = paid;
        }
    }

    /// When a flag counts as flagged (it decides the pay period it's in)
    pub fn set_flagged_at(&mut self, id: u64, at: i64) {
        if let Some(f) = self.flags.iter_mut().find(|f| f.id == id) {
            f.flagged_at = at;
        }
    }

    /// An RO's lines that count, each with what it pays and whether it's flagged (the Electron app's `roSummary`)
    pub fn ro_summary(&self, ro: &str) -> Option<RoSummary> {
        let rec = self.ros.get(ro)?;
        let lines: Vec<LineRow> = rec
            .order
            .iter()
            .filter_map(|k| rec.lines.get(k))
            .filter(|l| l.counts())
            .map(|l| {
                let flag = self.flag_for(ro, &l.key);
                LineRow {
                    key: l.key.clone(),
                    dms: l.dms.clone(),
                    text: l.text().to_string(),
                    service: l.service.clone(),
                    original: if l.name.is_some() {
                        [&l.complaint, &l.service]
                            .into_iter()
                            .find(|s| !s.is_empty())
                            .cloned()
                            .unwrap_or_default()
                    } else {
                        String::new()
                    },
                    hours: l.hours,
                    override_hours: l.override_hours,
                    paid: flag.map_or(l.paid(), |f| f.hours),
                    primary: l.primary,
                    diag: l.diag,
                    status: if flag.is_some() {
                        LineStatus::Flagged
                    } else if l.completed {
                        LineStatus::Done
                    } else {
                        LineStatus::Todo
                    },
                    flag: flag.map(|f| FlagInfo {
                        id: f.id,
                        flagged_at: f.flagged_at,
                        confirmed: f.confirmed,
                    }),
                }
            })
            .collect();
        let sum = |pick: &dyn Fn(&LineRow) -> bool| round2(lines.iter().filter(|l| pick(l)).map(|l| l.paid).sum());
        Some(RoSummary {
            total: sum(&|_| true),
            flagged: sum(&|l| l.status == LineStatus::Flagged),
            flagged_count: lines.iter().filter(|l| l.status == LineStatus::Flagged).count(),
            lines,
        })
    }

    // ----- spiffs -----

    pub fn spiffed(&self, ro: &str) -> Option<&Spiff> {
        self.spiffs.get(ro)
    }

    pub fn set_spiffed(&mut self, ro: &str, spiff: Option<Spiff>) {
        match spiff {
            Some(s) => {
                self.spiffs.insert(ro.to_string(), s);
            }
            None => {
                self.spiffs.remove(ro);
            }
        }
    }

    // ----- the report -----

    /// Keeps what an RO's approved recommendations come to (when its lines are read)
    pub fn set_spiff_total(&mut self, ro: &str, total: f64) {
        self.ros.entry(ro.to_string()).or_default().spiff_total = Some(total);
    }

    /// The report for a pay period (`offset` 0: this one). `spiff_over`: recommendations over this earn a spiff.
    pub fn report<Tz: TimeZone>(&self, tz: &Tz, now: i64, offset: i32, spiff_over: f64) -> Report {
        let period = pay_period(tz, now, offset);
        let inside = |t: i64| t >= period.start && t < period.end;
        let mut flags: Vec<Flag> = self.flags.iter().filter(|f| inside(f.flagged_at)).cloned().collect();
        flags.sort_by(|a, b| a.flagged_at.cmp(&b.flagged_at).then(a.ro.cmp(&b.ro)).then(a.dms.cmp(&b.dms)));
        let total = round2(flags.iter().map(|f| f.hours).sum());
        let confirmed = round2(flags.iter().filter(|f| f.confirmed).map(|f| f.hours).sum());
        // the ROs worked on: flagged in the period, or spiffed in it; each once, in order
        let mut worked_on: Vec<&str> = Vec::new();
        let spiffed_in = self.spiffs.iter().filter(|(_, s)| inside(s.at)).map(|(r, _)| r.as_str());
        for ro in flags.iter().map(|f| f.ro.as_str()).chain(spiffed_in) {
            if !worked_on.contains(&ro) {
                worked_on.push(ro);
            }
        }
        let over = |t: f64| (t * 100.0).round() > (spiff_over * 100.0).round();
        let mut spiffs = SpiffCount::default();
        for ro in worked_on {
            let rec = self.ros.get(ro);
            if self.spiffs.contains_key(ro) {
                spiffs.eligible += 1;
                spiffs.spiffed += 1;
            } else {
                match rec.and_then(|r| r.spiff_total) {
                    Some(t) if over(t) => {
                        spiffs.eligible += 1;
                        spiffs
                            .missed
                            .push((ro.to_string(), rec.map(|r| r.ymm.clone()).unwrap_or_default(), t));
                    }
                    Some(_) => {}
                    None => spiffs.unknown += 1,
                }
            }
        }
        Report {
            period,
            total,
            confirmed,
            unconfirmed: round2(total - confirmed),
            worked: self.worked_between(period.start, period.end.min(now + 1), now),
            flags,
            spiffs,
            is_current: offset == 0,
        }
    }

    /// The ROs open now (for sync): whether that changed
    pub fn set_open_ros(&mut self, open: BTreeMap<String, OpenRo>) -> bool {
        if self.open_ros == open {
            return false;
        }
        self.open_ros = open;
        true
    }

    /// Keeps the file small: ROs with no flags, no spiff and no use for `keep_days` go; flags and shifts are kept
    /// 400 days. `keep`: ROs to keep anyway (open ones, your list).
    pub fn prune(&mut self, now: i64, keep_days: i64, keep: &[String]) {
        self.flags.retain(|f| f.flagged_at >= now - 400 * DAY);
        self.shifts.retain(|s| s.end.is_none_or(|e| e >= now - 400 * DAY));
        let flagged: Vec<String> = self.flags.iter().map(|f| f.ro.clone()).collect();
        let cutoff = now - keep_days * DAY;
        let spiffs = &self.spiffs;
        self.ros.retain(|ro, rec| {
            keep.contains(ro)
                || flagged.contains(ro)
                || spiffs.contains_key(ro)
                || (rec.review.as_ref().is_some_and(|r| !r.is_null()) && rec.completed_at.is_none())
                || rec.touched.unwrap_or(0).max(rec.completed_at.unwrap_or(0)) >= cutoff
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn ms(y: i32, m: u32, d: u32, h: u32) -> i64 {
        Utc.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap().timestamp_millis()
    }

    fn x(index: usize, dms: &str, primary: bool, hours: f64) -> XLine {
        XLine {
            index,
            dms: dms.into(),
            complaint: format!("concern {index}"),
            hours,
            approved: !primary,
            primary,
            ..Default::default()
        }
    }

    #[test]
    fn keys_lines_like_the_electron_app() {
        let keys: Vec<String> = line_keys(&[
            x(0, "A", true, 1.0),
            x(1, "B", false, 0.5),
            x(2, "B", true, 0.3),
            x(3, "", false, 2.0),
            x(4, "B", false, 1.0),
        ])
        .into_iter()
        .map(|l| l.key)
        .collect();
        assert_eq!(keys, ["A", "B:diag", "B", "B:2", "#4"]);
    }

    #[test]
    fn pay_periods_split_the_month() {
        let p = pay_period(&Utc, ms(2026, 11, 15, 12), 0);
        assert_eq!((p.start, p.end), (ms(2026, 11, 1, 0), ms(2026, 11, 16, 0)));
        let p = pay_period(&Utc, ms(2026, 11, 16, 0), 0);
        assert_eq!((p.start, p.end), (ms(2026, 11, 16, 0), ms(2026, 12, 1, 0)));
        let p = pay_period(&Utc, ms(2026, 12, 20, 0), 0);
        assert_eq!(p.end, ms(2027, 1, 1, 0));
        let p = pay_period(&Utc, ms(2026, 1, 3, 0), -1); // back over the new year
        assert_eq!((p.start, p.end), (ms(2025, 12, 16, 0), ms(2026, 1, 1, 0)));
    }

    #[test]
    fn flags_and_reports_a_period() {
        let mut h = Hours::default();
        let lines = line_keys(&[
            x(0, "A", true, 1.2),
            x(1, "B", false, 0.5),
            XLine {
                declined: true,
                ..x(2, "C", false, 3.0)
            },
        ]);
        assert!(lines.iter().find(|l| l.key == "C").is_some_and(|l| !l.counts()));
        h.update_lines("820367", lines, "2026 Ford Bronco", ms(2026, 11, 15, 8));
        assert!(h.flag_line("820367", "A", ms(2026, 11, 15, 9)));
        assert!(!h.flag_line("820367", "A", ms(2026, 11, 15, 9))); // once
        assert!(h.flag_line("820367", "B", ms(2026, 11, 16, 9))); // the next period
        let id = h.flag_for("820367", "A").unwrap().id;
        h.set_flag_hours(id, 1.5);
        h.set_confirmed(id, true);
        assert_eq!(h.ros["820367"].lines["A"].paid(), 1.5);
        h.clock_in(ms(2026, 11, 15, 8));
        h.clock_out(ms(2026, 11, 15, 10));
        h.set_spiffed(
            "820367",
            Some(Spiff {
                at: ms(2026, 11, 15, 11),
                total: 212.5,
                ..Default::default()
            }),
        );
        h.set_spiff_total("555", 300.0); // worked on, could be spiffed, wasn't
        h.update_lines("555", line_keys(&[x(0, "A", true, 1.0)]), "2024 Ford F-150", ms(2026, 11, 15, 8));
        h.flag_line("555", "A", ms(2026, 11, 15, 9));
        h.update_lines("556", line_keys(&[x(0, "A", true, 1.0)]), "", ms(2026, 11, 15, 8)); // never read for spiffs
        h.flag_line("556", "A", ms(2026, 11, 15, 9));
        let r = h.report(&Utc, ms(2026, 11, 15, 12), 0, 150.0);
        assert_eq!((r.total, r.confirmed, r.unconfirmed), (3.5, 1.5, 2.0));
        assert_eq!(r.flags.len(), 3);
        assert_eq!(r.worked, 2 * 3_600_000);
        assert_eq!(r.efficiency().map(|e| e.round()), Some(175.0));
        assert_eq!((r.spiffs.eligible, r.spiffs.spiffed, r.spiffs.unknown), (2, 1, 1));
        assert_eq!(r.spiffs.missed, vec![("555".to_string(), "2024 Ford F-150".to_string(), 300.0)]);
        // your hours stay when Xtime's lines come again
        h.update_lines("820367", line_keys(&[x(0, "A", true, 1.2)]), "", ms(2026, 11, 15, 13));
        assert_eq!(h.ros["820367"].lines["A"].paid(), 1.5);
        assert!(!h.ros["820367"].lines.contains_key("B"));
    }

    #[test]
    fn ends_a_forgotten_shift() {
        let mut h = Hours::default();
        h.clock_in(ms(2026, 11, 14, 8));
        h.heartbeat(ms(2026, 11, 14, 17));
        assert_eq!(h.startup_check(&Utc, ms(2026, 11, 15, 7)), Some(ms(2026, 11, 14, 17)));
        assert!(h.shifts[0].auto && h.open_shift().is_none());
        h.clock_in(ms(2026, 11, 15, 8));
        h.heartbeat(ms(2026, 11, 15, 9));
        assert_eq!(h.startup_check(&Utc, ms(2026, 11, 15, 10)), None); // the same day
    }

    #[test]
    fn reads_and_brings_over_the_electron_file() {
        let electron = r#"{
            "ros": { "820367": { "lines": { "A": { "key": "A", "dms": "A", "complaint": "Noise", "service": "",
                "hours": 1.2, "override": 1.5, "name": null, "approved": false, "primary": true, "declined": false,
                "completed": true } }, "order": ["A"], "ymm": "2026 Ford Bronco", "touched": 1, "onList": "77", "tag": "T1" } },
            "flags": [ { "id": 7, "ro": "820367", "key": "A", "dms": "A", "label": "Noise", "ymm": "2026 Ford Bronco",
                "hours": 1.5, "flaggedAt": 1763200000000, "confirmed": true } ],
            "shifts": [ { "start": 1763190000000, "end": 1763220000000 } ],
            "lastSeen": 5, "lastClosed": null, "nextFlagId": 8, "lastClosedAll": 9
        }"#;
        let dir = std::env::temp_dir().join(format!("hours-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("tracking.json");
        std::fs::write(&file, electron).unwrap();
        let mut h = Hours::default();
        h.set_spiffed(
            "1",
            Some(Spiff {
                total: 200.0,
                ..Default::default()
            }),
        );
        assert_eq!(h.import_electron(&file), Some(1));
        assert_eq!(h.import_electron(&file), None); // once
        assert_eq!(h.flags[0].hours, 1.5);
        assert!(h.flags[0].confirmed);
        assert_eq!(h.ros["820367"].lines["A"].paid(), 1.5);
        assert_eq!(h.ros["820367"].other.get("onList"), Some(&serde_json::json!("77"))); // kept
        assert_eq!(h.shifts.len(), 1);
        assert!(h.spiffed("1").is_some());
        // and it saves and loads back the same
        let saved = dir.join("hours.json");
        h.save(&saved).unwrap();
        assert_eq!(Hours::load(&saved), h);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn keeps_open_ros_as_the_electron_app_names_them() {
        let h: Hours =
            serde_json::from_str(r#"{"openROs":{"820367":{"ymm":"2023 Ford Bronco Sport"}},"openClosedBy":{"1":"app"}}"#).unwrap();
        assert_eq!(h.open_ros["820367"].ymm, "2023 Ford Bronco Sport");
        assert_eq!(h.open_closed_by["1"], "app");
        assert!(h.other.is_empty());
        let out = serde_json::to_value(&h).unwrap();
        assert_eq!(out["openROs"]["820367"]["ymm"], "2023 Ford Bronco Sport");
        let mut h = h;
        assert!(!h.set_open_ros(h.open_ros.clone()));
        assert!(h.set_open_ros(BTreeMap::new()));
    }

    #[test]
    fn sums_up_an_ros_lines() {
        let mut h = Hours::default();
        let line = |key: &str, hours: f64, completed: bool| TLine {
            key: key.into(),
            dms: key.into(),
            complaint: format!("Concern {key}"),
            hours,
            approved: true,
            completed,
            ..Default::default()
        };
        let declined = TLine {
            declined: true,
            ..line("C", 9.0, false)
        };
        h.update_lines(
            "1",
            vec![line("A", 1.5, false), line("B", 0.3, true), declined],
            "2024 Ford F-150",
            10,
        );
        h.flag_line("1", "A", 20);
        let s = h.ro_summary("1").unwrap();
        assert_eq!(s.lines.len(), 2); // the declined line doesn't count
        assert_eq!((s.lines[0].status, s.lines[1].status), (LineStatus::Flagged, LineStatus::Done));
        assert_eq!((s.total, s.flagged, s.flagged_count), (1.8, 1.5, 1));

        // your hours: an unconfirmed flag follows them; back to Xtime's with none
        h.set_override("1", "A", Some(2.0));
        assert_eq!(h.ro_summary("1").unwrap().lines[0].paid, 2.0);
        assert_eq!(h.ro_summary("1").unwrap().lines[0].override_hours, Some(2.0));
        h.set_override("1", "A", None);
        assert_eq!(h.ro_summary("1").unwrap().lines[0].paid, 1.5);
        // a confirmed flag keeps its hours
        let id = h.flag_for("1", "A").unwrap().id;
        h.set_confirmed(id, true);
        h.set_override("1", "A", Some(3.0));
        assert_eq!(h.flag_for("1", "A").unwrap().hours, 1.5);
        // when it was flagged
        h.set_flagged_at(id, 5);
        assert_eq!(h.ro_summary("1").unwrap().lines[0].flag.unwrap().flagged_at, 5);
        assert!(h.ro_summary("2").is_none());
    }
}
