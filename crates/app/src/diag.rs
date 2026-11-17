//! Diagnostics: the vehicle plugged in through Ford's VCI (VCM 3, or the VCMM), the way IDS / FDRS reach it.
//!
//!   1. Find the VCI: the J2534 interfaces Windows has (Ford's VCI Software installs them), the VCM 3 picked first.
//!   2. Connect: the VCI opened, the battery voltage read, the high-speed CAN bus joined (OBD pins 6 / 14).
//!   3. Identify: the PCM asked for the VIN (22 F190; OBD's 09 02 if it won't say), matched to your ROs.
//!   4. Network test: each module asked for its part number and software (crates/vci/scan.rs). On pins 6 / 14
//!      (on a newer Ford, the gateway passes the requests on to every network); on an older Ford without a
//!      gateway, on its other networks' pins too (MS-CAN 3 / 11, HS-CAN2 8 / 12, HS-CAN3 1 / 9). The full search
//!      asks every other diagnostic address as well.
//!   5. Self test: each module's trouble codes; and clearing them, when you ask (crates/vci/dtc.rs).
//!
//! Its tabs: Vehicle ID (connect, the VIN), the Dashboard once the vehicle's identified (its modules by network,
//! or as a drawing of the networks; the applications), and each application opened (Self Test, for every module or
//! one).
//!
//! All of it through the 32-bit helper (crates/vci-bridge), asked one thing at a time. Only reads, and clearing codes
//! after you confirm: nothing on the vehicle is written or programmed.
//!
//! The communication log (each request and answer) has the VIN in it, so it stays in the app: shown here, copied
//! only when asked. diagnostics.log gets what happened, without the VIN.

use std::io::Write;
use std::time::Instant;

use iced::Task;
use vci::client::Bridge;
use vci::dtc::SelfTest;
use vci::modules::{self, Bus};
use vci::proto::{Device, Reply, Request};
use vci::scan::Scan;
use vci::uds;

/// Run from source (a debug build): the demo vehicle is offered beside the real VCIs
pub const DEMO: bool = cfg!(debug_assertions);
/// Under this, the battery's low: modules can drop out (and IDS / FDRS want a charger on)
pub const LOW_MV: u32 = 12_000;
/// Lines kept in the communication log
const LOG_LINES: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    Closed,
    /// Opening the VCI, reading the voltage, joining the bus
    Connecting,
    Connected {
        firmware: String,
        api: String,
    },
}

#[derive(Debug)]
pub struct Diag {
    bridge: Bridge,
    /// The interfaces Windows has (none yet: not looked; looking)
    pub devices: Option<Vec<Device>>,
    pub looking: bool,
    /// The one chosen (by name)
    pub picked: Option<String>,
    pub link: Link,
    pub millivolts: Option<u32>,
    pub reading_volts: bool,
    /// The vehicle's VIN (none: not asked, or it didn't say)
    pub vin: Option<String>,
    pub identifying: bool,
    /// Asked, and no VIN came back
    pub no_vin: bool,
    pub scan: Option<Scan>,
    pub scanning: bool,
    /// What the network test found out about the vehicle's networks ("This vehicle has a gateway…"), and the
    /// networks it couldn't reach
    pub scan_notes: Vec<String>,
    /// The tab showing, and since when (its slide in)
    pub tab: Tab,
    pub tab_since: Instant,
    /// The applications open (each in its own tab)
    pub apps: Vec<SelfTestApp>,
    next_app: u64,
    /// The application tab whose self test is running
    pub testing: Option<u64>,
    /// The Dashboard: the networks drawn (rather than listed); the module picked
    pub topology: bool,
    pub selected: Option<(u32, Bus)>,
    /// Stop asked for: stops after the request it's on
    pub stopping: bool,
    pub error: Option<String>,
    /// The communication log: in the app only (it has the VIN)
    pub log: Vec<String>,
    pub show_log: bool,
    started: Option<Instant>,
}

impl Default for Diag {
    fn default() -> Self {
        Diag {
            bridge: Bridge::default(),
            devices: None,
            looking: false,
            picked: None,
            link: Link::Closed,
            millivolts: None,
            reading_volts: false,
            vin: None,
            identifying: false,
            no_vin: false,
            scan: None,
            scanning: false,
            scan_notes: Vec::new(),
            tab: Tab::Id,
            tab_since: Instant::now(),
            apps: Vec::new(),
            next_app: 1,
            testing: None,
            topology: false,
            selected: None,
            stopping: false,
            error: None,
            log: Vec::new(),
            show_log: false,
            started: None,
        }
    }
}

