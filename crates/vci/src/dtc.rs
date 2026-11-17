//! Self test: each module's trouble codes (UDS ReadDTCInformation, 19 02), and clearing them (ClearDiagnosticInformation,
//! 14 FF FF FF), the way IDS / FDRS's self test does.
//!
//! [`SelfTest`] goes step by step like the network test: [`SelfTest::next`] is the next request, [`SelfTest::answer`]
//! what came back. Clearing asks each module that answered to clear, then reads them all again.

use crate::modules;
use crate::scan::Ask;

/// The statuses asked for: failed now, failed this drive cycle, pending, confirmed (stored), warning light on.
/// (Not the "test not run yet" bits: with those, some modules list every code they could ever set.)
pub const STATUS_MASK: u8 = 0x8F;

/// How long a module has to answer (a long list of codes takes a moment)
pub const TIMEOUT_MS: u32 = 600;

/// ReadDTCInformation: the codes with any of [`STATUS_MASK`]'s statuses
pub fn read_request() -> Vec<u8> {
    vec![0x19, 0x02, STATUS_MASK]
}

/// ClearDiagnosticInformation: every code
pub fn clear_request() -> Vec<u8> {
    vec![0x14, 0xFF, 0xFF, 0xFF]
}

/// A trouble code as Ford shows it: "B10D7:15" (the code, its failure type) and its status
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dtc {
    /// "B10D7"
    pub code: String,
    /// The failure type byte (0x15: "circuit short to battery or open")
    pub ft: u8,
    pub status: u8,
}

impl Dtc {
    /// "B10D7:15"
    pub fn full(&self) -> String {
        format!("{}:{:02X}", self.code, self.ft)
    }

    /// Failing right now
    pub fn current(&self) -> bool {
        self.status & 0x01 != 0
    }

    /// What its status says, in words ("Current, Stored, Warning light")
    pub fn status_words(&self) -> Vec<&'static str> {
        let mut w = Vec::new();
        if self.status & 0x01 != 0 {
            w.push("Current");
        } else if self.status & 0x02 != 0 {
            w.push("Failed this drive");
        }
        if self.status & 0x04 != 0 {
            w.push("Pending");
        }
        if self.status & 0x08 != 0 {
            w.push(if self.status & 0x01 != 0 { "Stored" } else { "History" });
        }
        if self.status & 0x80 != 0 {
            w.push("Warning light");
        }
        w
    }
}

/// The code from its first two bytes: the top two bits are the letter (P C B U), then the digits
pub fn code(hi: u8, lo: u8) -> String {
    let letter = ['P', 'C', 'B', 'U'][usize::from(hi >> 6)];
    format!("{letter}{}{:X}{lo:02X}", (hi >> 4) & 0x03, hi & 0x0F)
}

/// What a module said to 19 02
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Codes {
    List(Vec<Dtc>),
    /// It refused (its refusal)
    Refused(u8),
    Odd,
}

pub fn parse(r: &[u8]) -> Codes {
    match r {
        [0x59, 0x02, _available, records @ ..] => Codes::List(
            records
                .chunks_exact(4)
                .filter(|c| c[3] & STATUS_MASK != 0)
                .map(|c| Dtc {
                    code: code(c[0], c[1]),
                    ft: c[2],
                    status: c[3],
                })
                .collect(),
        ),
        [0x7F, 0x19, nrc, ..] => Codes::Refused(*nrc),
        _ => Codes::Odd,
    }
}

/// Whether a clear worked (none: it did; some: its refusal, or 0 for an odd answer)
pub fn cleared(r: &[u8]) -> Option<u8> {
    match r {
        [0x54, ..] => None,
        [0x7F, 0x14, nrc, ..] => Some(*nrc),
        _ => Some(0),
    }
}

/// The failure type in words (SAE J2012's), where it's a common one
pub fn failure_type(ft: u8) -> Option<&'static str> {
    Some(match ft {
        0x00 => return None,
        0x01 => "general electrical failure",
        0x02 => "general signal failure",
        0x04 => "system internal failure",
        0x11 => "circuit short to ground",
        0x12 => "circuit short to battery",
        0x13 => "circuit open",
        0x14 => "circuit short to ground or open",
        0x15 => "circuit short to battery or open",
        0x16 => "circuit voltage below threshold",
        0x17 => "circuit voltage above threshold",
        0x1C => "circuit voltage out of range",
        0x1D => "circuit current out of range",
        0x23 => "signal stuck low",
        0x24 => "signal stuck high",
        0x29 => "signal invalid",
        0x2F => "signal erratic",
        0x31 => "no signal",
        0x49 => "internal electronic failure",
        0x4B => "over temperature",
        0x54 => "missing calibration",
        0x55 => "not configured",
        0x62 => "signal compare failure",
        0x64 => "signal plausibility failure",
        0x68 => "event information",
        0x81 => "invalid serial data received",
        0x86 => "signal invalid",
        0x87 => "missing message",
        0x88 => "bus off",
        0x92 => "performance or incorrect operation",
        0x96 => "component internal failure",
        _ => return None,
    })
}

// ---------- the self test ----------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    Waiting,
    Codes(Vec<Dtc>),
    Refused(u8),
    NoAnswer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub address: u32,
    pub short: &'static str,
    pub name: &'static str,
    pub result: Found,
    /// Clearing: how it went (none: not asked; Ok; Err(its refusal))
    pub clear: Option<Result<(), u8>>,
}

