//! The demo vehicle: a made-up 2017 F-150 that answers like a real one, so Diagnostics can be built and tried without
//! a VCI or a truck (offered when the app's run from source: a debug build).
//!
//! It's laid out the way the real one is: the PCM, ABS, PSCM, RCM, BCM and PAM on pins 6 / 14; a gateway (GWM) on
//! pins 3 / 11 (HS-CAN2, 500 kbit/s) with the cab's modules behind it; one module that isn't on the app's list (found
//! by the full search). A few modules have codes; clearing them leaves one that's still failing, and the GWM refuses
//! to clear (so every path shows). Its VIN is made up (a serial of zeros).

use std::collections::HashMap;

use crate::modules::{Bus, HS_CAN_BAUD};
use crate::proto::{Reply, Request};

/// The demo vehicle's "driver" (what the app lists it as, and opens)
pub const DEMO_DLL: &str = "demo";
pub const DEMO_NAME: &str = "Demo vehicle";
/// Its VIN (made up: a valid check digit, a serial of zeros)
pub const DEMO_VIN: &str = "1FTEW1EG8HFA00000";

struct Module {
    part: &'static str,
    software: &'static str,
    /// (code bytes, failure type, status)
    codes: Vec<([u8; 2], u8, u8)>,
    /// Codes that come back right after clearing (still failing)
    stays: Vec<([u8; 2], u8, u8)>,
    /// Its refusal to clear (none: it clears)
    no_clear: Option<u8>,
}

/// The demo vehicle, connected
pub struct Sim {
    modules: HashMap<(u32, Bus), Module>,
    bus: Option<Bus>,
    tick: u32,
}

fn m(part: &'static str, software: &'static str) -> Module {
    Module {
        part,
        software,
        codes: Vec::new(),
        stays: Vec::new(),
        no_clear: None,
    }
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}

impl Sim {
    pub fn new() -> Sim {
        let mut modules = HashMap::new();
        let mut add = |a: u32, bus: Bus, mut module: Module, codes: &[([u8; 2], u8, u8)]| {
            module.codes = codes.to_vec();
            modules.insert((a, bus), module);
        };
        // pins 6 / 14
        add(
            0x7E0,
            Bus::Hs,
            m("HL3A-12A650-AXB", "HL3A-14C204-ACD"),
            &[([0x04, 0x20], 0x00, 0x08), ([0x01, 0x71], 0x00, 0x04)],
        );
        add(0x760, Bus::Hs, m("HL34-2C219-AE", "HL34-14C036-AD"), &[([0xC1, 0x00], 0x00, 0x08)]);
        add(0x730, Bus::Hs, m("HL34-3F964-AJ", "HL34-14D003-AF"), &[]);
        add(0x737, Bus::Hs, m("HL3T-14B321-AB", "HL3T-14C028-AB"), &[]);
        add(0x736, Bus::Hs, m("HL3T-15K866-AC", "HL3T-14F682-AA"), &[]);
        let mut bcm = m("HL3T-15604-BCE", "HL3T-14C184-BG");
        // a door ajar circuit still open: comes back after clearing
        bcm.stays = vec![([0x90, 0xD7], 0x15, 0x09)];
        add(0x726, Bus::Hs, bcm, &[([0x90, 0xD7], 0x15, 0x8F), ([0x91, 0x07], 0x13, 0x08)]);
        // pins 3 / 11: the gateway, and what's behind it
        let mut gwm = m("JL3T-14F642-AA", "JL3T-14H241-AB");
        gwm.no_clear = Some(0x22);
        add(0x716, Bus::Hs2, gwm, &[([0xC1, 0x55], 0x00, 0x08)]);
        add(0x720, Bus::Hs2, m("JL3T-10849-FCK", "JL3T-14C026-AE"), &[]);
        add(
            0x7D0,
            Bus::Hs2,
            m("JL3T-14G371-FCG", "JL3T-14G381-AB"),
            &[([0xD3, 0x00], 0x49, 0x08)],
        );
        add(0x727, Bus::Hs2, m("HL3T-18C815-AB", "HL3T-14D099-AA"), &[]);
        add(0x733, Bus::Hs2, m("HL3T-19980-AJ", "HL3T-14C381-AA"), &[]);
        add(0x740, Bus::Hs2, m("HL3T-14B531-AB", "HL3T-14C179-AB"), &[]);
        add(
            0x741,
            Bus::Hs2,
            m("HL3T-14B533-AB", "HL3T-14C179-AB"),
            &[([0x9A, 0x41], 0x13, 0x09)],
        );
        add(0x7A7, Bus::Hs2, m("HL3T-15607-AA", "HL3T-14C543-AA"), &[]);
        add(0x724, Bus::Hs2, m("HL3T-14A664-DA", "HL3T-14C580-AA"), &[]);
        // not on the app's list: found by the full search (a trailer module)
        add(0x7A5, Bus::Hs2, m("HL3T-19H332-AB", "HL3T-14G014-AA"), &[]);
        Sim {
            modules,
            bus: None,
            tick: 0,
        }
    }

