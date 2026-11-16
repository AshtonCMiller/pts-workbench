//! Sync in the app (crates/sync, the Electron app's sync): on unless turned off in Settings. A few seconds after
//! your hours change, and every minute, this computer's hours are merged with your other computers' (on either app)
//! through the sync server, encrypted. ROs opened on another computer open here too once their VIN is known (from
//! your ROs in Xtime: the VIN itself never syncs); ROs closed on another computer close here. Also: what sync's
//! doing, for Settings, and this computer's side of it (`sync-state.json`).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub use workbench_sync::{Outcome, State, URL, derive_keys};

/// How long after a change it's sent (changes in a row go together)
pub const SOON: Duration = Duration::from_secs(3);
/// How often it syncs anyway (seconds)
pub const EVERY: i64 = 60;

fn file() -> PathBuf {
    crate::settings::app_dir().join("sync-state.json")
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Off,
    Syncing,
    /// When (ms)
    Synced(i64),
    Failed(String),
}

#[derive(Debug)]
pub struct Syncing {
    pub state: State,
    pub status: Status,
    pub running: bool,
    /// Asked again while syncing: once more after
    pub again: bool,
    /// Counts changes to the hours here (a sync whose hours changed meanwhile isn't used: it goes again)
    pub generation: u64,
    pub started_at: u64,
    /// When the next sync after a change is due
    pub due: Option<Instant>,
    /// When it last synced (seconds since 1970)
    pub last_run: Option<i64>,
    /// ROs opened on another computer waiting for their VIN here (RO → year / make / model)
    pub waiting: BTreeMap<String, String>,
    /// "Forgot to clock out" waits for the first sync (another computer may have clocked out)
    pub check_after_first: bool,
}

impl Syncing {
    pub fn load(on: bool) -> Self {
        let state = std::fs::read_to_string(file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Syncing {
            state,
            status: if on { Status::Syncing } else { Status::Off },
            running: false,
            again: false,
            generation: 0,
            started_at: 0,
            due: None,
            last_run: None,
            waiting: BTreeMap::new(),
            check_after_first: on,
        }
    }

    pub fn save(&self) {
        let f = file();
        let tmp = f.with_extension("json.tmp");
        if let Ok(t) = serde_json::to_string(&self.state) {
            let _ = std::fs::write(&tmp, t).and_then(|_| std::fs::rename(&tmp, &f));
        }
    }

    /// The hours changed here: sent soon
    pub fn changed(&mut self) {
        self.generation += 1;
        self.due = Some(Instant::now() + SOON);
    }

    /// What it's doing, in words (Settings)
    pub fn describe(&self, now_ms: i64) -> String {
        match &self.status {
            Status::Off => "Off: your hours stay on this computer".into(),
            Status::Syncing => "Syncing…".into(),
            Status::Synced(at) => {
                let mins = (now_ms - at).max(0) / 60_000;
                match mins {
                    0 => "Synced just now".into(),
                    1 => "Synced a minute ago".into(),
                    m if m < 60 => format!("Synced {m} minutes ago"),
                    _ => "Synced over an hour ago".into(),
                }
            }
            Status::Failed(e) => format!("Couldn't sync: {e}"),
        }
    }
}
