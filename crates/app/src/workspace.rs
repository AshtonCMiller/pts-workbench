//! The vehicles open in the Workbench, and the tabs each one has (as in the Electron app).

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use xtime::dashboard::AssignedRo;
use xtime::ro::RoView;

use crate::ros::LoadError;

/// One open vehicle (RO)
#[derive(Debug, Clone)]
pub struct Workspace {
    pub ro: AssignedRo,
    pub tab: Tab,
    pub manual: ManualTab,
    /// The Xtime View tab: the RO as Xtime has it
    pub xv: XtimeView,
}

/// The Xtime View tab's RO and what's open on it
#[derive(Debug, Clone, Default)]
pub struct XtimeView {
    pub view: Option<RoView>,
    pub loading: bool,
    /// Why the last read didn't work (the last good RO stays shown)
    pub error: Option<LoadError>,
    /// When it was last read (seconds since 1970)
    pub at: Option<i64>,
    /// Lines shown opened (by line ID), and when each was opened (for its slide in)
    pub open: HashMap<String, Instant>,
    pub show_inspection: bool,
    /// Lines whose Done is being sent to Xtime
    pub busy: HashSet<String>,
}

impl Workspace {
    pub fn new(ro: AssignedRo) -> Self {
        Self {
            ro,
            tab: Tab::XtimeView, // a vehicle opens on its RO
            manual: ManualTab::Oasis,
            xv: XtimeView::default(),
        }
    }

    /// "2026 Ford Bronco"
    pub fn vehicle(&self) -> String {
        vehicle_name(&self.ro)
    }
}

pub fn vehicle_name(ro: &AssignedRo) -> String {
    [ro.year.as_str(), ro.make.as_str(), ro.model.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A vehicle's main tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// The app's own Xtime RO page
    XtimeView,
    /// The app's Service Manual (its own tabs below)
    ServiceManual,
}

impl Tab {
    pub const ALL: [Tab; 2] = [Tab::XtimeView, Tab::ServiceManual];
    pub fn label(self) -> &'static str {
        match self {
            Tab::XtimeView => "Xtime View",
            Tab::ServiceManual => "Service Manual",
        }
    }
}

/// The Service Manual's tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualTab {
    Oasis,
    ConnectedVehicle,
    Diagnostics,
    Tsbs,
    Workshop,
    Wiring,
    Labor,
}

impl ManualTab {
    pub const ALL: [ManualTab; 7] = [
        ManualTab::Oasis,
        ManualTab::ConnectedVehicle,
        ManualTab::Diagnostics,
        ManualTab::Tsbs,
        ManualTab::Workshop,
        ManualTab::Wiring,
        ManualTab::Labor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ManualTab::Oasis => "OASIS",
            ManualTab::ConnectedVehicle => "Connected Vehicle",
            ManualTab::Diagnostics => "Diagnostics",
            ManualTab::Tsbs => "TSBs",
            ManualTab::Workshop => "Workshop Manual",
            ManualTab::Wiring => "Wiring",
            ManualTab::Labor => "Labor Times",
        }
    }

    /// What the tab will show (until the Ford crate has it)
    pub fn about(self) -> &'static str {
        match self {
            ManualTab::Oasis => "The vehicle's OASIS record: vehicle details, warranty, field service actions and messages.",
            ManualTab::ConnectedVehicle => {
                "What the vehicle reports through its modem: status, DTC history with TSBs, over-the-air updates, subscriptions."
            }
            ManualTab::Diagnostics => "Search DTCs and symptoms for TSBs, GSBs, SSMs, pinpoint tests and procedures.",
            ManualTab::Tsbs => "TSBs, GSBs and SSMs for this VIN.",
            ManualTab::Workshop => "The Workshop Manual and PC/ED for this VIN: contents, search and procedures.",
            ManualTab::Wiring => "Wiring diagrams for this VIN: contents, pages and connector views.",
            ManualTab::Labor => "SLTS labor times for this VIN, with parts and a tray for the RO.",
        }
    }
}

/// My ROs' groups in the sidebar, in order
pub const STAGES: [&str; 5] = ["Inspection", "Approval", "Repair", "Review", "Done"];

/// Which of [`STAGES`] an RO is in by its Xtime mode: not started yet (Pre-RO, Dispatch) counts as Inspection; closed,
/// or a stage the app doesn't know, as Done
pub fn stage(mode: Option<f64>) -> usize {
    match mode.map(|m| m as i64) {
        Some(2 | 3 | 9) | None => 0,
        Some(4 | 5) => 1,
        Some(6) => 2,
        Some(7) => 3,
        _ => 4,
    }
}

/// Xtime's mode numbers, as the dashboard shows them
pub fn mode_label(mode: Option<f64>) -> (&'static str, [u8; 3]) {
    match mode.map(|m| m as i64) {
        Some(3) => ("In Inspection", [0x7a, 0x5a, 0xf5]),
        Some(4) => ("Pending Approval", [0xd9, 0x77, 0x06]),
        Some(5) => ("Pending Approval (parts?)", [0xd9, 0x77, 0x06]),
        Some(6) => ("In Repair", [0x15, 0x80, 0x3d]),
        Some(7) => ("Pending Review", [0x0e, 0x74, 0x90]),
        _ => ("", [0x5d, 0x66, 0x70]),
    }
}
