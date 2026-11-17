//! Your ROs from Xtime: loads the dashboard (and, the first time, your technician ID from your employee number), every
//! minute while signed in, as the Electron app does. What comes back stays in memory on this computer only.

use xtime::dashboard::AssignedRo;
use xtime::employee::Employee;
use xtime::{Xtime, paths};

/// How often your ROs are loaded again (the Electron app's pollSeconds)
pub const EVERY: std::time::Duration = std::time::Duration::from_secs(60);

/// A load that worked
#[derive(Debug, Clone)]
pub struct Loaded {
    pub ros: Vec<AssignedRo>,
    /// ROs in Dispatch with no technician yet (anyone's to accept)
    pub dispatch: Vec<AssignedRo>,
    /// You, when your technician ID was looked up from your employee number this time
    pub found: Option<Employee>,
    /// Your name as Xtime has it on your ROs ("" when none says)
    pub tech_name: String,
}

/// Why reading Xtime didn't work, and "2 min ago" (they're shared with the Xtime View: crates/xtime-ui)
pub use xtime_ui::{LoadError, ago};

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
        dispatch: dash.dispatch,
        found,
        tech_name: dash.tech_name,
    })
}
