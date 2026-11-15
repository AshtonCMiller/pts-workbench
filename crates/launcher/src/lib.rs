//! Starting PTS Workbench, the Rust version of the Electron app's `boot.js`.
//!
//! The installer puts two programs side by side: the launcher (`pts-workbench`, what people start) and the app
//! (`pts-workbench-app`). Updates are new app versions downloaded into the app's folder, under `app-updates\<version>\`,
//! with `app-updates\current.json` naming the one to use. No installer and no admin rights are needed for them.
//! The launcher itself only changes with a full install, so it stays small and careful.
//!
//! What the launcher does each time:
//! 1. Picks the app to run ([`choose`]): the downloaded version named in `current.json` when it's newer than the
//!    installed one (or picked on purpose, to leave a beta), fits this launcher, isn't set aside, and is there.
//! 2. Counts the start. The app calls [`started_ok`] once its window is up, which clears the count. A downloaded
//!    version that doesn't get that far twice (or exits with an error before it) is set aside and the installed one
//!    is used.
//! 3. When the app exits with [`RESTART_CODE`] ("Restart to update"), picks again and starts the new version.
//!
//! This library is also used by the app, for [`started_ok`], [`app_dir`] and the update record.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The app exits with this to be started again (after an update was downloaded)
pub const RESTART_CODE: i32 = 75;

/// What this launcher can start. An update that needs a newer launcher says so in `current.json` and is left alone
/// until a full install brings one (like the Electron app's "shell").
pub const LAUNCHER_LEVEL: u32 = 1;

/// Set by the launcher for the app: the version it started
pub const ENV_VERSION: &str = "PTS_WORKBENCH_VERSION";

/// The installed version: the launcher and the app it's installed with share the workspace version
pub const INSTALLED_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The app's program file name
pub const APP_EXE: &str = if cfg!(windows) {
    "pts-workbench-app.exe"
} else {
    "pts-workbench-app"
};

/// The app's own folder: Windows `%APPDATA%\PTS Workbench`, Linux `$XDG_CONFIG_HOME/pts-workbench` (or `~/.config/…`)
pub fn app_dir() -> PathBuf {
    if cfg!(windows) {
        if let Some(d) = std::env::var_os("APPDATA") {
            return PathBuf::from(d).join("PTS Workbench");
        }
    }
    if let Some(d) = std::env::var_os("XDG_CONFIG_HOME").filter(|d| !d.is_empty()) {
        return PathBuf::from(d).join("pts-workbench");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join("pts-workbench");
    }
    PathBuf::from(".pts-workbench")
}

/// Where downloaded versions go
pub fn updates_dir() -> PathBuf {
    app_dir().join("app-updates")
}

/// `app-updates/current.json`: the downloaded version to use, and how it's been doing
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Current {
    pub version: String,
    /// Its folder in `app-updates` (holding the app's program)
    pub folder: String,
    /// The launcher level it needs
    pub launcher: u32,
    /// Starts that haven't reached [`started_ok`] yet
    pub tries: u32,
    /// Set aside: it didn't start (twice), so the installed version is used
    pub bad: bool,
    pub error: Option<String>,
    /// Picked on purpose although it's older (leaving a beta): only for the install it was picked on
    pub downgrade: bool,
    pub installed: Option<String>,
}

pub fn read_current(dir: &Path) -> Option<Current> {
    serde_json::from_str(&std::fs::read_to_string(dir.join("current.json")).ok()?).ok()
}

/// Best effort (whole or not at all): a failed write just means the count isn't kept
pub fn write_current(dir: &Path, c: &Current) {
    let file = dir.join("current.json");
    let tmp = dir.join("current.json.tmp");
    let _ = std::fs::create_dir_all(dir)
        .and_then(|_| std::fs::write(&tmp, serde_json::to_string_pretty(c).unwrap_or_default()))
        .and_then(|_| std::fs::rename(&tmp, &file));
}

/// 0.4.1-beta.57 < 0.4.1 < 0.4.2 (as `boot.js` compares them)
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    fn parse(v: &str) -> (Vec<u64>, Vec<&str>) {
        let (core, pre) = v.split_once('-').unwrap_or((v, ""));
        (
            core.split('.').map(|n| n.trim().parse().unwrap_or(0)).collect(),
            if pre.is_empty() { vec![] } else { pre.split('.').collect() },
        )
    }
    let (x, y) = (parse(a), parse(b));
    for i in 0..3 {
        let (p, q) = (x.0.get(i).copied().unwrap_or(0), y.0.get(i).copied().unwrap_or(0));
        if p != q {
            return p.cmp(&q);
        }
    }
    match (x.1.is_empty(), y.1.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater, // a release is newer than its betas
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for i in 0..x.1.len().max(y.1.len()) {
        let (p, q) = match (x.1.get(i), y.1.get(i)) {
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(p), Some(q)) => (*p, *q),
        };
        let (pn, qn) = (p.parse::<u64>().ok(), q.parse::<u64>().ok());
        let o = match (pn, qn) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => p.cmp(q),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}

/// Which app to start
#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    /// The one installed beside the launcher
    Installed { exe: PathBuf },
    /// A downloaded version (its start already counted in `current.json`)
    Downloaded { exe: PathBuf, version: String },
}

impl Choice {
    pub fn exe(&self) -> &Path {
        match self {
            Choice::Installed { exe } | Choice::Downloaded { exe, .. } => exe,
        }
    }
    pub fn version(&self) -> &str {
        match self {
            Choice::Installed { .. } => INSTALLED_VERSION,
            Choice::Downloaded { version, .. } => version,
        }
    }
}