/// The Diagnostics tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// Vehicle ID: the VCI, connecting, the VIN
    Id,
    /// The vehicle's modules and the applications (once it's identified)
    Dashboard,
    /// An application (its ID)
    App(u64),
}

/// A Self Test tab: every module the network test found (or the list), or one module
#[derive(Debug)]
pub struct SelfTestApp {
    pub id: u64,
    pub target: Option<(modules::Entry, Bus)>,
    pub test: Option<SelfTest>,
    /// "Clear the codes?" showing
    pub confirm_clear: bool,
}

/// A module's short name ("ABS"), or its address for one found by the full search ("7A5")
pub fn tag(short: &str, address: u32) -> String {
    if short == modules::UNLISTED.0 {
        format!("{address:03X}")
    } else {
        short.to_string()
    }
}

impl SelfTestApp {
    pub fn title(&self) -> String {
        match self.target {
            Some(((a, short, _), _)) => format!("Self Test · {}", tag(short, a)),
            None => "Self Test".into(),
        }
    }
}

/// What came of connecting
#[derive(Debug, Clone)]
pub struct Joined {
    firmware: String,
    api: String,
    millivolts: Option<u32>,
}

/// What identifying found: the VIN, and the log's lines for it
#[derive(Debug, Clone)]
pub struct Identity {
    vin: Option<String>,
    lines: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum DiagMsg {
    /// Find the VCI (again)
    Look,
    Found(Result<Vec<Device>, String>),
    Pick(String),
    Connect,
    Connected(Result<Joined, String>),
    Disconnect,
    Disconnected,
    ReadVolts,
    Volts(Result<u32, String>),
    Identify,
    Identified(Result<Identity, String>),
    RunScan,
    StopScan,
    /// One module's answer (the request it answers, what came back)
    Scanned(vci::scan::Ask, Result<Option<Vec<u8>>, String>),
    /// The network test's full search: every other diagnostic address
    FullSearch,
    /// The tabs: one picked; an application's closed; the Dashboard without a VIN (the PCM didn't say)
    SetTab(Tab),
    CloseApp(u64),
    ToDashboard,
    /// The Dashboard: networks drawn or listed; a module picked (again: let go)
    Topology(bool),
    Select(u32, Bus),
    /// A Self Test tab opened: every module, or one (read straight away)
    OpenSelfTest(Option<(modules::Entry, Bus)>),
    /// Self test (in its tab): read the codes; stop; clear them (asks first); one module's answer
    RunSelfTest(u64),
    StopSelfTest,
    AskClear(u64),
    CancelClear(u64),
    ConfirmClear(u64),
    SelfTested(u64, vci::scan::Ask, Result<Option<Vec<u8>>, String>),
    /// The codes found, as text (for the RO's story)
    CopyCodes(u64),
    ToggleLog,
    CopyLog,
    ClearLog,
    CopyVin,
    OpenRo(String),
    DismissError,
}

/// What the app does about it
pub enum Out {
    None,
    OpenRo(String),
    Copy(String),
}

/// The helper's own errors as errors
fn ok(r: Result<Reply, String>) -> Result<Reply, String> {
    match r? {
        Reply::Error { text } => Err(text),
        r => Ok(r),
    }
}

/// diagnostics.log: what happened (never the VIN, or what a module said)
fn note(line: &str) {
    if cfg!(test) {
        return;
    }
    let path = crate::settings::app_dir().join("diagnostics.log");
    // kept short: started again past 1 MB
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 1_000_000) {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(f, "{at}  {line}");
    }
}

async fn connect(b: Bridge, dll: String) -> Result<Joined, String> {
    let (firmware, api) = match ok(b.call(Request::Open { dll }).await)? {
        Reply::Opened { firmware, api, .. } => (firmware, api),
        other => return Err(format!("The VCI answered oddly ({other:?})")),
    };
    let millivolts = match b.call(Request::Voltage).await {
        Ok(Reply::Voltage { millivolts }) => Some(millivolts),
        _ => None,
    };
    if let Err(e) = ok(b
        .call(Request::Connect {
            baud: modules::HS_CAN_BAUD,
            pins: 0,
        })
        .await)
    {
        // (let go of the VCI, so it isn't left in use)
        let _ = b.call(Request::Close).await;
        return Err(e);
    }
    Ok(Joined { firmware, api, millivolts })
}

