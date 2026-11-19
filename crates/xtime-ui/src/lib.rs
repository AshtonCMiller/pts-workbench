//! What you see of Xtime in PTS Workbench (the data comes from the `xtime` crate; the look from `ui-kit`):
//!
//!   ro_view.rs     the Xtime View's state, its messages, and what each one changes (reading the RO, Done, the
//!                  inspection form, deleting a line)
//!   view.rs        the Xtime View itself: the header, Service Details, Vehicle Info, the lines
//!   inspection.rs  its inspection form: Green / Yellow / Red, sub-items, values, switching forms
//!   service.rs     the Add / Edit Service window (Xtime's Add Service window): its state, messages and card
//!   my_ros.rs      your ROs: the dashboard table (with search) and the My ROs list
//!   chat.rs        the Dealership chat panel: your conversations and their messages
//!
//! Each part has its own messages. What only the app can do (your hours, printing, opening a window, the bottom
//! bar's notice) goes back to it: a part's `update` returns [`Out`]s, and buttons for the app send [`Ask`]s.

pub mod chat;
pub mod inspection;
pub mod my_ros;
pub mod ro_view;
pub mod service;
pub mod view;

pub use ro_view::{Ask, Env, Msg, Out, XtimeView};

/// Why reading or changing something in Xtime didn't work (kept short: it's shown in the bottom bar)
#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    /// The sign-in has run out: sign in again
    SignedOut,
    /// No employee number or technician ID in Settings
    NoEmployee,
    /// The employee number isn't in the store's list
    NotFound(String),
    /// Xtime is down or answering oddly, or the network is
    Problem(String),
}

impl LoadError {
    pub fn text(&self) -> String {
        match self {
            LoadError::SignedOut => "Xtime: signed out".into(),
            LoadError::NoEmployee => "Xtime: add your employee number in Settings".into(),
            LoadError::NotFound(m) => format!("Xtime: {m}"),
            LoadError::Problem(m) => format!("Xtime: {m}"),
        }
    }

    /// Without the "Xtime: " in front (for "Couldn't save to Xtime (…)")
    pub fn plain(&self) -> String {
        self.text().trim_start_matches("Xtime: ").to_string()
    }
}

impl From<xtime::Error> for LoadError {
    fn from(e: xtime::Error) -> Self {
        use xtime::Error;
        match e {
            Error::SessionExpired => LoadError::SignedOut,
            Error::NotFound(m) => LoadError::NotFound(m),
            Error::Status(code @ 500..=599) => LoadError::Problem(format!("having issues (HTTP {code})")),
            Error::Status(code) => LoadError::Problem(format!("answered HTTP {code}")),
            Error::Net(_) => LoadError::Problem("can't be reached".into()),
            Error::Format(m) => LoadError::Problem(m),
        }
    }
}

/// "just now", "3 min ago", "2 h ago"
pub fn ago(secs: i64) -> String {
    match secs.max(0) {
        0..60 => "just now".into(),
        s @ 60..3600 => format!("{} min ago", s / 60),
        s => format!("{} h ago", s / 3600),
    }
}

/// Seconds since 1970
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_how_long_ago() {
        assert_eq!(ago(5), "just now");
        assert_eq!(ago(-5), "just now");
        assert_eq!(ago(130), "2 min ago");
        assert_eq!(ago(7300), "2 h ago");
    }

    #[test]
    fn says_what_went_wrong() {
        assert_eq!(LoadError::from(xtime::Error::SessionExpired), LoadError::SignedOut);
        assert_eq!(LoadError::from(xtime::Error::Status(503)).text(), "Xtime: having issues (HTTP 503)");
        assert_eq!(LoadError::Problem("x".into()).plain(), "x");
    }
}
