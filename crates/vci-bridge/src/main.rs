//! `pts-vci32`: the 32-bit helper the app talks to vehicles through. Ford's J2534 drivers (VCM 3, VCMM: Bosch's
//! `BVTX4J32.dll`) are 32-bit only, and a 64-bit program can't load them; so the app starts this, built for 32-bit
//! Windows (`--target i686-pc-windows-msvc`), and asks it things one line of JSON at a time (crates/vci/proto.rs).
//!
//!   registry.rs  the J2534 interfaces Windows has (HKLM\SOFTWARE\PassThruSupport.04.04, as a 32-bit program sees it)
//!   j2534.rs     the J2534 (04.04) calls: open the interface, its voltage, join the CAN bus, a diagnostic request
//!
//! It quits when the app closes its input (or asks it to close).

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io::{BufRead, Write};

use vci::proto::{Reply, Request};

#[cfg(windows)]
mod j2534;
#[cfg(windows)]
mod registry;

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    let mut vci = Vci::default();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let (reply, quit) = match serde_json::from_str::<Request>(&line) {
            Ok(Request::Close) => (vci.close(), true),
            Ok(req) => (vci.handle(req), false),
            Err(e) => (
                Reply::Error {
                    text: format!("Not a request: {e}"),
                },
                false,
            ),
        };
        let text = serde_json::to_string(&reply).unwrap_or_else(|_| r#"{"reply":"error","text":"?"}"#.into());
        if writeln!(out, "{text}").and_then(|_| out.flush()).is_err() || quit {
            break;
        }
    }
    vci.close();
}

/// The interface, while it's open
#[derive(Default)]
struct Vci {
    #[cfg(windows)]
    open: Option<j2534::Interface>,
}

#[cfg(windows)]
impl Vci {
    fn handle(&mut self, req: Request) -> Reply {
        let r = match req {
            Request::Devices => Ok(Reply::Devices { list: registry::devices() }),
            Request::Open { dll } => {
                self.open = None;
                j2534::Interface::open(&dll).map(|i| {
                    let (firmware, dll, api) = i.versions.clone();
                    self.open = Some(i);
                    Reply::Opened { firmware, dll, api }
                })
            }
            Request::Voltage => self.with(|i| i.voltage().map(|millivolts| Reply::Voltage { millivolts })),
            Request::Connect { baud, ms_can } => self.with(|i| i.connect(baud, ms_can).map(|_| Reply::Done)),
            Request::Ask {
                tx,
                rx,
                data,
                timeout_ms,
                ms_can,
            } => self.with(|i| {
                i.ask(tx, rx, &data, timeout_ms, ms_can)
                    .map(|(data, waited_ms)| Reply::Answer { data, waited_ms })
            }),
            Request::Disconnect => self.with(|i| {
                i.disconnect();
                Ok(Reply::Done)
            }),
            Request::Close => Ok(self.close()),
        };
        r.unwrap_or_else(|text| Reply::Error { text })
    }

    fn with(&mut self, f: impl FnOnce(&mut j2534::Interface) -> Result<Reply, String>) -> Result<Reply, String> {
        match self.open.as_mut() {
            Some(i) => f(i),
            None => Err("The VCI isn't open".into()),
        }
    }

    fn close(&mut self) -> Reply {
        // (closing it leaves the bus and lets go of the VCI)
        self.open = None;
        Reply::Done
    }
}

#[cfg(not(windows))]
impl Vci {
    fn handle(&mut self, req: Request) -> Reply {
        match req {
            Request::Devices => Reply::Devices { list: Vec::new() },
            _ => Reply::Error {
                text: "Talking to a VCI needs Windows (Ford's J2534 drivers)".into(),
            },
        }
    }

    fn close(&mut self) -> Reply {
        Reply::Done
    }
}