async fn ask(b: &Bridge, tx: u32, data: Vec<u8>, timeout_ms: u32) -> Result<Option<Vec<u8>>, String> {
    let rx = modules::answers_at(tx);
    let req = Request::Ask {
        tx,
        rx,
        data,
        timeout_ms,
        pins: 0,
        baud: 0,
    };
    match ok(b.call(req).await)? {
        Reply::Answer { data, .. } => Ok(data),
        other => Err(format!("The VCI answered oddly ({other:?})")),
    }
}

/// The PCM asked for the VIN: UDS first, OBD's way if it won't say
async fn identify(b: Bridge, t0: Instant) -> Result<Identity, String> {
    let mut lines = Vec::new();
    for req in [uds::read(uds::VIN), uds::OBD_VIN.to_vec()] {
        lines.push(line(t0, "→", modules::PCM, &req));
        let got = ask(&b, modules::PCM, req, 1000).await?;
        match &got {
            Some(r) => lines.push(line(t0, "←", modules::answers_at(modules::PCM), r)),
            None => lines.push(format!("{}  ←  {:03X}  (no answer)", stamp(t0), modules::answers_at(modules::PCM))),
        }
        if let Some(vin) = got.as_deref().and_then(uds::vin_from) {
            return Ok(Identity { vin: Some(vin), lines });
        }
    }
    Ok(Identity { vin: None, lines })
}

fn stamp(t0: Instant) -> String {
    format!("{:>8.3}", t0.elapsed().as_secs_f64())
}

/// "  1.204  →  7E0  22 F1 90"
fn line(t0: Instant, dir: &str, addr: u32, data: &[u8]) -> String {
    format!("{}  {dir}  {addr:03X}  {}", stamp(t0), uds::hex(data))
}

impl Diag {
    pub fn connected(&self) -> bool {
        matches!(self.link, Link::Connected { .. })
    }

    /// Busy with something that has to finish before the next thing
    pub fn busy(&self) -> bool {
        self.looking || self.link == Link::Connecting || self.identifying || self.scanning || self.testing.is_some()
    }

    /// The chosen interface
    pub fn device(&self) -> Option<&Device> {
        let list = self.devices.as_ref()?;
        self.picked
            .as_ref()
            .and_then(|p| list.iter().find(|d| &d.name == p))
            .or_else(|| list.first())
    }

    /// Whether the voltage should be read again now (connected, nothing else going on)
    pub fn wants_volts(&self) -> bool {
        self.connected() && !self.busy() && !self.reading_volts
    }

    fn push_log(&mut self, l: String) {
        self.log.push(l);
        if self.log.len() > LOG_LINES {
            let extra = self.log.len() - LOG_LINES;
            self.log.drain(..extra);
        }
    }

    fn t0(&mut self) -> Instant {
        *self.started.get_or_insert_with(Instant::now)
    }

    fn go(&mut self, tab: Tab) {
        if self.tab != tab {
            self.tab = tab;
            self.tab_since = Instant::now();
        }
    }

    pub fn app(&self, id: u64) -> Option<&SelfTestApp> {
        self.apps.iter().find(|a| a.id == id)
    }

    /// The modules found (by the network test), in order
    pub fn found(&self) -> Vec<&vci::scan::Module> {
        self.scan.iter().flat_map(|s| s.modules.iter()).filter(|m| m.present()).collect()
    }

    /// The first time Diagnostics is shown: the VCI looked for
    pub fn shown(&mut self) -> Task<DiagMsg> {
        if self.devices.is_none() && !self.looking {
            return Task::done(DiagMsg::Look);
        }
        Task::none()
    }

