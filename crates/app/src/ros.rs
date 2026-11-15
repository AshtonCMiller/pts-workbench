//! Your ROs from Xtime: loads the dashboard (and, the first time, your technician ID from your employee number), every
//! minute while signed in, as the Electron app does. What comes back stays in memory on this computer only.

use xtime::dashboard::AssignedRo;
use xtime::employee::Employee;
use xtime::{Error, Xtime, paths};

/// How often your ROs are loaded again (the Electron app's pollSeconds)
pub const EVERY: std::time::Duration = std::time::Duration::from_secs(60);

/// A load that worked
#[derive(Debug, Clone)]
pub struct Loaded {
    pub ros: Vec<AssignedRo>,
    /// You, when your technician ID was looked up from your employee number this time
    pub found: Option<Employee>,
    /// Your name as Xtime has it on your ROs ("" when none says)
    pub tech_name: String,
}

/// Why a load didn't work (kept short: it's shown in the bottom bar)
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
}

impl From<Error> for LoadError {
    fn from(e: Error) -> Self {
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

/// Loads your ROs. `tech_id`: from Settings; when it's empty, it's looked up from `employee_number` first.
pub async fn load(x: Xtime, tech_id: String, employee_number: String, store: String) -> Result<Loaded, LoadError> {
    let staff_path = paths::employees(&store);
    let (tech_id, found) = match (tech_id.trim(), employee_number.trim()) {
        ("", "") => return Err(LoadError::NoEmployee),
        ("", number) => {
            let me = x.employee(number, &store, paths::EMPLOYEES).await?;
            (me.id.clone(), Some(me))
        }
        (id, _) => (id.to_string(), None),
    };
    let dash = x.dashboard(&tech_id, paths::DASHBOARD, &staff_path).await?;
    Ok(Loaded {
        ros: dash.mine,
        found,
        tech_name: dash.tech_name,
    })
}

/// "just now", "3 min ago", "2 h ago"
pub fn ago(secs: i64) -> String {
    match secs.max(0) {
        0..60 => "just now".into(),
        s @ 60..3600 => format!("{} min ago", s / 60),
        s => format!("{} h ago", s / 3600),
    }
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
        assert_eq!(LoadError::from(Error::SessionExpired), LoadError::SignedOut);
        assert_eq!(LoadError::from(Error::Status(503)).text(), "Xtime: having issues (HTTP 503)");
    }
}
