//! The vehicles open in the Workbench, and the tabs each one has (as in the Electron app).

use xtime::dashboard::AssignedRo;
// the vehicle's name and the RO's stage (crates/xtime-ui), under their usual names here
pub use xtime_ui::my_ros::{mode_label, vehicle_name};

/// One open vehicle (RO)
#[derive(Debug, Clone)]
pub struct Workspace {
    pub ro: AssignedRo,
    pub tab: Tab,
    pub manual: ManualTab,
    /// The Xtime View tab: the RO as Xtime has it (crates/xtime-ui)
    pub xv: xtime_ui::XtimeView,
    /// The Service Manual's OASIS and TSBs tabs (crates/ford-ui)
    pub oasis: ford_ui::OasisView,
    /// The Connected Vehicle tab (its status shows on OASIS too)
    pub cv: ford_ui::ConnectedView,
    pub tsb: ford_ui::TsbView,
    /// The Workshop Manual tab
    pub wsm: ford_ui::ManualView,
    /// The Diagnostics tab (Ford's search for DTCs and symptoms)
    pub diag: ford_ui::DiagView,
    /// The Labor Times tab (Ford's SLTS), with its tray
    pub labor: ford_ui::LaborView,
    /// The Wiring tab (the vehicle's wiring book)
    pub wiring: ford_ui::WiringView,
    /// Tabs of their own for Ford's documents (an FSA opened from OASIS…), and the next one's number
    pub docs: Vec<ford_ui::DocTab>,
    pub next_doc: u32,
    /// The Maintenance tab's specs (a Quick Lane profile's)
    pub maint: Maint,
}

/// The Maintenance tab: the specs read from the Workshop Manual (ford::specs)
#[derive(Debug, Clone, Default)]
pub enum Maint {
    #[default]
    NotRead,
    Reading,
    Read(Vec<ford::specs::SpecGroup>),
    Failed(String),
}

impl Workspace {
    pub fn new(ro: AssignedRo) -> Self {
        let xv = xtime_ui::XtimeView::new(ro.ro.clone());
        let oasis = ford_ui::OasisView::new(ro.vin.clone());
        let cv = ford_ui::ConnectedView::new(ro.vin.clone());
        let tsb = ford_ui::TsbView::new(ro.vin.clone());
        let wsm = ford_ui::ManualView::new(ro.vin.clone());
        let diag = ford_ui::DiagView::new(ro.vin.clone());
        let wiring = ford_ui::WiringView::new(ro.vin.clone());
        // (for its printouts: "2023 Ford F-150 · VIN · RO 820367")
        let line = [vehicle_name(&ro), ro.vin.clone(), format!("RO {}", ro.ro)]
            .into_iter()
            .filter(|s| !s.is_empty() && s != "RO ")
            .collect::<Vec<_>>()
            .join(" · ");
        let labor = ford_ui::LaborView::new(ro.vin.clone(), line);
        Self {
            ro,
            tab: Tab::XtimeView, // a vehicle opens on its RO
            manual: ManualTab::Oasis,
            xv,
            oasis,
            cv,
            tsb,
            wsm,
            diag,
            labor,
            wiring,
            docs: Vec::new(),
            next_doc: 1,
            maint: Maint::NotRead,
        }
    }

    /// "2026 Ford Bronco"
    pub fn vehicle(&self) -> String {
        vehicle_name(&self.ro)
    }

    /// One of the sample ROs shown before Xtime is connected (its VIN isn't real: nothing is asked about it)
    pub fn is_sample(&self) -> bool {
        self.ro.vin.starts_with("SAMPLE")
    }
}

/// A vehicle's main tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// The maintenance specs (ford::specs): a Quick Lane profile's first tab, a Main Shop's last
    Maintenance,
    /// The app's own Xtime RO page
    XtimeView,
    /// Xtime's own RO page (the website, inside the app: crates/webpane)
    XtimeSite,
    /// The app's Service Manual (its own tabs below)
    ServiceManual,
    /// PTS itself, opened on the vehicle (the website, inside the app)
    PtsSite,
}

impl Tab {
    /// A Main Shop profile's: the maintenance specs last
    pub const ALL: [Tab; 5] = [Tab::XtimeView, Tab::XtimeSite, Tab::ServiceManual, Tab::PtsSite, Tab::Maintenance];
    /// A Quick Lane profile's: the maintenance specs first
    pub const QUICK: [Tab; 5] = [Tab::Maintenance, Tab::XtimeView, Tab::XtimeSite, Tab::ServiceManual, Tab::PtsSite];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Maintenance => "Maintenance",
            Tab::XtimeView => "Xtime View",
            Tab::XtimeSite => "Xtime RO Page",
            Tab::ServiceManual => "Service Manual",
            Tab::PtsSite => "PTS",
        }
    }
}

/// The Service Manual's tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualTab {
    Oasis,
    ConnectedVehicle,
    Diagnostics,
    /// Under Diagnostics (its menu): every PTS / FDRS session recorded for the VIN (PTS's own page, inside the app)
    History,
    /// Under Diagnostics (its menu): the vehicle's software
    Software,
    Tsbs,
    Workshop,
    Wiring,
    Labor,
    /// One of Ford's documents in a tab of its own (its number among the vehicle's [`Workspace::docs`])
    Doc(u32),
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
            ManualTab::History => "Vehicle History",
            ManualTab::Software => "Vehicle Software",
            ManualTab::Tsbs => "TSBs",
            ManualTab::Workshop => "Workshop Manual",
            ManualTab::Wiring => "Wiring",
            ManualTab::Labor => "Labor Times",
            ManualTab::Doc(_) => "Document",
        }
    }

    /// One of the Diagnostics tab's (its menu: Diagnostics Home, Vehicle History, Vehicle Software)
    pub fn is_diag(self) -> bool {
        matches!(self, ManualTab::Diagnostics | ManualTab::History | ManualTab::Software)
    }

    /// What the tab will show (until the Ford crate has it)
    pub fn about(self) -> &'static str {
        match self {
            ManualTab::Oasis => "The vehicle's OASIS record: vehicle details, warranty, field service actions and messages.",
            ManualTab::ConnectedVehicle => {
                "What the vehicle reports through its modem: status, DTC history with TSBs, over-the-air updates, subscriptions."
            }
            ManualTab::Diagnostics => "Search DTCs and symptoms for TSBs, GSBs, SSMs, pinpoint tests and procedures.",
            ManualTab::History => "Every PTS and FDRS session recorded for this VIN: when, the mileage, and each step.",
            ManualTab::Software => "The vehicle's module software: what it has, and what's newer.",
            ManualTab::Tsbs => "TSBs, GSBs and SSMs for this VIN.",
            ManualTab::Workshop => "The Workshop Manual and PC/ED for this VIN: contents, search and procedures.",
            ManualTab::Wiring => "Wiring diagrams for this VIN: contents, pages and connector views.",
            ManualTab::Labor => "SLTS labor times for this VIN, with parts and a tray for the RO.",
            ManualTab::Doc(_) => "One of Ford's documents.",
        }
    }
}