    pub fn update(&mut self, msg: DiagMsg) -> (Task<DiagMsg>, Out) {
        let none = |t| (t, Out::None);
        match msg {
            DiagMsg::Look => {
                if self.busy() || self.connected() {
                    return none(Task::none());
                }
                self.looking = true;
                self.error = None;
                let b = self.bridge.clone();
                none(Task::perform(
                    async move {
                        let found = match ok(b.call(Request::Devices).await) {
                            Ok(Reply::Devices { list }) => Ok(list),
                            Ok(_) => Ok(Vec::new()),
                            Err(e) => Err(e),
                        };
                        // run from source: the demo vehicle too (whether or not the helper's built)
                        if DEMO {
                            let mut list = found.unwrap_or_default();
                            list.push(Device {
                                name: vci::sim::DEMO_NAME.into(),
                                vendor: "PTS Workbench".into(),
                                dll: vci::sim::DEMO_DLL.into(),
                                iso15765: true,
                                iso15765_ps: true,
                            });
                            return Ok(list);
                        }
                        found
                    },
                    DiagMsg::Found,
                ))
            }
            DiagMsg::Found(r) => {
                self.looking = false;
                match r {
                    Ok(list) => {
                        note(&format!(
                            "VCIs found: {}",
                            if list.is_empty() {
                                "none".to_string()
                            } else {
                                list.iter().map(|d| d.name.as_str()).collect::<Vec<_>>().join(", ")
                            }
                        ));
                        // the one chosen before, if it's still there; otherwise the first (the VCM 3)
                        if !self.picked.as_ref().is_some_and(|p| list.iter().any(|d| &d.name == p)) {
                            self.picked = list.first().map(|d| d.name.clone());
                        }
                        self.devices = Some(list);
                    }
                    Err(e) => {
                        note(&format!("Looking for the VCI didn't work: {e}"));
                        self.devices = Some(Vec::new());
                        self.error = Some(e);
                    }
                }
                none(Task::none())
            }
            DiagMsg::Pick(name) => {
                if !self.connected() && !self.busy() {
                    self.picked = Some(name);
                }
                none(Task::none())
            }
            DiagMsg::Connect => {
                let Some(d) = self.device().cloned() else {
                    return none(Task::none());
                };
                if self.busy() || self.connected() {
                    return none(Task::none());
                }
                self.link = Link::Connecting;
                self.error = None;
                self.vin = None;
                self.no_vin = false;
                self.scan = None;
                self.millivolts = None;
                // (a new vehicle: its tabs start again)
                self.apps.clear();
                self.selected = None;
                self.go(Tab::Id);
                let t0 = self.t0();
                self.push_log(format!("{}  ··  Opening {} ({})", stamp(t0), d.name, d.dll));
                note(&format!("Connecting: {}", d.name));
                let b = self.bridge.clone();
                none(Task::perform(connect(b, d.dll), DiagMsg::Connected))
            }
            DiagMsg::Connected(r) => match r {
                Ok(j) => {
                    let t0 = self.t0();
                    self.push_log(format!(
                        "{}  ··  Connected: firmware {}, J2534 {}; on the CAN bus at {} kbit/s",
                        stamp(t0),
                        j.firmware,
                        j.api,
                        modules::HS_CAN_BAUD / 1000
                    ));
                    note(&format!(
                        "Connected (firmware {}, J2534 {}){}",
                        j.firmware,
                        j.api,
                        j.millivolts
                            .map(|mv| format!(", {:.1} V", f64::from(mv) / 1000.0))
                            .unwrap_or_default()
                    ));
                    self.millivolts = j.millivolts;
                    self.link = Link::Connected {
                        firmware: j.firmware,
                        api: j.api,
                    };
                    // straight on to who the vehicle is
                    none(Task::done(DiagMsg::Identify))
                }
                Err(e) => {
                    note(&format!("Connecting didn't work: {e}"));
                    let t0 = self.t0();
                    self.push_log(format!("{}  ··  {e}", stamp(t0)));
                    self.link = Link::Closed;
                    self.error = Some(e);
                    none(Task::none())
                }
            },
            DiagMsg::Disconnect => {
                self.stopping = self.scanning || self.testing.is_some();
                let b = self.bridge.clone();
                none(Task::perform(
                    async move {
                        let _ = b.call(Request::Disconnect).await;
                        b.stop().await;
                    },
                    |_| DiagMsg::Disconnected,
                ))
            }
            DiagMsg::Disconnected => {
                if self.link != Link::Closed {
                    note("Disconnected");
                }
                let t0 = self.t0();
                self.push_log(format!("{}  ··  Disconnected", stamp(t0)));
                self.link = Link::Closed;
                self.millivolts = None;
                self.identifying = false;
                self.scanning = false;
                self.testing = None;
                for a in &mut self.apps {
                    a.confirm_clear = false;
                }
                self.stopping = false;
                none(Task::none())
            }
            DiagMsg::ReadVolts => {
                if !self.wants_volts() {
                    return none(Task::none());
                }
                self.reading_volts = true;
                let b = self.bridge.clone();
                none(Task::perform(
                    async move {
                        match ok(b.call(Request::Voltage).await)? {
                            Reply::Voltage { millivolts } => Ok(millivolts),
                            _ => Err("no voltage".to_string()),
                        }
                    },
                    DiagMsg::Volts,
                ))
            }
            DiagMsg::Volts(r) => {
                self.reading_volts = false;
                match r {
                    Ok(mv) => self.millivolts = Some(mv),
                    // the helper stopped (the VCI unplugged…): connected no more
                    Err(e) if e.contains("helper") || e.contains("in time") => {
                        note(&format!("Lost the VCI: {e}"));
                        self.error = Some(format!("Lost the VCI: {e}"));
                        self.link = Link::Closed;
                        self.millivolts = None;
                    }
                    Err(_) => {}
                }
                none(Task::none())
            }
            DiagMsg::Identify => {
                if !self.connected() || self.identifying || self.scanning || self.testing.is_some() {
                    return none(Task::none());
                }
                self.identifying = true;
                self.no_vin = false;
                let t0 = self.t0();
                let b = self.bridge.clone();
                none(Task::perform(identify(b, t0), DiagMsg::Identified))
            }
            DiagMsg::Identified(r) => {
                self.identifying = false;
                match r {
                    Ok(id) => {
                        for l in id.lines {
                            self.push_log(l);
                        }
                        note(if id.vin.is_some() {
                            "Vehicle identified"
                        } else {
                            "The vehicle didn't give its VIN"
                        });
                        self.no_vin = id.vin.is_none();
                        let found = id.vin.is_some();
                        self.vin = id.vin;
                        // identified: on to the Dashboard, its modules found straight away
                        if found {
                            return none(self.to_dashboard());
                        }
                    }
                    Err(e) => {
                        note(&format!("Identifying didn't work: {e}"));
                        self.error = Some(e);
                    }
                }
                none(Task::none())
            }
            DiagMsg::RunScan => {
                if !self.connected() || self.busy() {
                    return none(Task::none());
                }
                let scan = Scan::hs_can();
                let Some(first) = scan.next() else { return none(Task::none()) };
                self.scan = Some(scan);
                self.scanning = true;
                self.stopping = false;
                self.error = None;
                self.scan_notes.clear();
                let t0 = self.t0();
                self.push_log(format!("{}  ··  Network test (pins 6 / 14)", stamp(t0)));
                note("Network test started");
                none(self.ask_next(first).map(|(a, got)| DiagMsg::Scanned(a, got)))
            }
            DiagMsg::StopScan => {
                self.stopping = self.scanning;
                none(Task::none())
            }
            DiagMsg::Scanned(asked, r) => {
                if !self.scanning {
                    return none(Task::none());
                }
                let t0 = self.t0();
                match r {
                    Ok(answer) => {
                        match &answer {
                            Some(a) => self.push_log(line(t0, "←", asked.rx, a)),
                            None => self.push_log(format!("{}  ←  {:03X}  (no answer)", stamp(t0), asked.rx)),
                        }
                        let Some(scan) = self.scan.as_mut() else {
                            return none(Task::none());
                        };
                        scan.answer(answer.as_deref());
                        // the list done on pins 6 / 14: an older Ford (no gateway) has its other networks at the DLC too
                        if scan.done() && !self.stopping && asked.bus == Bus::Hs && !scan.others_asked() {
                            if scan.gateway() {
                                self.scan_notes
                                    .push("This vehicle has a gateway (GWM): every module is reached through it, from pins 6 / 14.".into());
                            } else if scan.modules.iter().all(|m| m.listed) {
                                scan.add_other_networks();
                                self.log.push(format!(
                                    "{}  ··  No gateway on 6 / 14: the other networks too (pins 3 / 11, 8 / 12, 1 / 9)",
                                    stamp(t0)
                                ));
                            }
                        }
                        match scan.next() {
                            Some(next) if !self.stopping => none(self.ask_next(next).map(|(a, got)| DiagMsg::Scanned(a, got))),
                            _ => {
                                let (asked_n, total, _) = scan.progress();
                                let found = scan.found();
                                note(&format!(
                                    "Network test {}: {found} module(s) answered of {asked_n} asked ({total} on the list)",
                                    if self.stopping { "stopped" } else { "done" }
                                ));
                                self.scanning = false;
                                self.stopping = false;
                                none(Task::none())
                            }
                        }
                    }
                    // another network can't be reached (the VCI, or the vehicle): that pass left out, the rest carries on
                    Err(e) if asked.bus != Bus::Hs && self.scan.is_some() => {
                        let bus = asked.bus;
                        note(&format!("Network test: {} left out ({e})", bus.label()));
                        self.push_log(format!("{}  ··  {e}", stamp(t0)));
                        self.scan_notes.push(format!("{} couldn't be reached: {e}", bus.full_label()));
                        let scan = self.scan.as_mut().expect("checked");
                        scan.skip(bus);
                        match scan.next() {
                            Some(next) if !self.stopping => none(self.ask_next(next).map(|(a, got)| DiagMsg::Scanned(a, got))),
                            _ => {
                                self.scanning = false;
                                self.stopping = false;
                                none(Task::none())
                            }
                        }
                    }
                    Err(e) => {
                        note(&format!("Network test stopped: {e}"));
                        self.push_log(format!("{}  ··  {e}", stamp(t0)));
                        self.error = Some(e);
                        self.scanning = false;
                        self.stopping = false;
                        none(Task::none())
                    }
                }
            }
            DiagMsg::FullSearch => {
                if !self.connected() || self.busy() {
                    return none(Task::none());
                }
                let Some(scan) = self.scan.as_mut().filter(|s| s.done()) else {
                    return none(Task::none());
                };
                scan.add_sweep(Bus::Hs);
                // (an older Ford's other networks too, where anything answered)
                for bus in Bus::OTHERS {
                    if scan.any_on(bus) {
                        scan.add_sweep(bus);
                    }
                }
                let Some(first) = scan.next() else { return none(Task::none()) };
                self.scanning = true;
                self.stopping = false;
                self.error = None;
                let t0 = self.t0();
                self.push_log(format!("{}  ··  Network test: the full search", stamp(t0)));
                note("Network test: full search started");
                none(self.ask_next(first).map(|(a, got)| DiagMsg::Scanned(a, got)))
            }
            DiagMsg::SetTab(t) => {
                if t != Tab::Dashboard || self.dashboard_shown() {
                    self.go(t);
                }
                none(Task::none())
            }
            DiagMsg::ToDashboard => none(self.to_dashboard()),
            DiagMsg::CloseApp(id) => {
                if self.testing == Some(id) {
                    self.stopping = true;
                }
                if let Some(i) = self.apps.iter().position(|a| a.id == id) {
                    self.apps.remove(i);
                    if self.tab == Tab::App(id) {
                        // the tab beside it, else the Dashboard
                        let next = self
                            .apps
                            .get(i)
                            .or_else(|| self.apps.get(i.wrapping_sub(1)))
                            .map(|a| Tab::App(a.id));
                        self.go(next.unwrap_or(Tab::Dashboard));
                    }
                }
                none(Task::none())
            }
            DiagMsg::Topology(on) => {
                self.topology = on;
                none(Task::none())
            }
            DiagMsg::Select(a, bus) => {
                self.selected = if self.selected == Some((a, bus)) { None } else { Some((a, bus)) };
                none(Task::none())
            }
            DiagMsg::OpenSelfTest(target) => {
                // that one's tab, if it's open
                let id = match self.apps.iter().find(|a| a.target == target) {
                    Some(a) => a.id,
                    None => {
                        let id = self.next_app;
                        self.next_app += 1;
                        self.apps.push(SelfTestApp {
                            id,
                            target,
                            test: None,
                            confirm_clear: false,
                        });
                        id
                    }
                };
                self.go(Tab::App(id));
                if self.app(id).is_some_and(|a| a.test.is_none()) {
                    return self.update(DiagMsg::RunSelfTest(id));
                }
                none(Task::none())
            }
            DiagMsg::RunSelfTest(id) => {
                if !self.connected() || self.busy() {
                    return none(Task::none());
                }
                // one module; or the modules the network test found (when it's been run all the way); or the list
                let found: Vec<(modules::Entry, Bus)> = self
                    .scan
                    .as_ref()
                    .filter(|s| s.done() && s.found() > 0)
                    .map(|s| s.modules.iter().filter(|m| m.present()).map(|m| m.entry()).collect())
                    .unwrap_or_default();
                let Some(app) = self.apps.iter_mut().find(|a| a.id == id) else {
                    return none(Task::none());
                };
                let t = match app.target {
                    Some(one) => SelfTest::new(Some(&[one])),
                    None => SelfTest::new((!found.is_empty()).then_some(found.as_slice())),
                };
                let Some(first) = t.next() else { return none(Task::none()) };
                app.test = Some(t);
                app.confirm_clear = false;
                let what = format!("{}: reading codes", app.title());
                self.start_test(id, &what, first)
            }
            DiagMsg::StopSelfTest => {
                self.stopping = self.testing.is_some();
                none(Task::none())
            }
            DiagMsg::AskClear(id) => {
                let idle = self.connected() && !self.busy();
                if let Some(a) = self.apps.iter_mut().find(|a| a.id == id) {
                    a.confirm_clear = idle && a.test.as_ref().is_some_and(|t| t.done());
                }
                none(Task::none())
            }
            DiagMsg::CancelClear(id) => {
                if let Some(a) = self.apps.iter_mut().find(|a| a.id == id) {
                    a.confirm_clear = false;
                }
                none(Task::none())
            }
            DiagMsg::ConfirmClear(id) => {
                let idle = self.connected() && !self.busy();
                let Some(app) = self.apps.iter_mut().find(|a| a.id == id) else {
                    return none(Task::none());
                };
                app.confirm_clear = false;
                if !idle {
                    return none(Task::none());
                }
                let what = format!("{}: clearing codes", app.title());
                let Some(t) = app.test.as_mut() else {
                    return none(Task::none());
                };
                t.clear();
                let Some(first) = t.next() else { return none(Task::none()) };
                self.start_test(id, &what, first)
            }
            DiagMsg::SelfTested(id, asked, r) => {
                if self.testing != Some(id) {
                    return none(Task::none());
                }
                let t0 = self.t0();
                match r {
                    Ok(answer) => {
                        match &answer {
                            Some(a) => self.push_log(line(t0, "←", asked.rx, a)),
                            None => self.push_log(format!("{}  ←  {:03X}  (no answer)", stamp(t0), asked.rx)),
                        }
                        let stopping = self.stopping;
                        let Some(t) = self.apps.iter_mut().find(|a| a.id == id).and_then(|a| a.test.as_mut()) else {
                            // (its tab was closed)
                            self.testing = None;
                            self.stopping = false;
                            return none(Task::none());
                        };
                        t.answer(answer.as_deref());
                        match t.next() {
                            // (a clear isn't left half done: it carries on to reading them again)
                            Some(next) if !stopping || t.clearing() => {
                                none(self.ask_next(next).map(move |(a, got)| DiagMsg::SelfTested(id, a, got)))
                            }
                            _ => {
                                let present = t.modules.iter().filter(|m| m.present()).count();
                                let refused = t.modules.iter().filter(|m| matches!(m.clear, Some(Err(_)))).count();
                                note(&format!(
                                    "Self test {}: {} code(s) in {present} module(s){}",
                                    if stopping { "stopped" } else { "done" },
                                    t.total_codes(),
                                    if refused > 0 {
                                        format!("; {refused} module(s) didn't clear")
                                    } else {
                                        String::new()
                                    }
                                ));
                                self.testing = None;
                                self.stopping = false;
                                none(Task::none())
                            }
                        }
                    }
                    Err(e) => {
                        note(&format!("Self test stopped: {e}"));
                        self.push_log(format!("{}  ··  {e}", stamp(t0)));
                        self.error = Some(e);
                        self.testing = None;
                        self.stopping = false;
                        none(Task::none())
                    }
                }
            }
            DiagMsg::CopyCodes(id) => {
                let Some(t) = self.app(id).and_then(|a| a.test.as_ref()) else {
                    return none(Task::none());
                };
                let text = t
                    .modules
                    .iter()
                    .filter(|m| !m.codes().is_empty())
                    .map(|m| {
                        let codes = m
                            .codes()
                            .iter()
                            .map(|c| format!("{} ({})", c.full(), c.status_words().join(", ")))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("{}: {codes}", tag(m.short, m.address))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                (Task::none(), Out::Copy(if text.is_empty() { "No codes".into() } else { text }))
            }
            DiagMsg::ToggleLog => {
                self.show_log = !self.show_log;
                none(Task::none())
            }
            DiagMsg::CopyLog => (Task::none(), Out::Copy(self.log.join("\n"))),
            DiagMsg::ClearLog => {
                self.log.clear();
                none(Task::none())
            }
            DiagMsg::CopyVin => match &self.vin {
                Some(v) => (Task::none(), Out::Copy(v.clone())),
                None => none(Task::none()),
            },
            DiagMsg::OpenRo(ro) => (Task::none(), Out::OpenRo(ro)),
            DiagMsg::DismissError => {
                self.error = None;
                none(Task::none())
            }
        }
    }

    fn start_test(&mut self, id: u64, what: &str, first: vci::scan::Ask) -> (Task<DiagMsg>, Out) {
        self.testing = Some(id);
        self.stopping = false;
        self.error = None;
        let t0 = self.t0();
        self.push_log(format!("{}  ··  {what}", stamp(t0)));
        // (diagnostics.log: what was done, not which modules)
        note(if what.contains("clearing") {
            "Self test: clearing codes"
        } else {
            "Self test: reading codes"
        });
        (self.ask_next(first).map(move |(a, got)| DiagMsg::SelfTested(id, a, got)), Out::None)
    }

    /// The Dashboard shows once the vehicle's identified (or it's asked for without a VIN)
    pub fn dashboard_shown(&self) -> bool {
        self.vin.is_some() || self.scan.is_some()
    }

    /// On to the Dashboard: its modules found (the network test), unless they have been
    fn to_dashboard(&mut self) -> Task<DiagMsg> {
        if !self.connected() && self.scan.is_none() {
            return Task::none();
        }
        self.go(Tab::Dashboard);
        if self.scan.is_none() {
            let (t, _) = self.update(DiagMsg::RunScan);
            return t;
        }
        Task::none()
    }

    /// One request (logged), and what came back for it
    fn ask_next(&mut self, a: vci::scan::Ask) -> Task<(vci::scan::Ask, Result<Option<Vec<u8>>, String>)> {
        let t0 = self.t0();
        self.push_log(line(t0, "→", a.tx, &a.data));
        let b = self.bridge.clone();
        Task::perform(
            async move {
                let got = match ok(b
                    .call(Request::Ask {
                        tx: a.tx,
                        rx: a.rx,
                        data: a.data.clone(),
                        timeout_ms: a.timeout_ms,
                        pins: a.bus.pins(),
                        baud: a.bus.baud(),
                    })
                    .await)
                {
                    Ok(Reply::Answer { data, .. }) => Ok(data),
                    Ok(other) => Err(format!("The VCI answered oddly ({other:?})")),
                    Err(e) => Err(e),
                };
                (a, got)
            },
            |r| r,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_lines() {
        let t0 = Instant::now();
        let l = line(t0, "→", 0x7E0, &[0x22, 0xF1, 0x90]);
        assert!(l.ends_with("→  7E0  22 F1 90"), "{l}");
    }

    #[test]
    fn picks_the_first_found_and_keeps_a_choice() {
        let mut d = Diag::default();
        let dev = |n: &str| Device {
            name: n.into(),
            ..Default::default()
        };
        let _ = d.update(DiagMsg::Found(Ok(vec![dev("VCM3"), dev("VCMM")])));
        assert_eq!(d.device().map(|d| d.name.as_str()), Some("VCM3"));
        let _ = d.update(DiagMsg::Pick("VCMM".into()));
        let _ = d.update(DiagMsg::Found(Ok(vec![dev("VCM3"), dev("VCMM")])));
        assert_eq!(d.device().map(|d| d.name.as_str()), Some("VCMM"));
    }
}