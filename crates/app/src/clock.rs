//! The shift clock: clocked in since when (or not). Saved as `clock.json` in the app's folder, so a restart doesn't
//! clock you out. (Hours and flags, the rest of the Electron app's tracker, come later.)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clock {
    /// When you clocked in (seconds since 1970); none when clocked out
    pub since: Option<i64>,
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn file() -> std::path::PathBuf {
    crate::settings::app_dir().join("clock.json")
}

impl Clock {
    pub fn load() -> Self {
        std::fs::read_to_string(file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let f = file();
        if let Some(dir) = f.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = f.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_string(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, f)
    }

    /// Clocks in, or out
    pub fn toggle(&mut self, now: i64) {
        self.since = match self.since {
            Some(_) => None,
            None => Some(now),
        };
    }

    /// "2h 05m" on the clock so far; none when clocked out
    pub fn elapsed(&self, now: i64) -> Option<String> {
        let mins = (now - self.since?).max(0) / 60;
        Some(format!("{}h {:02}m", mins / 60, mins % 60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_the_time_on_the_clock() {
        let mut c = Clock::default();
        assert_eq!(c.elapsed(1000), None);
        c.toggle(1000);
        assert_eq!(c.elapsed(1000 + 2 * 3600 + 5 * 60 + 30).as_deref(), Some("2h 05m"));
        c.toggle(9999);
        assert_eq!(c.since, None);
    }
}
