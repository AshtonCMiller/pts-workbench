//! Live data: OBD-II's own readings (SAE J1979 service 01, "show current data"), which the PCM gives on any vehicle
//! sold in the US since 1996, and on CAN the same way as the rest of the app's requests (01 0C → 41 0C A B).
//!
//! Each reading ([`Pid`]) is shown in the units a US shop uses (°F, mph, psi, miles). The module says which it has
//! (01 00, 01 20, 01 40…: a bit for each of the next 32), so only those are offered.

/// A reading: its number, name, units, how many bytes it comes in, and how to work it out from them
pub struct Pid {
    pub id: u8,
    pub name: &'static str,
    pub unit: &'static str,
    pub len: usize,
    /// Decimal places shown
    pub decimals: usize,
    /// Its usual range (the graph's scale)
    pub range: (f64, f64),
    decode: fn(&[u8]) -> f64,
}

fn f_from_c(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

fn ab(d: &[u8]) -> f64 {
    f64::from(d[0]) * 256.0 + f64::from(d[1])
}

const KPA_PSI: f64 = 0.145_038;
const KM_MI: f64 = 0.621_371;

/// The readings the app knows (J1979's own formulas)
pub const PIDS: &[Pid] = &[
    Pid {
        id: 0x04,
        name: "Engine load (calculated)",
        unit: "%",
        len: 1,
        decimals: 1,
        range: (0.0, 100.0),
        decode: |d| f64::from(d[0]) * 100.0 / 255.0,
    },
    Pid {
        id: 0x05,
        name: "Engine coolant temperature",
        unit: "°F",
        len: 1,
        decimals: 0,
        range: (-40.0, 260.0),
        decode: |d| f_from_c(f64::from(d[0]) - 40.0),
    },
    Pid {
        id: 0x06,
        name: "Short term fuel trim, bank 1",
        unit: "%",
        len: 1,
        decimals: 1,
        range: (-25.0, 25.0),
        decode: |d| (f64::from(d[0]) - 128.0) * 100.0 / 128.0,
    },
    Pid {
        id: 0x07,
        name: "Long term fuel trim, bank 1",
        unit: "%",
        len: 1,
        decimals: 1,
        range: (-25.0, 25.0),
        decode: |d| (f64::from(d[0]) - 128.0) * 100.0 / 128.0,
    },
    Pid {
        id: 0x08,
        name: "Short term fuel trim, bank 2",
        unit: "%",
        len: 1,
        decimals: 1,
        range: (-25.0, 25.0),
        decode: |d| (f64::from(d[0]) - 128.0) * 100.0 / 128.0,
    },
    Pid {
        id: 0x09,
        name: "Long term fuel trim, bank 2",
        unit: "%",
        len: 1,
        decimals: 1,
        range: (-25.0, 25.0),
        decode: |d| (f64::from(d[0]) - 128.0) * 100.0 / 128.0,
    },
    Pid {
        id: 0x0A,
        name: "Fuel pressure",
        unit: "psi",
        len: 1,
        decimals: 1,
        range: (0.0, 110.0),
        decode: |d| f64::from(d[0]) * 3.0 * KPA_PSI,
    },
    Pid {
        id: 0x0B,
        name: "Intake manifold pressure (MAP)",
        unit: "kPa",
        len: 1,
        decimals: 0,
        range: (0.0, 255.0),
        decode: |d| f64::from(d[0]),
    },
    Pid {
        id: 0x0C,
        name: "Engine speed",
        unit: "rpm",
        len: 2,
        decimals: 0,
        range: (0.0, 7000.0),
        decode: |d| ab(d) / 4.0,
    },
    Pid {
        id: 0x0D,
        name: "Vehicle speed",
        unit: "mph",
        len: 1,
        decimals: 0,
        range: (0.0, 100.0),
        decode: |d| f64::from(d[0]) * KM_MI,
    },
    Pid {
        id: 0x0E,
        name: "Ignition timing advance",
        unit: "°",
        len: 1,
        decimals: 1,
        range: (-20.0, 50.0),
        decode: |d| f64::from(d[0]) / 2.0 - 64.0,
    },
    Pid {
        id: 0x0F,
        name: "Intake air temperature",
        unit: "°F",
        len: 1,
        decimals: 0,
        range: (-40.0, 200.0),
        decode: |d| f_from_c(f64::from(d[0]) - 40.0),
    },
    Pid {
        id: 0x10,
        name: "Mass air flow (MAF)",
        unit: "g/s",
        len: 2,
        decimals: 2,
        range: (0.0, 300.0),
        decode: |d| ab(d) / 100.0,
    },
    Pid {
        id: 0x11,
        name: "Throttle position",
        unit: "%",
        len: 1,
        decimals: 1,
        range: (0.0, 100.0),
        decode: |d| f64::from(d[0]) * 100.0 / 255.0,
    },
    Pid {
        id: 0x14,
        name: "O2 sensor voltage, bank 1 sensor 1",
        unit: "V",
        len: 2,
        decimals: 3,
        range: (0.0, 1.275),
        decode: |d| f64::from(d[0]) / 200.0,
    },
    Pid {
        id: 0x15,
        name: "O2 sensor voltage, bank 1 sensor 2",
        unit: "V",
        len: 2,
        decimals: 3,
        range: (0.0, 1.275),
        decode: |d| f64::from(d[0]) / 200.0,
    },
    Pid {
        id: 0x18,
        name: "O2 sensor voltage, bank 2 sensor 1",
        unit: "V",
        len: 2,
        decimals: 3,
        range: (0.0, 1.275),
        decode: |d| f64::from(d[0]) / 200.0,
    },
    Pid {
        id: 0x19,
        name: "O2 sensor voltage, bank 2 sensor 2",
        unit: "V",
        len: 2,
        decimals: 3,
        range: (0.0, 1.275),
        decode: |d| f64::from(d[0]) / 200.0,
    },
    Pid {
        id: 0x1F,
        name: "Run time since engine start",
        unit: "s",
        len: 2,
        decimals: 0,
        range: (0.0, 3600.0),
        decode: ab,
    },
    Pid {
        id: 0x21,
        name: "Distance with the MIL on",
        unit: "mi",
        len: 2,
        decimals: 0,
        range: (0.0, 1000.0),
        decode: |d| ab(d) * KM_MI,
    },
    Pid {
        id: 0x22,
        name: "Fuel rail pressure (to vacuum)",
        unit: "psi",
        len: 2,
        decimals: 1,
        range: (0.0, 120.0),
        decode: |d| ab(d) * 0.079 * KPA_PSI,
    },
    Pid {
        id: 0x23,
        name: "Fuel rail pressure",
        unit: "psi",
        len: 2,
        decimals: 0,
        range: (0.0, 3000.0),
        decode: |d| ab(d) * 10.0 * KPA_PSI,
    },
    Pid {
        id: 0x2C,
        name: "Commanded EGR",
        unit: "%",
        len: 1,
        decimals: 1,
        range: (0.0, 100.0),
        decode: |d| f64::from(d[0]) * 100.0 / 255.0,
    },
    Pid {
        id: 0x2E,
        name: "Commanded evap purge",
        unit: "%",
        len: 1,
        decimals: 1,
        range: (0.0, 100.0),
        decode: |d| f64::from(d[0]) * 100.0 / 255.0,
    },
    Pid {
        id: 0x2F,
        name: "Fuel level",
        unit: "%",
        len: 1,
        decimals: 1,
        range: (0.0, 100.0),
        decode: |d| f64::from(d[0]) * 100.0 / 255.0,
    },
    Pid {
        id: 0x30,
        name: "Warm-ups since codes cleared",
        unit: "",
        len: 1,
        decimals: 0,
        range: (0.0, 255.0),
        decode: |d| f64::from(d[0]),
    },
    Pid {
        id: 0x31,
        name: "Distance since codes cleared",
        unit: "mi",
        len: 2,
        decimals: 0,
        range: (0.0, 10000.0),
        decode: |d| ab(d) * KM_MI,
    },
    Pid {
        id: 0x33,
        name: "Barometric pressure",
        unit: "kPa",
        len: 1,
        decimals: 0,
        range: (60.0, 110.0),
        decode: |d| f64::from(d[0]),
    },
    Pid {
        id: 0x3C,
        name: "Catalyst temperature, bank 1 sensor 1",
        unit: "°F",
        len: 2,
        decimals: 0,
        range: (0.0, 1800.0),
        decode: |d| f_from_c(ab(d) / 10.0 - 40.0),
    },
    Pid {
        id: 0x3D,
        name: "Catalyst temperature, bank 2 sensor 1",
        unit: "°F",
        len: 2,
        decimals: 0,
        range: (0.0, 1800.0),
        decode: |d| f_from_c(ab(d) / 10.0 - 40.0),
    },
    Pid {
        id: 0x42,
        name: "Control module voltage",
        unit: "V",
        len: 2,
        decimals: 2,
        range: (0.0, 16.0),
        decode: |d| ab(d) / 1000.0,
    },
    Pid {
        id: 0x43,
        name: "Absolute load",
        unit: "%",
        len: 2,
        decimals: 1,
        range: (0.0, 100.0),
        decode: |d| ab(d) * 100.0 / 255.0,
    },
    Pid {
        id: 0x44,
        name: "Commanded air-fuel equivalence ratio",
        unit: "λ",
        len: 2,
        decimals: 3,
        range: (0.5, 1.5),
        decode: |d| ab(d) / 32768.0,
    },
    Pid {
        id: 0x45,
        name: "Relative throttle position",
        unit: "%",
        len: 1,
        decimals: 1,
        range: (0.0, 100.0),
        decode: |d| f64::from(d[0]) * 100.0 / 255.0,
    },
    Pid {
        id: 0x46,
        name: "Ambient air temperature",
        unit: "°F",
        len: 1,
        decimals: 0,
        range: (-40.0, 130.0),
        decode: |d| f_from_c(f64::from(d[0]) - 40.0),
    },
    Pid {
        id: 0x47,
        name: "Throttle position B",
        unit: "%",
        len: 1,
        decimals: 1,
        range: (0.0, 100.0),
        decode: |d| f64::from(d[0]) * 100.0 / 255.0,
    },
    Pid {
        id: 0x49,
        name: "Accelerator pedal position D",
        unit: "%",
        len: 1,
        decimals: 1,
        range: (0.0, 100.0),
        decode: |d| f64::from(d[0]) * 100.0 / 255.0,
    },
    Pid {
        id: 0x4A,
        name: "Accelerator pedal position E",
        unit: "%",
        len: 1,
        decimals: 1,
        range: (0.0, 100.0),
        decode: |d| f64::from(d[0]) * 100.0 / 255.0,
    },
    Pid {
        id: 0x4C,
        name: "Commanded throttle actuator",
        unit: "%",
        len: 1,
        decimals: 1,
        range: (0.0, 100.0),
        decode: |d| f64::from(d[0]) * 100.0 / 255.0,
    },
    Pid {
        id: 0x5C,
        name: "Engine oil temperature",
        unit: "°F",
        len: 1,
        decimals: 0,
        range: (-40.0, 300.0),
        decode: |d| f_from_c(f64::from(d[0]) - 40.0),
    },
    Pid {
        id: 0x5E,
        name: "Engine fuel rate",
        unit: "gal/h",
        len: 2,
        decimals: 2,
        range: (0.0, 20.0),
        decode: |d| ab(d) / 20.0 * 0.264_172,
    },
];

/// The readings shown first, when the module has them (what's looked at most)
pub const FIRST: [u8; 8] = [0x0C, 0x05, 0x04, 0x06, 0x07, 0x0F, 0x42, 0x0D];

/// The modules that answer OBD-II's requests (the PCM, and the other powertrain modules on 7E0–7E7)
pub fn answers_obd(address: u32) -> bool {
    (0x7E0..=0x7E7).contains(&address)
}

pub fn pid(id: u8) -> Option<&'static Pid> {
    PIDS.iter().find(|p| p.id == id)
}

