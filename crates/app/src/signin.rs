//! Signing in to Xtime and Ford PTS: runs the sign-in window (`pts-signin`, crates/signin) for a site and takes the
//! cookies it hands over into the profile's web requests (`workbench_net::Http`), which are then saved with the
//! profile so the sign-in lasts between starts.

use std::path::PathBuf;

use workbench_net::{SiteCookie, Url};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Site {
    Xtime,
    Ford,
}

impl Site {
    pub const ALL: [Site; 2] = [Site::Xtime, Site::Ford];

    pub fn name(self) -> &'static str {
        match self {
            Site::Xtime => "Xtime",
            Site::Ford => "Ford PTS",
        }
    }

    fn start(self) -> &'static str {
        match self {
            Site::Xtime => xtime::signin::START,
            Site::Ford => ford::signin::START,
        }
    }

    fn done_host(self) -> &'static str {
        match self {
            Site::Xtime => xtime::signin::DONE_HOST,
            Site::Ford => ford::signin::DONE_HOST,
        }
    }

    fn cookie_domains(self) -> &'static [&'static str] {
        match self {
            Site::Xtime => xtime::signin::COOKIE_DOMAINS,
            Site::Ford => ford::signin::COOKIE_DOMAINS,
        }
    }

    /// An address on the site, to tell whether there's a sign-in for it
    pub fn check_url(self) -> Url {
        Url::parse(self.start()).expect("sign-in address")
    }
}

/// The sign-in window's program: beside the app's own (installed together; `cargo` builds them into the same folder)
fn program() -> PathBuf {
    let name = if cfg!(windows) { "pts-signin.exe" } else { "pts-signin" };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

#[derive(serde::Deserialize)]
struct Answer {
    ok: bool,
    #[serde(default)]
    cancelled: bool,
    error: Option<String>,
    #[serde(default)]
    cookies: Vec<SiteCookie>,
}

#[derive(Debug, Clone)]
pub enum Outcome {
    SignedIn(Vec<SiteCookie>),
    /// The window was closed first
    Cancelled,
    Failed(String),
}

/// Opens the sign-in window for a site and waits for it. `web_dir`: the web view's own profile folder (keeps
/// "remember me" and saved passwords between sign-ins).
pub async fn run(site: Site, web_dir: PathBuf) -> Outcome {
    let out = tokio::process::Command::new(program())
        .arg("--title")
        .arg(format!("Sign in to {}", site.name()))
        .arg("--start")
        .arg(site.start())
        .arg("--done-host")
        .arg(site.done_host())
        .arg("--domains")
        .arg(site.cookie_domains().join(","))
        .arg("--data-dir")
        .arg(&web_dir)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await;
    let out = match out {
        Ok(o) => o,
        Err(e) => {
            let hint = if cfg!(debug_assertions) {
                " (build it with `cargo build --workspace`)"
            } else {
                ""
            };
            return Outcome::Failed(format!("couldn't open the sign-in window ({}): {e}{hint}", program().display()));
        }
    };
    let text = String::from_utf8_lossy(&out.stdout);
    match text.lines().rev().find_map(|l| serde_json::from_str::<Answer>(l).ok()) {
        Some(a) if a.ok => Outcome::SignedIn(a.cookies),
        Some(a) if a.cancelled => Outcome::Cancelled,
        Some(a) => Outcome::Failed(a.error.unwrap_or_else(|| "the sign-in didn't finish".into())),
        None => Outcome::Failed("the sign-in window closed without an answer".into()),
    }
}
