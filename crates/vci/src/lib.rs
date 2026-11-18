//! Talking to vehicles through a J2534 VCI: Ford's VCM 3 and VCMM (the same way FDRS does, through the J2534 driver
//! Ford's VCI Manager installs).
//!
//! Ford's J2534 drivers are 32-bit only (`BVTX4J32.dll`), and the app is 64-bit: a 64-bit program can't load a 32-bit
//! driver. So a small 32-bit helper program, `pts-vci32.exe` (crates/vci-bridge), beside the app, loads the driver and
//! makes the J2534 calls; the app starts it in the background and they talk in lines of JSON ([`proto`]).
//!
//!   proto.rs    what the app asks the helper, and its answers
//!   modules.rs  Ford's modules on the high-speed CAN bus (OBD pins 6 / 14), by their diagnostic address
//!   uds.rs      the diagnostic requests (UDS: ISO 14229) and reading their answers: part numbers, the VIN, refusals
//!   scan.rs     the network test: each module asked who it is, one after another
//!   dtc.rs      the self test: each module's trouble codes, and clearing them
//!   obd.rs      live data: OBD-II's readings (engine speed, temperatures, fuel trims…) in US units
//!   sim.rs      the demo vehicle (a made-up F-150, for trying Diagnostics without a truck)
//!   client.rs   (the app's side) starting the helper and asking it things
//!
//! Only reads, and clearing trouble codes (when asked): nothing is written or programmed.

pub mod dtc;
pub mod modules;
pub mod obd;
pub mod proto;
pub mod scan;
pub mod sim;
pub mod uds;

#[cfg(feature = "client")]
pub mod client;
