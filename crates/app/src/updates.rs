//! The app updating itself (the Electron app's `app/updates.js`), from the repository's GitHub releases.
//!
//! Each release built by `.github/workflows/release.yml` has `workbench.json`: its version, the launcher level it
//! needs, and each of its files with its size and SHA-256 (the app, the sign-in window, the PDF engine, the
//! launcher), plus the installer. Checking: shortly after the app starts and every 30 minutes, the newest release
//! (`releases/latest`, which never holds betas), or with **Beta updates** on, the newest of all (the releases feed,
//! betas included). A newer version is downloaded into the launcher's `versions\<version>\` folder (files the same as
//! ones already here are copied, not downloaded), each file checked, and `versions\current.json` pointed at it; the
//! launcher starts it on "Restart to update" (or the next start). One that needs a newer launcher downloads the
//! installer instead, which "Restart to update" runs (silently: it puts everything in place and starts the app).
//!
//! The Beta updates choice is `updates.json` in the app's folder, the Electron app's file: a computer that had it on
//! keeps it on. Leaving beta goes back to the newest regular version, even though it's "older".
//!
//! Only on Windows, and only when started by the launcher (not while developing).

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use launcher::{Current, ENV_HOME, ENV_INSTALLED, ENV_VERSION, LAUNCHER_LEVEL, compare_versions, updates_dir, write_current};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use workbench_net::{Http, Method, Url};

/// Where the releases are
pub const REPO: &str = "AshtonCMiller/pts-workbench";
/// Each release's list of its files
pub const MANIFEST: &str = "workbench.json";
/// How often to look (seconds)
const CHECK_EVERY: i64 = 30 * 60;

/// A release's `workbench.json`
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub version: String,
    /// The launcher it needs ([`LAUNCHER_LEVEL`]): newer than this one's, and only the installer will do
    pub launcher: u32,
    /// The program files, each to sit in the version's folder
    pub files: Vec<FileInfo>,
    /// The installer (for a new launcher)
    pub setup: Option<FileInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
    /// Lowercase hex
    pub sha256: String,
}

/// `updates.json` (shared with the Electron app)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Prefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    beta: Option<bool>,
}

fn prefs_file() -> PathBuf {
    launcher::app_dir().join("updates.json")
}

/// A newer version found: its release's tag, what's in it, and whether it's "older" (leaving beta)
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub tag: String,
    pub manifest: Manifest,
    pub downgrade: bool,
}

/// A version downloaded: ready for "Restart to update" (its installer, when it needs one)
#[derive(Debug, Clone, PartialEq)]
pub struct Ready {
    pub version: String,
    pub installer: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    /// Updates can't happen here (why)
    Off(String),
    Idle,
    Checking,
    /// Up to date: the newest version seen
    Current(Option<String>),
    Downloading(String),
    Ready(Ready),
    Failed(String),
}

/// What the app keeps about updates
#[derive(Debug, Clone)]
pub struct Updates {
    pub state: State,
    pub beta: bool,
    /// The version running
    pub running: String,
    /// When it last looked (seconds since 1970)
    pub checked_at: Option<i64>,
}

impl Default for Updates {
    fn default() -> Self {
        Self::new()
    }
}

