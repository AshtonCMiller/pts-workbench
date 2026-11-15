//! Your hours (crates/hours) in the app: kept as `hours.json` in the app's folder, only on this computer. The first
//! time, the Electron app's hours (`tracking.json` in the same folder) are brought over, and the clock the app used to
//! keep on its own (`clock.json`) joins them. Also: an RO's lines from Xtime, as the hours see them.

use std::path::PathBuf;

use hours::{Hours, TLine, XLine, line_keys_indexed};
use xtime::ro::RoView;

fn file() -> PathBuf {
    crate::settings::app_dir().join("hours.json")
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// What happened when the hours were opened (for the bottom bar)
pub struct Opened {
    pub hours: Hours,
    /// Flags brought over from the Electron app
    pub imported: Option<usize>,
    /// A shift left open on an earlier day was ended (when)
    pub auto_clocked_out: Option<i64>,
}

/// The hours: brought over from the Electron app the first time, a forgotten clock-out ended, old ROs dropped
pub fn open() -> Opened {
    let dir = crate::settings::app_dir();
    let mut h = Hours::load(&file());
    let imported = h.import_electron(&dir.join("tracking.json"));
    // the clock this app kept before (clock.json: { since: seconds }) becomes a shift
    let old_clock = dir.join("clock.json");
    if let Some(since) = std::fs::read_to_string(&old_clock)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("since").and_then(serde_json::Value::as_i64))
    {
        h.clock_in(since * 1000);
    }
    let now = now_ms();
    let auto_clocked_out = h.startup_check(&chrono::Local, now);
    h.prune(now, 60, &[]);
    h.heartbeat(now);
    let _ = save(&h);
    let _ = std::fs::rename(&old_clock, dir.join("clock.json.old")); // only once it's in the hours
    Opened {
        hours: h,
        imported,
        auto_clocked_out,
    }
}

pub fn save(h: &Hours) -> std::io::Result<()> {
    h.save(&file())
}

/// "2h 05m" on the clock so far today's shift; none when clocked out
pub fn on_the_clock(h: &Hours, now: i64) -> Option<String> {
    let s = h.open_shift()?;
    let mins = (now - s.start).max(0) / 60_000;
    Some(format!("{}h {:02}m", mins / 60, mins % 60))
}

/// An RO's lines as the hours see them (keyed as the Electron app keys them; the inspection lines left out), and for
/// each, the Xtime line it came from (by line ID)
pub fn lines_of(v: &RoView) -> Vec<(String, TLine)> {
    // in Xtime's own order (as the Electron app read them), not the RO page's
    let mut in_order: Vec<&xtime::ro::Line> = v.lines.iter().filter(|l| !l.is_inspection()).collect();
    in_order.sort_by_key(|l| l.index);
    let x: Vec<(String, XLine)> = in_order
        .into_iter()
        .map(|l| {
            (
                l.key(),
                XLine {
                    index: l.index,
                    dms: l.dms.clone(),
                    complaint: l.concern.clone(),
                    service: l.name.clone(),
                    hours: l.hours.unwrap_or(0.0),
                    approved: l.approved,
                    declined: l.declined,
                    completed: l.completed,
                    primary: l.primary,
                },
            )
        })
        .collect();
    let keyed = line_keys_indexed(&x.iter().map(|(_, l)| l.clone()).collect::<Vec<_>>());
    keyed.into_iter().map(|(i, t)| (x[i].0.clone(), t)).collect()
}

/// The RO's vehicle: "2026 Ford Bronco"
pub fn ymm(v: &RoView) -> String {
    [v.vehicle.year.as_str(), v.vehicle.make.as_str(), v.vehicle.model.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}