    /// Its answer, and how long it takes (milliseconds: none answering takes the whole wait)
    pub fn answer(&mut self, req: &Request) -> (Reply, u64) {
        match req {
            Request::Devices => (Reply::Devices { list: Vec::new() }, 5),
            Request::Open { .. } => (
                Reply::Opened {
                    firmware: "Demo 1.0".into(),
                    dll: "Demo".into(),
                    api: "04.04".into(),
                },
                400,
            ),
            Request::Voltage => {
                self.tick = self.tick.wrapping_add(1);
                // 12.5–12.7 V, moving a little
                (
                    Reply::Voltage {
                        millivolts: 12_540 + (self.tick * 37) % 160,
                    },
                    20,
                )
            }
            Request::Connect { baud, pins } => {
                self.bus = Some(match (*pins, *baud) {
                    (0, _) => Bus::Hs,
                    (0x030B, b) if b == HS_CAN_BAUD => Bus::Hs2,
                    (0x030B, _) => Bus::Ms,
                    (0x080C, _) => Bus::Hs2b,
                    _ => Bus::Hs3,
                });
                (Reply::Done, 150)
            }
            Request::Ask {
                tx,
                data,
                timeout_ms,
                pins,
                baud,
                ..
            } => {
                let baud = if *baud == 0 { HS_CAN_BAUD } else { *baud };
                let bus = match (*pins, baud) {
                    (0, _) => Bus::Hs,
                    (0x030B, b) if b == HS_CAN_BAUD => Bus::Hs2,
                    (0x030B, _) => Bus::Ms,
                    (0x080C, _) => Bus::Hs2b,
                    _ => Bus::Hs3,
                };
                self.bus = Some(bus);
                match self.reply(*tx, bus, data) {
                    Some(r) => (
                        Reply::Answer {
                            data: Some(r),
                            waited_ms: 0,
                        },
                        25,
                    ),
                    None => (Reply::Answer { data: None, waited_ms: 0 }, u64::from(*timeout_ms)),
                }
            }
            Request::Disconnect => {
                self.bus = None;
                (Reply::Done, 20)
            }
            Request::Close => (Reply::Done, 20),
        }
    }

    fn reply(&mut self, tx: u32, bus: Bus, data: &[u8]) -> Option<Vec<u8>> {
        let m = self.modules.get_mut(&(tx, bus))?;
        let text = |head: &[u8], t: &str| {
            let mut v = head.to_vec();
            v.extend_from_slice(t.as_bytes());
            v
        };
        Some(match data {
            [0x22, 0xF1, 0x13] => text(&[0x62, 0xF1, 0x13], m.part),
            [0x22, 0xF1, 0x88] => text(&[0x62, 0xF1, 0x88], m.software),
            [0x22, 0xF1, 0x90] if tx == 0x7E0 => text(&[0x62, 0xF1, 0x90], DEMO_VIN),
            [0x09, 0x02] if tx == 0x7E0 => text(&[0x49, 0x02, 0x01], DEMO_VIN),
            [0x19, 0x02, mask] => {
                let mut v = vec![0x59, 0x02, 0xFF];
                for (code, ft, status) in &m.codes {
                    if status & mask != 0 {
                        v.extend_from_slice(&[code[0], code[1], *ft, *status]);
                    }
                }
                v
            }
            [0x14, 0xFF, 0xFF, 0xFF] => match m.no_clear {
                Some(nrc) => vec![0x7F, 0x14, nrc],
                None => {
                    m.codes = m.stays.clone();
                    vec![0x54]
                }
            },
            [sid, ..] => vec![0x7F, *sid, 0x11],
            [] => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dtc, scan::Scan, uds};

    fn ask(sim: &mut Sim, a: &crate::scan::Ask) -> Option<Vec<u8>> {
        let req = Request::Ask {
            tx: a.tx,
            rx: a.rx,
            data: a.data.clone(),
            timeout_ms: a.timeout_ms,
            pins: a.bus.pins(),
            baud: a.bus.baud(),
        };
        match sim.answer(&req).0 {
            Reply::Answer { data, .. } => data,
            _ => None,
        }
    }

    #[test]
    fn answers_like_the_truck() {
        let mut sim = Sim::new();
        assert!(uds::check_digit_ok(DEMO_VIN));
        let vin = sim.reply(0x7E0, Bus::Hs, &uds::read(uds::VIN)).unwrap();
        assert_eq!(uds::vin_from(&vin).as_deref(), Some(DEMO_VIN));
        // the network test: 6 on pins 6 / 14, then the gateway's network on 3 / 11 (and nothing else asked)
        let mut s = Scan::hs_can();
        while let Some(a) = s.next() {
            let r = ask(&mut sim, &a);
            s.answer(r.as_deref());
            if s.done() && !s.others_asked() && !s.gateway() {
                s.add_other_networks();
            }
        }
        assert_eq!(s.modules.iter().filter(|m| m.present() && m.bus == Bus::Hs).count(), 6);
        assert_eq!(s.modules.iter().filter(|m| m.present() && m.bus == Bus::Hs2).count(), 9);
        assert!(s.gateway());
        assert!(!s.modules.iter().any(|m| m.bus == Bus::Ms || m.bus == Bus::Hs3));
        // the self test, and clearing
        let found: Vec<_> = s.modules.iter().filter(|m| m.present()).map(|m| m.entry()).collect();
        let mut t = dtc::SelfTest::new(Some(&found));
        while let Some(a) = t.next() {
            let r = ask(&mut sim, &a);
            t.answer(r.as_deref());
        }
        assert_eq!(t.total_codes(), 8);
        t.clear();
        while let Some(a) = t.next() {
            let r = ask(&mut sim, &a);
            t.answer(r.as_deref());
        }
        // the BCM's door circuit came back; the GWM wouldn't clear
        assert_eq!(t.total_codes(), 2);
        assert!(t.modules.iter().any(|m| m.address == 0x716 && m.clear == Some(Err(0x22))));
    }
}