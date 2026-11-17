//! Ford's modules on the high-speed CAN bus (OBD pins 6 / 14), by their diagnostic address. Each answers at its
//! address + 8 (the PCM is asked at 7E0 and answers at 7E8).
//!
//! The list is what the network test asks. It's a starting list: a module that isn't on it isn't asked (a vehicle has
//! some of these, not all; others sit on the other buses, reached from pins 3 / 11 later).

/// (address, short name, name)
pub const HS_CAN: &[(u32, &str, &str)] = &[
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

/// The high-speed CAN bus's speed
pub const HS_CAN_BAUD: u32 = 500_000;
