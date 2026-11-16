//! What you see of Ford PTS in PTS Workbench (the data comes from the `ford` crate; the look from `ui-kit`): the
//! Service Manual's tabs, one module each.
//!
//!   oasis.rs     OASIS: the vehicle's information, field service actions (each opens its document), warnings and
//!                warranty coverage
//!   doc_tab.rs   a tab of its own for one of Ford's documents (an FSA: its dealer bulletin, instructions…)
//!   pdf.rs       PDFs drawn as pages (with PDFium, when it's beside the app)
//!   tsb.rs       TSBs: the VIN's TSBs, GSBs and SSMs, with a filter, categories and kinds
//!   article.rs   one of Ford's documents opened (a bulletin, an FSA's document), or its PDF
//!   manual.rs    the Workshop Manual: contents, search, procedures as tabs
//!   document.rs  Ford's documents drawn in the app's style (headings, paragraphs, lists, tables, pictures)
//!
//! Each tab has its own state, messages and view, like crates/xtime-ui's parts. What only the app can do (copying,
//! signing in to PTS, the bottom bar's notice) goes back to it as [`Out`]s.

pub mod article;
pub mod diagnostics;
pub mod doc_tab;
pub mod document;
pub mod labor;
pub mod manual;
pub mod oasis;
pub mod pdf;
pub mod pinpoint;
pub mod tsb;

pub use diagnostics::{DiagMsg, DiagView};
pub use doc_tab::{DocMsg as DocTabMsg, DocTab};
pub use labor::{LaborMsg, LaborView};
pub use manual::{ManualMsg, ManualView};
pub use oasis::{OasisMsg, OasisView};
pub use tsb::{TsbMsg, TsbView};

/// What a tab asks Ford with
#[derive(Clone)]
pub struct Env {
    pub ford: ford::Ford,
    /// The app is showing its sample ROs (their VINs aren't real: nothing is asked)
    pub sample: bool,
}

/// What only the app can do
#[derive(Debug, Clone)]
pub enum Out {
    /// Copy this (the text, and what it is: "VIN", "FSA")
    Copy(String, String),
    /// Ford's sign-in has run out: open the PTS sign-in window
    SignIn,
    /// Open one of Ford's documents (an FSA from OASIS: its page and its PDFs) in a Service Manual tab of its own.
    /// `kind`: "FSA", "Notice"; `id`: its number; `title`: what it's about.
    OpenDoc {
        kind: String,
        id: String,
        title: String,
        url: String,
    },
    /// PTS needs to have shown this vehicle (VIN) once: open it in PTS, unseen (the tab hears back with
    /// [`crate::oasis::OasisMsg::SetUp`])
    SetUpPts(String),
    /// Open a Workshop Manual procedure (a pinpoint test's link: at the test) in the Workshop Manual tab
    OpenProcedure { link: ford::doc::ProcLink, title: String },
    /// Print the Labor Times tray (with its one-time use parts), or a parts list, in the app's Print window
    Print(labor::LaborPrint),
    /// Open a bulletin (`kind`: "TSB", "GSB", "SSM") in the TSBs tab
    OpenBulletin {
        kind: String,
        id: String,
        title: String,
        url: String,
    },
}

/// Why asking Ford didn't work (kept short)
#[derive(Debug, Clone, PartialEq)]
pub enum FordError {
    SignedOut,
    /// PTS hasn't shown this vehicle yet (see [`Out::SetUpPts`])
    NotSetUp,
    Problem(String),
}

impl FordError {
    pub fn text(&self) -> String {
        match self {
            FordError::SignedOut => "Sign in to PTS".into(),
            FordError::NotSetUp => "PTS isn't set up for this vehicle yet".into(),
            FordError::Problem(m) => m.clone(),
        }
    }
}

impl From<ford::Error> for FordError {
    fn from(e: ford::Error) -> Self {
        match e {
            ford::Error::NeedsLogin => FordError::SignedOut,
            ford::Error::NotSetUp => FordError::NotSetUp,
            ford::Error::Status(code @ 500..=599) => FordError::Problem(format!("Ford is having issues (HTTP {code})")),
            ford::Error::Status(code) => FordError::Problem(format!("Ford answered HTTP {code}")),
            ford::Error::Net(_) => FordError::Problem("Ford can't be reached".into()),
            ford::Error::Format(m) => FordError::Problem(m),
        }
    }
}

/// Seconds since 1970
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Opens an address or a file with what the computer uses for it (the browser, the PDF viewer…)
pub fn open_external(target: &str) -> Result<(), String> {
    #[cfg(windows)]
    let r = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", target])
        .spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(target).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(target).spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}

/// Saves a file (a PDF from Ford) in the app's spot in the computer's temporary folder and opens it. `name`: what
/// it's called ("TSB 26-2414"), `ext`: its kind ("pdf").
pub fn open_file(name: &str, ext: &str, bytes: &[u8]) -> Result<(), String> {
    let dir = std::env::temp_dir().join("pts-workbench");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .chars()
        .take(60)
        .collect();
    let file = dir.join(format!("{}.{ext}", if safe.is_empty() { "document" } else { &safe }));
    std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
    open_external(&file.to_string_lossy())
}