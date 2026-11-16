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

    /// Extra arguments for the sign-in window: Xtime goes on from Scheduling to Inspect (that needs the store ID)
    fn extra_args(self, store: &str) -> Vec<String> {
        match self {
            Site::Xtime => vec![
                "--then".into(),
                xtime::signin::INSPECT.replace("{STORE}", &encode(store.trim())),
                "--then-from".into(),
                xtime::signin::LANDING.into(),
                "--done-cookie".into(),
            ],
            Site::Ford => vec![],
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

/// For a store ID in an address
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// Opens the sign-in window for a site and waits for it. `web_dir`: the web view's own profile folder (keeps
/// "remember me" and saved passwords between sign-ins). `store`: the Xtime store ID from settings.
pub async fn run(site: Site, web_dir: PathBuf, store: String) -> Outcome {
    let mut args = vec![
        "--title".to_string(),
        format!("Sign in to {}", site.name()),
        "--start".into(),
        site.start().to_string(),
        "--done-host".into(),
        site.done_host().to_string(),
        "--domains".into(),
        site.cookie_domains().join(","),
        "--data-dir".into(),
        web_dir.to_string_lossy().into_owned(),
    ];
    args.extend(site.extra_args(&store));
    window(args).await
}

/// PTS's vehicle page for a VIN (PTS's own address for opening a vehicle)
const PTS_VEHICLE: &str = "https://www.fordtechservice.dealerconnection.com/transport?link=704&params={VIN}";

/// Opens a vehicle in PTS once, unseen, so PTS's book pages have what its vehicle page sets up (as the Electron app
/// did). Done a few seconds after the vehicle page has loaded and set its PTSSession cookie; the window only shows
/// when PTS asks to sign in first. Its cookies come back as a sign-in's do.
pub async fn set_up_pts(vin: String, web_dir: PathBuf) -> Outcome {
    let args = vec![
        "--title".to_string(),
        "Opening the vehicle in PTS".into(),
        "--start".into(),
        PTS_VEHICLE.replace("{VIN}", &encode(vin.trim())),
        "--done-host".into(),
        Site::Ford.done_host().to_string(),
        "--domains".into(),
        Site::Ford.cookie_domains().join(","),
        "--data-dir".into(),
        web_dir.to_string_lossy().into_owned(),
        "--hidden".into(),
        "--timeout".into(),
        "40".into(),
        "--settle".into(),
        "1".into(),
        "--wait-cookie".into(),
        "PTSSession".into(),
    ];
    window(args).await
}

/// Runs the sign-in window's program with these arguments and reads its answer
async fn window(args: Vec<String>) -> Outcome {
    let out = tokio::process::Command::new(program())
        .args(&args)
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
