//! What the app asks the 32-bit helper (`pts-vci32`), and its answers: one line of JSON each way, an answer for each
//! request, in order.

use serde::{Deserialize, Serialize};

/// A J2534 interface as Windows lists it (the registry's PassThruSupport.04.04)
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Device {
    /// "VCM3"
    pub name: String,
    /// "Bosch"
    pub vendor: String,
    /// Its J2534 driver
    pub dll: String,
    /// CAN with ISO 15765 (diagnostics on CAN), and on other pins (Ford's MS-CAN / HS-CAN3 on pins 3 / 11)
    pub iso15765: bool,
    pub iso15765_ps: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// The J2534 interfaces installed
    Devices,
    /// Loads a driver and opens its interface (the VCI plugged in, by USB or Wi-Fi as VCI Manager set it up)
    Open { dll: String },
    /// The battery voltage at the OBD port
    Voltage,
    /// Joins one of the vehicle's CAN networks for diagnostics (ISO 15765): on pins 6 / 14 (`pins` 0), or on other
    /// pins (J2534's J1962_PINS: 0x030B is pins 3 / 11)
    Connect {
        baud: u32,
        #[serde(default, skip_serializing_if = "is_zero")]
        pins: u32,
    },
    /// One diagnostic request to a module (`tx`: its address, `rx`: its answers'), and its answer (the "wait"
    /// answers a module gives while it works on it are waited through). On the network at `pins` and `baud` (none:
    /// pins 6 / 14 at 500 kbit/s): the helper moves to it first when it's on another.
    Ask {
        tx: u32,
        rx: u32,
        data: Vec<u8>,
        timeout_ms: u32,
        #[serde(default, skip_serializing_if = "is_zero")]
        pins: u32,
        #[serde(default, skip_serializing_if = "is_zero")]
        baud: u32,
    },
    /// Leaves the bus
    Disconnect,
    /// Closes the interface (and leaves the bus)
    Close,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl Request {
    /// How long its answer can take (the app gives up after this)
    pub fn time_limit_ms(&self) -> u64 {
        match self {
            // (a module can ask for more time, up to a few times)
            Request::Ask { timeout_ms, .. } => u64::from(*timeout_ms) + 30_000,
            // finding the VCI (USB, or Wi-Fi)
            Request::Open { .. } => 60_000,
            _ => 20_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "snake_case")]
pub enum Reply {
    Devices {
        list: Vec<Device>,
    },
    Opened {
        firmware: String,
        dll: String,
        api: String,
    },
    Voltage {
        millivolts: u32,
    },
    /// The module's answer (none: it didn't answer in time); how long it asked to wait
    Answer {
        data: Option<Vec<u8>>,
        waited_ms: u32,
    },
    Done,
    /// What went wrong (the driver's own words where it has them)
    Error {
        text: String,
    },
}

/// The helper's program, beside the app's
pub const PROGRAM: &str = if cfg!(windows) { "pts-vci32.exe" } else { "pts-vci32" };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_lines() {
        let r = Request::Ask {
            tx: 0x7E0,
            rx: 0x7E8,
            data: vec![0x22, 0xF1, 0x90],
            timeout_ms: 250,
            pins: 0,
            baud: 0,
        };
        let line = serde_json::to_string(&r).unwrap();
        assert_eq!(line, r#"{"op":"ask","tx":2016,"rx":2024,"data":[34,241,144],"timeout_ms":250}"#);
        assert_eq!(serde_json::from_str::<Request>(&line).unwrap(), r);
        let a: Reply = serde_json::from_str(r#"{"reply":"answer","data":null,"waited_ms":0}"#).unwrap();
        assert_eq!(a, Reply::Answer { data: None, waited_ms: 0 });
        assert_eq!(serde_json::to_string(&Request::Devices).unwrap(), r#"{"op":"devices"}"#);
    }
}