/// Picks the app to start (see the top of this file), and counts the start of a downloaded one.
/// `installed_dir`: the launcher's own folder; `updates`: [`updates_dir`].
pub fn choose(installed_dir: &Path, updates: &Path) -> Choice {
    let installed = Choice::Installed {
        exe: installed_dir.join(APP_EXE),
    };
    let Some(mut cur) = read_current(updates) else { return installed };
    if cur.folder.is_empty() || cur.bad || cur.launcher > LAUNCHER_LEVEL {
        return installed;
    }
    let wanted = compare_versions(&cur.version, INSTALLED_VERSION) == Ordering::Greater
        || (cur.downgrade && cur.installed.as_deref() == Some(INSTALLED_VERSION));
    // only a plain folder name inside app-updates
    let folder = Path::new(&cur.folder).file_name().map(|f| updates.join(f));
    let exe = match folder {
        Some(f) if wanted => f.join(APP_EXE),
        _ => return installed,
    };
    if !exe.is_file() {
        return installed;
    }
    if cur.tries >= 2 {
        cur.bad = true;
        cur.error.get_or_insert_with(|| "didn't start twice".into());
        write_current(updates, &cur);
        return installed;
    }
    cur.tries += 1;
    write_current(updates, &cur);
    Choice::Downloaded { exe, version: cur.version }
}

/// Sets a downloaded version aside (it exited with an error before its window was up)
pub fn set_aside(updates: &Path, version: &str, why: &str) {
    if let Some(mut c) = read_current(updates).filter(|c| c.version == version) {
        c.bad = true;
        c.error = Some(why.chars().take(300).collect());
        write_current(updates, &c);
    }
}

/// Whether a downloaded version has reached [`started_ok`] this time
pub fn has_started(updates: &Path, version: &str) -> bool {
    read_current(updates).is_some_and(|c| c.version != version || c.tries == 0)
}

/// What the app prints when its window is up (the launcher closes its loading window then)
pub const READY_LINE: &str = "PTS-WORKBENCH-READY";

/// Called by the app once its window is up: tells the launcher (which closes its loading window), and marks this
/// version as one that starts fine. Only the first call does anything.
pub fn window_ready() {
    static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    use std::io::Write;
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{READY_LINE}"); // nothing to write to (started on its own): fine
    let _ = out.flush();
    started_ok();
}

/// This version works, so the launcher keeps using it (part of [`window_ready`])
pub fn started_ok() {
    let Ok(version) = std::env::var(ENV_VERSION) else { return }; // not started by the launcher (development)
    let dir = updates_dir();
    if let Some(mut c) = read_current(&dir).filter(|c| c.version == version && c.tries > 0) {
        c.tries = 0;
        write_current(&dir, &c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_like_boot_js() {
        use Ordering::*;
        assert_eq!(compare_versions("0.4.1-beta.57", "0.4.1"), Less);
        assert_eq!(compare_versions("0.4.1", "0.4.2"), Less);
        assert_eq!(compare_versions("0.4.10", "0.4.9"), Greater);
        assert_eq!(compare_versions("1.0.0-beta.2", "1.0.0-beta.10"), Less);
        assert_eq!(compare_versions("1.0.0-alpha", "1.0.0-beta"), Less);
        assert_eq!(compare_versions("1.0.0-beta", "1.0.0-beta.1"), Less);
        assert_eq!(compare_versions("1.0", "1.0.0"), Equal);
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pts-launcher-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn picks_a_newer_download_and_sets_it_aside_after_two_failed_starts() {
        let (installed, updates) = (temp("inst"), temp("upd"));
        let folder = updates.join("9.9.9");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(APP_EXE), b"").unwrap();
        write_current(
            &updates,
            &Current {
                version: "9.9.9".into(),
                folder: "9.9.9".into(),
                launcher: 1,
                ..Default::default()
            },
        );

        assert_eq!(
            choose(&installed, &updates),
            Choice::Downloaded {
                exe: folder.join(APP_EXE),
                version: "9.9.9".into()
            }
        );
        assert!(!has_started(&updates, "9.9.9"));
        assert!(matches!(choose(&installed, &updates), Choice::Downloaded { .. })); // second try
        assert!(matches!(choose(&installed, &updates), Choice::Installed { .. })); // two without starting: set aside
        assert!(read_current(&updates).unwrap().bad);
    }

    #[test]
    fn leaves_older_unfit_or_missing_versions_alone() {
        let (installed, updates) = (temp("inst2"), temp("upd2"));
        let installed_choice = Choice::Installed {
            exe: installed.join(APP_EXE),
        };
        // older than installed
        write_current(
            &updates,
            &Current {
                version: "0.0.1".into(),
                folder: "0.0.1".into(),
                ..Default::default()
            },
        );
        assert_eq!(choose(&installed, &updates), installed_choice);
        // needs a newer launcher
        write_current(
            &updates,
            &Current {
                version: "9.9.9".into(),
                folder: "9.9.9".into(),
                launcher: LAUNCHER_LEVEL + 1,
                ..Default::default()
            },
        );
        assert_eq!(choose(&installed, &updates), installed_choice);
        // not downloaded (no program in its folder)
        write_current(
            &updates,
            &Current {
                version: "9.9.9".into(),
                folder: "9.9.9".into(),
                ..Default::default()
            },
        );
        assert_eq!(choose(&installed, &updates), installed_choice);
        // a folder name that tries to leave app-updates stays inside it
        write_current(
            &updates,
            &Current {
                version: "9.9.9".into(),
                folder: "../../elsewhere".into(),
                ..Default::default()
            },
        );
        assert_eq!(choose(&installed, &updates), installed_choice);
    }
}
