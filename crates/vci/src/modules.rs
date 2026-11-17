//! Ford's modules by their diagnostic address. Each answers at its address + 8 (the PCM is asked at 7E0 and answers
//! at 7E8).
//!
//! Where they're reached from the OBD port (the DLC):
//!   * Newer Fords have a gateway (the GWM, 716): the DLC has one diagnostic network (pins 6 / 14), and the GWM
//!     passes requests on to the modules on the vehicle's other networks. Every module is asked there.
//!   * Older Fords (no gateway) bring their medium-speed CAN out to pins 3 / 11 too (125 kbit/s): the body and
//!     comfort modules are there.
//!
//! [`MODULES`] is the list the network test asks (by name). The full search ([`sweep`]) asks every other diagnostic
//! address too, for modules that aren't on it.

/// A network at the OBD port
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Bus {
    /// High-speed CAN, pins 6 / 14, 500 kbit/s (on gateway vehicles: the diagnostic network to the GWM)
    #[default]
    Hs,
    /// Medium-speed CAN, pins 3 / 11, 125 kbit/s (older Fords, without a gateway)
    Ms,
}

impl Bus {
    pub fn label(self) -> &'static str {
        match self {
            Bus::Hs => "HS-CAN",
            Bus::Ms => "MS-CAN",
        }
    }

    pub fn baud(self) -> u32 {
        match self {
            Bus::Hs => HS_CAN_BAUD,
            Bus::Ms => MS_CAN_BAUD,
        }
    }
}

/// (address, short name, name)
pub type Entry = (u32, &'static str, &'static str);

/// The modules asked by name (on gateway vehicles, the GWM passes these on to whichever network they're on)
pub const MODULES: &[Entry] = HS_CAN;

/// (the list's first name, kept for what used it)
pub const HS_CAN: &[Entry] = &[
    (0x7E0, "PCM", "Powertrain Control Module"),
    (0x7E1, "TCM", "Transmission Control Module"),
    (0x7E4, "BECM", "Battery Energy Control Module"),
    (0x746, "DCDC", "DC/DC Converter Control Module"),
    (0x760, "ABS", "Anti-Lock Brake System Module"),
    (0x730, "PSCM", "Power Steering Control Module"),
    (0x737, "RCM", "Restraints Control Module"),
    (0x765, "OCSM", "Occupant Classification System Module"),
    (0x716, "GWM", "Gateway Module"),
    (0x726, "BCM", "Body Control Module"),
    (0x720, "IPC", "Instrument Panel Cluster"),
    (0x724, "SCCM", "Steering Column Control Module"),
    (0x7D0, "APIM", "SYNC (Accessory Protocol Interface Module)"),
    (0x727, "ACM", "Audio Control Module"),
    (0x7A7, "FCIM", "Front Controls Interface Module"),
    (0x733, "HVAC", "Heating, Ventilation and Air Conditioning Module"),
    (0x736, "PAM", "Parking Aid Module"),
    (0x764, "CCM", "Cruise Control Module"),
    (0x706, "IPMA", "Image Processing Module A"),
    (0x7C4, "SODL", "Side Obstacle Detection, Left"),
    (0x7C6, "SODR", "Side Obstacle Detection, Right"),
    (0x734, "HCM", "Headlamp Control Module"),
    (0x740, "DDM", "Driver Door Module"),
    (0x741, "PDM", "Passenger Door Module"),
    (0x744, "DSM", "Driver Seat Module"),
    (0x731, "RFA", "Remote Function Actuator"),
    (0x703, "AWD", "All-Wheel Drive Module"),
    (0x754, "TCU", "Telematics Control Unit"),
];

/// Where a module answers
pub fn answers_at(address: u32) -> u32 {
    address + 8
}

/// The PCM: asked for the VIN
pub const PCM: u32 = 0x7E0;

/// The gateway: when it answers, every module is reached through it (nothing on pins 3 / 11)
pub const GWM: u32 = 0x716;

/// The high-speed CAN bus's speed
pub const HS_CAN_BAUD: u32 = 500_000;
/// The medium-speed CAN bus's speed
pub const MS_CAN_BAUD: u32 = 125_000;

/// A module found by the full search that isn't on [`MODULES`]
pub const UNLISTED: (&str, &str) = ("Module", "Not on the app's list (its part number says what it is)");

/// The full search's addresses: every diagnostic request address (700–7F7) that isn't on [`MODULES`], isn't where
/// one of them answers, and isn't the OBD request to all (7DF) or the OBD answers (7E8–7EF)
pub fn sweep() -> Vec<u32> {
    let listed: Vec<u32> = MODULES.iter().map(|m| m.0).collect();
    (0x700..=0x7F7)
        .filter(|a| !listed.contains(a))
        .filter(|a| !listed.iter().any(|l| answers_at(*l) == *a))
        .filter(|a| *a != 0x7DF && !(0x7E8..=0x7EF).contains(a))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_full_search_skips_answers_and_the_list() {
        let s = sweep();
        assert!(!s.contains(&0x7E0) && !s.contains(&0x7E8) && !s.contains(&0x728) && !s.contains(&0x7DF));
        assert!(s.contains(&0x7E2) && s.contains(&0x700) && s.contains(&0x7F7));
        assert!(s.len() > 150 && s.len() < 230, "{}", s.len());
    }
}