impl Updates {
    pub fn new() -> Self {
        let launched = std::env::var(ENV_VERSION).ok().filter(|v| !v.is_empty());
        let running = launched.clone().unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
        let state = if !cfg!(windows) {
            State::Off("Updates come with the Windows app".into())
        } else if launched.is_none() {
            State::Off("Started without the launcher (while developing), so updates are off".into())
        } else {
            State::Idle
        };
        // no choice saved: a beta keeps getting betas (it was installed on purpose), a regular version regular ones
        let saved: Prefs = std::fs::read_to_string(prefs_file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let beta = saved.beta.unwrap_or(running.contains('-'));
        Self {
            state,
            beta,
            running,
            checked_at: None,
        }
    }

    pub fn on(&self) -> bool {
        !matches!(self.state, State::Off(_))
    }

    /// Time to look again (not while looking or downloading; not once something's waiting for a restart)
    pub fn due(&self, now: i64) -> bool {
        matches!(self.state, State::Idle | State::Current(_) | State::Failed(_)) && self.checked_at.is_none_or(|t| now - t >= CHECK_EVERY)
    }

    pub fn busy(&self) -> bool {
        matches!(self.state, State::Checking | State::Downloading(_))
    }

    /// Beta updates on or off, kept in `updates.json` (keeping anything else the Electron app wrote there)
    pub fn set_beta(&mut self, on: bool) {
        self.beta = on;
        let file = prefs_file();
        let mut v: serde_json::Value = std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .filter(|v: &serde_json::Value| v.is_object())
            .unwrap_or_else(|| serde_json::json!({}));
        v["beta"] = serde_json::Value::Bool(on);
        if let Some(d) = file.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(&file, serde_json::to_string_pretty(&v).unwrap_or_default());
        // (a version waiting for a restart may be the wrong kind now: look again)
        if matches!(self.state, State::Ready(_)) {
            self.state = State::Idle;
        }
        self.checked_at = None;
    }

    /// What the state says, in words (Settings)
    pub fn describe(&self) -> String {
        match &self.state {
            State::Off(why) => why.clone(),
            State::Idle => "Checks for updates shortly after the app starts".into(),
            State::Checking => "Checking for updates…".into(),
            State::Current(Some(latest)) if compare_versions(latest, &self.running) == Ordering::Greater => {
                format!("v{latest} is out, but couldn't be used here")
            }
            State::Current(_) => "Up to date".into(),
            State::Downloading(v) => format!("Downloading v{v}…"),
            State::Ready(r) => format!("v{} is ready: restart to update", r.version),
            State::Failed(e) => format!("Couldn't update: {e}"),
        }
    }
}

fn releases() -> String {
    format!("https://github.com/{REPO}/releases")
}

fn url(s: &str) -> Result<Url, String> {
    Url::parse(s).map_err(|_| format!("bad address {s}"))
}

/// The release tags in GitHub's releases feed (`…/releases.atom`), newest first, each once
pub fn feed_tags(xml: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find("/releases/tag/") {
        rest = &rest[at + "/releases/tag/".len()..];
        let end = rest.find(['"', '<', '\'', ' ']).unwrap_or(rest.len());
        let tag = rest[..end].replace("%2B", "+");
        if !tag.is_empty() && !out.contains(&tag) {
            out.push(tag);
        }
    }
    out
}

/// A tag's version ("v0.5.0-beta.3" → "0.5.0-beta.3")
fn version_of(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

/// Which of the beta channel's tags to try, best first: newer than what's running
pub fn beta_candidates(tags: &[String], running: &str) -> Vec<String> {
    let mut t: Vec<String> = tags
        .iter()
        .filter(|t| compare_versions(version_of(t), running) == Ordering::Greater)
        .cloned()
        .collect();
    t.sort_by(|a, b| compare_versions(version_of(b), version_of(a)));
    t
}

/// Whether the newest regular version should be taken: newer, or leaving a beta (as long as it's another version)
pub fn take_regular(latest: &str, running: &str) -> Option<bool> {
    let newer = compare_versions(latest, running) == Ordering::Greater;
    let leaving_beta = running.contains('-') && latest != running;
    (newer || leaving_beta).then_some(!newer)
}

/// A release's manifest (none when the release has none: an Electron release, or still being uploaded)
async fn manifest_at(http: &Http, address: &str) -> Result<Option<Manifest>, String> {
    let res = http
        .send(Http::request(Method::GET, url(address)?).header("Accept", "application/json, */*"))
        .await
        .map_err(|e| format!("couldn't reach GitHub ({e})"))?;
    if res.status.as_u16() == 404 {
        return Ok(None);
    }
    if !res.ok() {
        return Err(format!("GitHub answered {}", res.status.as_u16()));
    }
    serde_json::from_str(&res.body)
        .map(Some)
        .map_err(|e| format!("its file list couldn't be read ({e})"))
}

/// Looks for a newer version (see the top of this file)
pub async fn check(http: Http, beta: bool, running: String) -> Result<Option<Found>, String> {
    let base = releases();
    if beta {
        let feed = http
            .send(Http::request(Method::GET, url(&format!("{base}.atom"))?).header("Accept", "application/atom+xml, */*"))
            .await
            .map_err(|e| format!("couldn't reach GitHub ({e})"))?;
        if !feed.ok() {
            return Err(format!("GitHub answered {}", feed.status.as_u16()));
        }
        // the newest first; releases without a file list (the Electron app's) are passed over
        for tag in beta_candidates(&feed_tags(&feed.body), &running).into_iter().take(8) {
            if let Some(m) = manifest_at(&http, &format!("{base}/download/{tag}/{MANIFEST}")).await? {
                return Ok(Some(Found {
                    tag,
                    manifest: m,
                    downgrade: false,
                }));
            }
        }
        return Ok(None);
    }
    let Some(m) = manifest_at(&http, &format!("{base}/latest/download/{MANIFEST}")).await? else {
        return Ok(None);
    };
    Ok(take_regular(&m.version, &running).map(|downgrade| Found {
        tag: format!("v{}", m.version),
        manifest: m,
        downgrade,
    }))
}

/// A file's SHA-256, in lowercase hex (none when it can't be read)
fn sha256_of(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(hex(&Sha256::digest(&bytes)))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Only a plain file or folder name (nothing that leaves the folder it's meant for)
fn plain_name(s: &str) -> Option<String> {
    let n = Path::new(s).file_name()?.to_str()?.to_string();
    (!n.is_empty() && n != "." && n != ".." && n == s).then_some(n)
}

/// Fetches one of a release's files, checked against its size and SHA-256, into `dest` (whole or not at all)
async fn fetch_file(http: &Http, tag: &str, f: &FileInfo, dest: &Path) -> Result<(), String> {
    let address = format!("{}/download/{tag}/{}", releases(), f.name);
    let res = http
        .send_bytes(Http::request(Method::GET, url(&address)?))
        .await
        .map_err(|e| format!("couldn't download {} ({e})", f.name))?;
    if !res.status.is_success() {
        return Err(format!("{} wasn't there (GitHub answered {})", f.name, res.status.as_u16()));
    }
    if res.bytes.len() as u64 != f.size {
        return Err(format!("{} didn't download whole", f.name));
    }
    if hex(&Sha256::digest(&res.bytes)) != f.sha256.to_ascii_lowercase() {
        return Err(format!("{} doesn't match its checksum", f.name));
    }
    let part = dest.with_extension("part");
    std::fs::write(&part, &res.bytes).map_err(|e| format!("couldn't save {} ({e})", f.name))?;
    let _ = std::fs::remove_file(dest);
    std::fs::rename(&part, dest).map_err(|e| format!("couldn't save {} ({e})", f.name))
}

/// Downloads a version found into `versions\<version>\` and points `current.json` at it; or, when it needs a newer
/// launcher, downloads its installer
pub async fn download(http: Http, found: Found) -> Result<Ready, String> {
    let m = &found.manifest;
    let dir = updates_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't make the updates folder ({e})"))?;
    let version = m.version.clone();
    let folder = plain_name(&version.replace(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-'), "_"))
        .ok_or("the release's version isn't usable")?;

    // a new launcher: only the installer brings one
    if m.launcher > LAUNCHER_LEVEL {
        let setup = m
            .setup
            .as_ref()
            .ok_or("it needs a full install, but its release has no installer")?;
        let name = plain_name(&setup.name).ok_or("its installer's name isn't usable")?;
        let dest = dir.join(&name);
        if sha256_of(&dest).as_deref() != Some(setup.sha256.to_ascii_lowercase().as_str()) {
            fetch_file(&http, &found.tag, setup, &dest).await?;
        }
        return Ok(Ready {
            version,
            installer: Some(dest),
        });
    }

    let target = dir.join(&folder);
    std::fs::create_dir_all(&target).map_err(|e| format!("couldn't make the version's folder ({e})"))?;
    // files the same as ones already here (the running app's, the installed ones) are copied, not downloaded
    let here: Vec<PathBuf> = [
        std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)),
        std::env::var_os(ENV_HOME).map(PathBuf::from),
    ]
    .into_iter()
    .flatten()
    .collect();
    for f in &m.files {
        let name = plain_name(&f.name).ok_or_else(|| format!("a file name that isn't usable ({})", f.name))?;
        let want = f.sha256.to_ascii_lowercase();
        let dest = target.join(&name);
        if sha256_of(&dest).as_deref() == Some(want.as_str()) {
            continue;
        }
        let copied = here.iter().map(|d| d.join(&name)).any(|src| {
            sha256_of(&src).as_deref() == Some(want.as_str())
                && std::fs::copy(&src, &dest).is_ok()
                && sha256_of(&dest).as_deref() == Some(want.as_str())
        });
        if !copied {
            fetch_file(&http, &found.tag, f, &dest).await?;
        }
    }
    if !target.join(launcher::APP_EXE).is_file() {
        return Err("the release doesn't have the app in it".into());
    }
    let installed = std::env::var(ENV_INSTALLED).unwrap_or_else(|_| launcher::INSTALLED_VERSION.to_string());
    write_current(
        &dir,
        &Current {
            version: version.clone(),
            folder: folder.clone(),
            launcher: m.launcher,
            tries: 0,
            bad: false,
            error: None,
            downgrade: found.downgrade,
            installed: Some(installed),
        },
    );
    // tidy up: the new version and the one running stay; older downloads go (not while they're running)
    let running_folder = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            let keep = p == target || Some(&p) == running_folder.as_ref() || e.file_name() == "current.json";
            if !keep {
                let _ = if p.is_dir() {
                    std::fs::remove_dir_all(&p)
                } else {
                    std::fs::remove_file(&p)
                };
            }
        }
    }
    Ok(Ready { version, installer: None })
}

/// The code the app exits with once its window is closed: set for "Restart to update"
static EXIT_CODE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// After the app's window closed: the code to exit with
pub fn exit_code() -> i32 {
    EXIT_CODE.load(std::sync::atomic::Ordering::SeqCst)
}

/// "Restart to update": an installer is started (it puts everything in place and starts the app again); otherwise
/// the app closes for the launcher to start the new version. Tells the app to close (or why it can't).
pub fn restart(ready: &Ready) -> Result<(), String> {
    match &ready.installer {
        Some(setup) => {
            std::process::Command::new(setup)
                .arg("/S")
                .spawn()
                .map_err(|e| format!("couldn't start the installer ({e})"))?;
            EXIT_CODE.store(0, std::sync::atomic::Ordering::SeqCst);
        }
        None => EXIT_CODE.store(launcher::RESTART_CODE, std::sync::atomic::Ordering::SeqCst),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_releases_feed() {
        let xml = r#"<feed><entry><id>tag:github.com,2008:Repository/1/v0.5.0-beta.3</id>
            <link rel="alternate" type="text/html" href="https://github.com/AshtonCMiller/pts-workbench/releases/tag/v0.5.0-beta.3"/>
            </entry><entry><link rel="alternate" type="text/html" href="https://github.com/AshtonCMiller/pts-workbench/releases/tag/v0.4.7-beta.42"/></entry>
            <entry><link rel="alternate" href="https://github.com/AshtonCMiller/pts-workbench/releases/tag/v0.4.6"/></entry>
            <entry><link rel="alternate" href="https://github.com/AshtonCMiller/pts-workbench/releases/tag/v0.5.0-beta.3"/></entry></feed>"#;
        let tags = feed_tags(xml);
        assert_eq!(tags, ["v0.5.0-beta.3", "v0.4.7-beta.42", "v0.4.6"]);
        // only newer ones, newest first
        assert_eq!(beta_candidates(&tags, "0.4.6"), ["v0.5.0-beta.3", "v0.4.7-beta.42"]);
        assert_eq!(beta_candidates(&tags, "0.5.0-beta.3"), Vec::<String>::new());
    }

    #[test]
    fn regular_versions() {
        assert_eq!(take_regular("0.5.1", "0.5.0"), Some(false));
        assert_eq!(take_regular("0.5.0", "0.5.0"), None);
        assert_eq!(take_regular("0.4.9", "0.5.0"), None);
        // leaving beta: back to the newest regular version, though it's older
        assert_eq!(take_regular("0.5.0", "0.5.1-beta.4"), Some(true));
        assert_eq!(take_regular("0.5.1", "0.5.1-beta.4"), Some(false));
    }

    #[test]
    fn names_stay_inside_their_folder() {
        assert_eq!(plain_name("pts-workbench-app.exe").as_deref(), Some("pts-workbench-app.exe"));
        assert_eq!(plain_name("../x.exe"), None);
        assert_eq!(plain_name("a\\..\\x"), if cfg!(windows) { None } else { Some("a\\..\\x".into()) });
        assert_eq!(plain_name(".."), None);
    }

    #[test]
    fn reads_a_manifest() {
        let m: Manifest = serde_json::from_str(
            r#"{"version":"0.5.0-beta.3","launcher":1,"files":[{"name":"pts-workbench-app.exe","size":3,"sha256":"ab"}],
                "setup":{"name":"PTS-Workbench-Setup-0.5.0-beta.3.exe","size":9,"sha256":"cd"},"extra":true}"#,
        )
        .unwrap();
        assert_eq!(m.files[0].name, "pts-workbench-app.exe");
        assert_eq!(m.setup.unwrap().size, 9);
    }
}