impl Pid {
    /// Its value from the module's answer to [`request`] (41 id A B…)
    pub fn value(&self, answer: &[u8]) -> Option<f64> {
        match answer {
            [0x41, id, data @ ..] if *id == self.id && data.len() >= self.len => Some((self.decode)(&data[..self.len])),
            _ => None,
        }
    }

    /// "712", "195", "-3.1"
    pub fn show(&self, v: f64) -> String {
        let s = format!("{v:.*}", self.decimals);
        // (no "-0")
        if s.trim_start_matches('-').trim_start_matches(['0', '.']).is_empty() {
            s.trim_start_matches('-').to_string()
        } else {
            s
        }
    }
}

/// Service 01: a reading
pub fn request(id: u8) -> Vec<u8> {
    vec![0x01, id]
}

/// Which readings the module has: asked from 00, then 20, 40… while the last bit (the next block) is set
#[derive(Debug)]
pub struct Supported {
    pub ids: Vec<u8>,
    next: Option<u8>,
}

impl Default for Supported {
    fn default() -> Self {
        Supported {
            ids: Vec::new(),
            next: Some(0x00),
        }
    }
}

impl Supported {
    /// The next block to ask about (none: all known)
    pub fn next(&self) -> Option<u8> {
        self.next
    }

    /// What the module said to `01 base` (none: it didn't answer: no more blocks)
    pub fn answer(&mut self, answer: Option<&[u8]>) {
        let Some(base) = self.next.take() else { return };
        let Some([0x41, b, a1, a2, a3, a4, ..]) = answer else { return };
        if *b != base {
            return;
        }
        let bits = u32::from_be_bytes([*a1, *a2, *a3, *a4]);
        for i in 0..32u8 {
            if bits & (1 << (31 - i)) != 0 {
                let id = base.wrapping_add(i + 1);
                if i == 31 {
                    // "the next block's supported": ask about it
                    if base < 0xE0 {
                        self.next = Some(base + 0x20);
                    }
                } else {
                    self.ids.push(id);
                }
            }
        }
    }

