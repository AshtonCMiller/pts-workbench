//! Diagnostics: the vehicle plugged in through Ford's VCI (VCM 3, or the VCMM), the way IDS / FDRS reach it.
//!
//!   1. Find the VCI: the J2534 interfaces Windows has (Ford's VCI Software installs them), the VCM 3 picked first.
//!   2. Connect: the VCI opened, the battery voltage read, the high-speed CAN bus joined (OBD pins 6 / 14).
//!   3. Identify: the PCM asked for the VIN (22 F190; OBD's 09 02 if it won't say), matched to your ROs.
//!   4. Network test: each module on the bus asked for its part number and software (crates/vci/scan.rs).
//!   5. Self test: each module's trouble codes; and clearing them, when you ask (crates/vci/dtc.rs).
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
use vci::modules;
use vci::proto::{Device, Reply, Request};
use vci::scan::Scan;
use vci::uds;

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
    /// The self test: each module's codes
    pub selftest: Option<SelfTest>,
    pub testing: bool,
    /// "Clear all codes?" showing
    pub confirm_clear: bool,
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
            selftest: None,
            testing: false,
            confirm_clear: false,
            stopping: false,
            error: None,
            log: Vec::new(),
            show_log: false,
            started: None,
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
    /// Self test: read every module's codes; clear them (asks first); one module's answer
    RunSelfTest,
    StopSelfTest,
    AskClear,
    CancelClear,
    ConfirmClear,
    SelfTested(vci::scan::Ask, Result<Option<Vec<u8>>, String>),
    /// The codes found, as text (for the RO's story)
    CopyCodes,
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
    match ok(b.call(Request::Ask { tx, rx, data, timeout_ms }).await)? {
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
        self.looking || self.link == Link::Connecting || self.identifying || self.scanning || self.testing
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
                        match ok(b.call(Request::Devices).await)? {
                            Reply::Devices { list } => Ok(list),
                            _ => Ok(Vec::new()),
                        }
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
                self.stopping = self.scanning || self.testing;
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
                self.testing = false;
                self.confirm_clear = false;
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
                if !self.connected() || self.identifying || self.scanning || self.testing {
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
                        self.vin = id.vin;
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
                let t0 = self.t0();
                self.push_log(format!("{}  ··  Network test (high-speed CAN)", stamp(t0)));
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
            DiagMsg::RunSelfTest => {
                if !self.connected() || self.busy() {
                    return none(Task::none());
                }
                // only the modules the network test found, when it's been run all the way
                let found: Option<Vec<u32>> = self
                    .scan
                    .as_ref()
                    .filter(|s| s.done() && s.found() > 0)
                    .map(|s| s.modules.iter().filter(|m| m.present()).map(|m| m.address).collect());
                let t = SelfTest::new(found.as_deref());
                let Some(first) = t.next() else { return none(Task::none()) };
                self.selftest = Some(t);
                self.start_test("Self test: reading codes", first)
            }
            DiagMsg::StopSelfTest => {
                self.stopping = self.testing;
                none(Task::none())
            }
            DiagMsg::AskClear => {
                self.confirm_clear = self.connected() && !self.busy() && self.selftest.as_ref().is_some_and(|t| t.done());
                none(Task::none())
            }
            DiagMsg::CancelClear => {
                self.confirm_clear = false;
                none(Task::none())
            }
            DiagMsg::ConfirmClear => {
                self.confirm_clear = false;
                if !self.connected() || self.busy() {
                    return none(Task::none());
                }
                let Some(t) = self.selftest.as_mut() else {
                    return none(Task::none());
                };
                t.clear();
                let Some(first) = t.next() else { return none(Task::none()) };
                self.start_test("Self test: clearing codes", first)
            }
            DiagMsg::SelfTested(asked, r) => {
                if !self.testing {
                    return none(Task::none());
                }
                let t0 = self.t0();
                match r {
                    Ok(answer) => {
                        match &answer {
                            Some(a) => self.push_log(line(t0, "←", asked.rx, a)),
                            None => self.push_log(format!("{}  ←  {:03X}  (no answer)", stamp(t0), asked.rx)),
                        }
                        let Some(t) = self.selftest.as_mut() else {
                            return none(Task::none());
                        };
                        t.answer(answer.as_deref());
                        match t.next() {
                            // (a clear isn't left half done: it carries on to reading them again)
                            Some(next) if !self.stopping || t.clearing() => {
                                none(self.ask_next(next).map(|(a, got)| DiagMsg::SelfTested(a, got)))
                            }
                            _ => {
                                let present = t.modules.iter().filter(|m| m.present()).count();
                                let refused = t.modules.iter().filter(|m| matches!(m.clear, Some(Err(_)))).count();
                                note(&format!(
                                    "Self test {}: {} code(s) in {present} module(s){}",
                                    if self.stopping { "stopped" } else { "done" },
                                    t.total_codes(),
                                    if refused > 0 {
                                        format!("; {refused} module(s) didn't clear")
                                    } else {
                                        String::new()
                                    }
                                ));
                                self.testing = false;
                                self.stopping = false;
                                none(Task::none())
                            }
                        }
                    }
                    Err(e) => {
                        note(&format!("Self test stopped: {e}"));
                        self.push_log(format!("{}  ··  {e}", stamp(t0)));
                        self.error = Some(e);
                        self.testing = false;
                        self.stopping = false;
                        none(Task::none())
                    }
                }
            }
            DiagMsg::CopyCodes => {
                let Some(t) = &self.selftest else { return none(Task::none()) };
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
                        format!("{}: {codes}", m.short)
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

    fn start_test(&mut self, what: &str, first: vci::scan::Ask) -> (Task<DiagMsg>, Out) {
        self.testing = true;
        self.stopping = false;
        self.error = None;
        let t0 = self.t0();
        self.push_log(format!("{}  ··  {what}", stamp(t0)));
        note(what);
        (self.ask_next(first).map(|(a, got)| DiagMsg::SelfTested(a, got)), Out::None)
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
