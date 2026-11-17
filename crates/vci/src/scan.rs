//! The network test (like IDS / FDRS's): each module on the list asked for its part number, one after another. One
//! that answers is there (and its software is asked too); one that refuses is there but won't say; one that says
//! nothing in time isn't there, or isn't talking.
//!
//! Step by step, so the app can show where it's got to: [`Scan::next`] is the next request to make,
//! [`Scan::answer`] what came back.
//!
//! It goes in passes, each added when the one before is done ([`Scan::add`]): the modules on the list on pins 6 / 14;
//! then, on an older Ford (no gateway answered), the list again on pins 3 / 11; and, when asked, the full search (every
//! other diagnostic address, more quickly).

use crate::modules::{self, Bus, Entry};
use crate::uds::{self, Answer};

/// How long a module has to answer (it can ask for longer: the helper waits through that)
pub const TIMEOUT_MS: u32 = 300;
/// The full search's (most addresses have nothing: a module answers well within this)
pub const SWEEP_TIMEOUT_MS: u32 = 120;

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
    /// Where it was asked
    pub bus: Bus,
    /// On the app's list (not found by the full search)
    pub listed: bool,
    pub status: Status,
    pub part: String,
    pub software: String,
    timeout_ms: u32,
}

impl Module {
    pub fn present(&self) -> bool {
        matches!(self.status, Status::Answered | Status::Refused(_))
    }

    /// (address, short name, name) and where, for the self test
    pub fn entry(&self) -> (Entry, Bus) {
        ((self.address, self.short, self.name), self.bus)
    }
}

/// A request to make: to `tx`, answered at `rx`, on `bus`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub tx: u32,
    pub rx: u32,
    pub data: Vec<u8>,
    pub timeout_ms: u32,
    pub bus: Bus,
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
    /// Every module on the list, on pins 6 / 14
    pub fn hs_can() -> Self {
        let mut s = Scan {
            modules: Vec::new(),
            at: 0,
            step: Step::Part,
        };
        s.add(Bus::Hs, modules::MODULES, true, TIMEOUT_MS);
        s
    }

    /// Another pass: these modules, on that network (one already found there isn't asked again)
    pub fn add(&mut self, bus: Bus, list: &[Entry], listed: bool, timeout_ms: u32) {
        for &(address, short, name) in list {
            if self.modules.iter().any(|m| m.address == address && m.bus == bus) {
                continue;
            }
            self.modules.push(Module {
                address,
                short,
                name,
                bus,
                listed,
                status: Status::Waiting,
                part: String::new(),
                software: String::new(),
                timeout_ms,
            });
        }
    }

    /// The list again on pins 3 / 11 (an older Ford's medium-speed CAN), but for the modules found on pins 6 / 14
    pub fn add_ms_can(&mut self) {
        let found: Vec<u32> = self.modules.iter().filter(|m| m.present()).map(|m| m.address).collect();
        let list: Vec<Entry> = modules::MODULES.iter().copied().filter(|m| !found.contains(&m.0)).collect();
        self.add(Bus::Ms, &list, true, TIMEOUT_MS);
    }

    /// The full search on a network: every other diagnostic address (not where a module found answers)
    pub fn add_sweep(&mut self, bus: Bus) {
        let answers: Vec<u32> = self
            .modules
            .iter()
            .filter(|m| m.present() && m.bus == bus)
            .map(|m| modules::answers_at(m.address))
            .collect();
        let list: Vec<Entry> = modules::sweep()
            .into_iter()
            .filter(|a| !answers.contains(a))
            .map(|a| (a, modules::UNLISTED.0, modules::UNLISTED.1))
            .collect();
        self.add(bus, &list, false, SWEEP_TIMEOUT_MS);
    }

    /// Whether the gateway answered (every module is reached through it)
    pub fn gateway(&self) -> bool {
        self.modules.iter().any(|m| m.address == modules::GWM && m.present())
    }

    /// Whether anything answered on that network
    pub fn any_on(&self, bus: Bus) -> bool {
        self.modules.iter().any(|m| m.bus == bus && m.present())
    }

    /// The rest of a pass that can't be asked (that network can't be reached): left as not answering
    pub fn skip(&mut self, bus: Bus) {
        while let Some(m) = self.modules.get_mut(self.at).filter(|m| m.bus == bus) {
            m.status = Status::NoAnswer;
            self.next_module();
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
            timeout_ms: m.timeout_ms,
            bus: m.bus,
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
        // a module found by the full search: the address it answers at isn't asked (it's not a module's)
        if let Some(m) = self.modules.get(self.at).filter(|m| m.present()) {
            let (answer, bus) = (modules::answers_at(m.address), m.bus);
            if let Some(i) = self.modules[self.at + 1..]
                .iter()
                .position(|x| x.address == answer && x.bus == bus && x.status == Status::Waiting)
            {
                self.modules.remove(self.at + 1 + i);
            }
        }
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
        assert!(!s.gateway());
        // an older Ford: the list again on pins 3 / 11, but for the two found
        s.add_ms_can();
        let a = s.next().unwrap();
        assert_eq!((a.tx, a.bus), (0x7E1, Bus::Ms));
        assert_eq!(s.modules.len(), 2 * modules::MODULES.len() - 2);
        s.skip(Bus::Ms);
        assert!(s.done());
        // the full search: a module at 7A5 answers, so 7AD (where it answers) isn't asked
        s.add_sweep(Bus::Hs);
        while let Some(a) = s.next() {
            assert_ne!(a.tx, 0x7AD);
            assert_eq!(a.timeout_ms, SWEEP_TIMEOUT_MS);
            if a.tx == 0x7A5 {
                let mut p = vec![0x62, 0xF1, 0x13];
                p.extend(b"PU5T-14F642-AA");
                s.answer(Some(&p));
                s.answer(None);
            } else {
                s.answer(None);
            }
        }
        let found = s.modules.iter().find(|m| m.address == 0x7A5).unwrap();
        assert!(found.present() && !found.listed);
    }
}