    /// The ones the app knows how to show
    pub fn known(&self) -> Vec<&'static Pid> {
        PIDS.iter().filter(|p| self.ids.contains(&p.id)).collect()
    }
}

/// A module's answer to `01 base` saying it has these readings (for the demo vehicle)
pub fn supported_answer(base: u8, ids: &[u8]) -> Vec<u8> {
    let mut bits = 0u32;
    for &id in ids {
        if id > base && id <= base.saturating_add(32) && id != base.saturating_add(32) {
            bits |= 1 << (31 - (id - base - 1));
        }
    }
    if ids.iter().any(|&id| id > base.saturating_add(32)) {
        bits |= 1; // the next block
    }
    let b = bits.to_be_bytes();
    vec![0x41, base, b[0], b[1], b[2], b[3]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings() {
        let rpm = pid(0x0C).unwrap();
        assert_eq!(rpm.value(&[0x41, 0x0C, 0x0B, 0x20]), Some(712.0));
        assert_eq!(rpm.show(712.0), "712");
        let ect = pid(0x05).unwrap();
        assert_eq!(ect.value(&[0x41, 0x05, 0x7B]), Some(f_from_c(83.0)));
        assert_eq!(pid(0x06).unwrap().value(&[0x41, 0x06, 0x80]), Some(0.0));
        assert_eq!(pid(0x06).unwrap().show(-0.01), "0.0");
        assert_eq!(pid(0x42).unwrap().value(&[0x41, 0x42, 0x36, 0xB0]), Some(14.0));
        // another reading's answer, or too short
        assert_eq!(rpm.value(&[0x41, 0x0D, 0x00, 0x00]), None);
        assert_eq!(rpm.value(&[0x41, 0x0C, 0x0B]), None);
        assert_eq!(rpm.value(&[0x7F, 0x01, 0x12]), None);
    }

    #[test]
    fn which_it_has() {
        let ids = [0x04, 0x05, 0x0C, 0x0D, 0x20 + 0x02, 0x42, 0x46];
        let mut s = Supported::default();
        while let Some(base) = s.next() {
            let a = supported_answer(base, &ids);
            s.answer(Some(&a));
        }
        assert_eq!(s.ids, ids.to_vec());
        assert_eq!(s.known().len(), 7);
        // no answer: what's known so far
        let mut s = Supported::default();
        s.answer(None);
        assert!(s.next().is_none() && s.ids.is_empty());
    }
}
