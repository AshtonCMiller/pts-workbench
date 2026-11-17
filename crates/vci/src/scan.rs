//! The network test (like IDS / FDRS's): each module on the list asked for its part number, one after another. One
//! that answers is there (and its software is asked too); one that refuses is there but won't say; one that says
//! nothing in time isn't there, or isn't talking.
//!
//! Step by step, so the app can show where it's got to: [`Scan::next`] is the next request to make,
//! [`Scan::answer`] what came back.

use crate::modules;
use crate::uds::{self, Answer};

/// How long a module has to answer (it can ask for longer: the helper waits through that)
pub const TIMEOUT_MS: u32 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Not asked yet
    Waiting,
    Answered,
    /// It's there, but refused to say (its refusal)
    Refused(u8),
    NoAnswer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub address: u32,
    pub short: &'static str,
    pub name: &'static str,
    pub status: Status,
    pub part: String,
    pub software: String,
}

impl Module {
    pub fn present(&self) -> bool {
        matches!(self.status, Status::Answered | Status::Refused(_))
    }
}

/// A request to make: to `tx`, answered at `rx`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub tx: u32,
    pub rx: u32,
    pub data: Vec<u8>,
    pub timeout_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Part,
    Software,
}

#[derive(Debug, Clone)]
pub struct Scan {
    pub modules: Vec<Module>,
    at: usize,
    step: Step,
}

impl Scan {
    /// Every module on the high-speed CAN list
    pub fn hs_can() -> Self {
        Scan {
            modules: modules::HS_CAN
                .iter()
                .map(|&(address, short, name)| Module {
                    address,
                    short,
                    name,
                    status: Status::Waiting,
                    part: String::new(),
                    software: String::new(),
                })
                .collect(),
            at: 0,
            step: Step::Part,
        }
    }

    pub fn done(&self) -> bool {
        self.at >= self.modules.len()
    }

    /// (asked so far, how many), and the one being asked
    pub fn progress(&self) -> (usize, usize, Option<&Module>) {
        (self.at.min(self.modules.len()), self.modules.len(), self.modules.get(self.at))
    }

    pub fn found(&self) -> usize {
        self.modules.iter().filter(|m| m.present()).count()
    }

    /// The next request to make (none: the test's done)
    pub fn next(&self) -> Option<Ask> {
        let m = self.modules.get(self.at)?;
        let did = match self.step {
            Step::Part => uds::PART,
            Step::Software => uds::SOFTWARE,
        };
        Some(Ask {
            tx: m.address,
            rx: modules::answers_at(m.address),
            data: uds::read(did),
            timeout_ms: TIMEOUT_MS,
        })
    }

    /// What came back for the last request (none: no answer in time)
    pub fn answer(&mut self, reply: Option<&[u8]>) {
        let step = self.step;
        let Some(m) = self.modules.get_mut(self.at) else { return };
        match (step, reply) {
            (Step::Part, None) => {
                m.status = Status::NoAnswer;
                self.next_module();
            }
            (Step::Part, Some(r)) => match uds::answer_to_read(uds::PART, r) {
                Answer::Data(d) => {
                    m.status = Status::Answered;
                    m.part = uds::text(&d);
                    self.step = Step::Software;
                }
                Answer::Refused(code) => {
                    m.status = Status::Refused(code);
                    // (it may still say what software it has)
                    self.step = Step::Software;
                }
                // it answered: it's there
                Answer::Odd(_) => {
                    m.status = Status::Answered;
                    self.step = Step::Software;
                }
            },
            (Step::Software, r) => {
                if let Some(Answer::Data(d)) = r.map(|r| uds::answer_to_read(uds::SOFTWARE, r)) {
                    m.software = uds::text(&d);
                    if matches!(m.status, Status::Refused(_)) {
                        m.status = Status::Answered;
                    }
                }
                self.next_module();
            }
        }
    }

    fn next_module(&mut self) {
        self.at += 1;
        self.step = Step::Part;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_each_module_in_turn() {
        let mut s = Scan::hs_can();
        let first = s.next().unwrap();
        assert_eq!((first.tx, first.rx, first.data.clone()), (0x7E0, 0x7E8, vec![0x22, 0xF1, 0x13]));
        // the PCM answers: its software is asked next
        let mut part = vec![0x62, 0xF1, 0x13];
        part.extend(b"NZ6A-12A650-AB");
        s.answer(Some(&part));
        assert_eq!(s.next().unwrap().data, vec![0x22, 0xF1, 0x88]);
        let mut sw = vec![0x62, 0xF1, 0x88];
        sw.extend(b"NZ6A-14C204-AAA");
        s.answer(Some(&sw));
        // the TCM: nothing
        assert_eq!(s.next().unwrap().tx, 0x7E1);
        s.answer(None);
        // the BECM: refuses both
        s.answer(Some(&[0x7F, 0x22, 0x31]));
        s.answer(Some(&[0x7F, 0x22, 0x31]));
        let m = &s.modules;
        assert_eq!(
            (m[0].status, m[0].part.as_str(), m[0].software.as_str()),
            (Status::Answered, "NZ6A-12A650-AB", "NZ6A-14C204-AAA")
        );
        assert_eq!(m[1].status, Status::NoAnswer);
        assert_eq!(m[2].status, Status::Refused(0x31));
        assert_eq!(s.found(), 2);
        while !s.done() {
            s.answer(None);
        }
        assert_eq!(s.next(), None);
        assert_eq!(s.progress().0, s.modules.len());
    }
}