impl Module {
    pub fn present(&self) -> bool {
        matches!(self.result, Found::Codes(_) | Found::Refused(_))
    }

    pub fn codes(&self) -> &[Dtc] {
        match &self.result {
            Found::Codes(c) => c,
            _ => &[],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pass {
    Clear,
    Read,
}

#[derive(Debug, Clone)]
pub struct SelfTest {
    pub modules: Vec<Module>,
    at: usize,
    pass: Pass,
}

impl SelfTest {
    /// Every module on the high-speed CAN list, or only these (the ones the network test found)
    pub fn new(only: Option<&[u32]>) -> Self {
        SelfTest {
            modules: modules::HS_CAN
                .iter()
                .filter(|(a, ..)| only.is_none_or(|o| o.contains(a)))
                .map(|&(address, short, name)| Module {
                    address,
                    short,
                    name,
                    result: Found::Waiting,
                    clear: None,
                })
                .collect(),
            at: 0,
            pass: Pass::Read,
        }
    }

    /// Clears the codes of every module that answered, then reads them all again
    pub fn clear(&mut self) {
        self.modules.retain(Module::present);
        for m in &mut self.modules {
            m.clear = None;
        }
        self.at = 0;
        self.pass = Pass::Clear;
    }

    pub fn clearing(&self) -> bool {
        self.pass == Pass::Clear && !self.done()
    }

    pub fn done(&self) -> bool {
        self.at >= self.modules.len()
    }

    /// (asked so far, how many), and the one being asked
    pub fn progress(&self) -> (usize, usize, Option<&Module>) {
        (self.at.min(self.modules.len()), self.modules.len(), self.modules.get(self.at))
    }

    pub fn total_codes(&self) -> usize {
        self.modules.iter().map(|m| m.codes().len()).sum()
    }

    pub fn next(&self) -> Option<Ask> {
        let m = self.modules.get(self.at)?;
        Some(Ask {
            tx: m.address,
            rx: modules::answers_at(m.address),
            data: match self.pass {
                Pass::Clear => clear_request(),
                Pass::Read => read_request(),
            },
            timeout_ms: match self.pass {
                // clearing can take a module a moment (it says when it's working on it)
                Pass::Clear => 2000,
                Pass::Read => TIMEOUT_MS,
            },
        })
    }

    pub fn answer(&mut self, reply: Option<&[u8]>) {
        let pass = self.pass;
        let Some(m) = self.modules.get_mut(self.at) else { return };
        match pass {
            Pass::Read => {
                m.result = match reply.map(parse) {
                    None => Found::NoAnswer,
                    Some(Codes::List(c)) => Found::Codes(c),
                    Some(Codes::Refused(n)) => Found::Refused(n),
                    // it answered: it's there, with nothing readable
                    Some(Codes::Odd) => Found::Codes(Vec::new()),
                };
            }
            Pass::Clear => {
                m.clear = Some(match reply.map(cleared) {
                    Some(None) => Ok(()),
                    Some(Some(n)) => Err(n),
                    None => Err(0),
                });
            }
        }
        self.at += 1;
        // cleared them all: now read them again
        if pass == Pass::Clear && self.done() {
            self.at = 0;
            self.pass = Pass::Read;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes() {
        assert_eq!(code(0xC1, 0x00), "U0100");
        assert_eq!(code(0x90, 0xD7), "B10D7");
        assert_eq!(code(0x04, 0x20), "P0420");
        assert_eq!(code(0x45, 0x61), "C0561");
        assert_eq!(code(0x3A, 0x0F), "P3A0F");
    }

    #[test]
    fn reads_a_list() {
        // two codes, and one only "not run yet" (left out)
        let r = [
            0x59, 0x02, 0xFF, 0x90, 0xD7, 0x15, 0x09, 0xC1, 0x00, 0x00, 0x08, 0x04, 0x20, 0x00, 0x50,
        ];
        let Codes::List(l) = parse(&r) else { panic!() };
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].full(), "B10D7:15");
        assert_eq!(l[0].status_words(), vec!["Current", "Stored"]);
        assert_eq!(l[1].status_words(), vec!["History"]);
        assert_eq!(parse(&[0x7F, 0x19, 0x11]), Codes::Refused(0x11));
        assert_eq!(cleared(&[0x54]), None);
        assert_eq!(cleared(&[0x7F, 0x14, 0x22]), Some(0x22));
    }

    #[test]
    fn clears_then_reads_again() {
        let mut t = SelfTest::new(Some(&[0x7E0, 0x726]));
        assert_eq!(t.modules.len(), 2);
        t.answer(Some(&[0x59, 0x02, 0xFF, 0x04, 0x20, 0x00, 0x08]));
        t.answer(None);
        assert!(t.done());
        assert_eq!(t.total_codes(), 1);
        t.clear();
        assert_eq!(t.modules.len(), 1); // (only the PCM answered)
        assert_eq!(t.next().unwrap().data, clear_request());
        t.answer(Some(&[0x54]));
        assert!(!t.done());
        assert_eq!(t.next().unwrap().data, read_request());
        t.answer(Some(&[0x59, 0x02, 0xFF]));
        assert!(t.done());
        assert_eq!(t.total_codes(), 0);
        assert_eq!(t.modules[0].clear, Some(Ok(())));
    }
}
