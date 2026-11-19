//! SAE J2534 (04.04) through the interface's own driver: open it, its voltage, join a CAN bus for diagnostics
//! (ISO 15765, 11-bit addresses): the high-speed one on pins 6 / 14, or an older Ford's other networks on their own
//! pins (J2534-2's pin-switched ISO15765_PS, its pins set with J1962_PINS); one diagnostic request and its answer,
//! close.
//!
//! A request to a module: a flow-control filter for its address pair (so the VCI handles the multi-frame transfer),
//! the request written, its answer read (the "still working on it" answers waited through), the filter taken away.

use std::ffi::{c_char, c_void};
use std::time::{Duration, Instant};

// ---------- the driver ----------

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}

/// A J2534 message (PASSTHRU_MSG)
#[repr(C)]
struct Msg {
    protocol_id: u32,
    rx_status: u32,
    tx_flags: u32,
    timestamp: u32,
    data_size: u32,
    extra_data_index: u32,
    data: [u8; 4128],
}

impl Msg {
    fn new(protocol: u32, flags: u32, data: &[u8]) -> Box<Msg> {
        let mut m = Box::new(Msg {
            protocol_id: protocol,
            rx_status: 0,
            tx_flags: flags,
            timestamp: 0,
            data_size: data.len() as u32,
            extra_data_index: 0,
            data: [0; 4128],
        });
        m.data[..data.len()].copy_from_slice(data);
        m
    }

    fn bytes(&self) -> &[u8] {
        &self.data[..(self.data_size as usize).min(self.data.len())]
    }
}

#[repr(C)]
struct SConfig {
    parameter: u32,
    value: u32,
}

#[repr(C)]
struct SConfigList {
    count: u32,
    list: *mut SConfig,
}

type Open = unsafe extern "system" fn(*const c_void, *mut u32) -> i32;
type Close = unsafe extern "system" fn(u32) -> i32;
type Connect = unsafe extern "system" fn(u32, u32, u32, u32, *mut u32) -> i32;
type Disconnect = unsafe extern "system" fn(u32) -> i32;
type ReadMsgs = unsafe extern "system" fn(u32, *mut Msg, *mut u32, u32) -> i32;
type WriteMsgs = unsafe extern "system" fn(u32, *const Msg, *mut u32, u32) -> i32;
type StartFilter = unsafe extern "system" fn(u32, u32, *const Msg, *const Msg, *const Msg, *mut u32) -> i32;
type StopFilter = unsafe extern "system" fn(u32, u32) -> i32;
type Ioctl = unsafe extern "system" fn(u32, u32, *mut c_void, *mut c_void) -> i32;
type ReadVersion = unsafe extern "system" fn(u32, *mut c_char, *mut c_char, *mut c_char) -> i32;
type GetLastError = unsafe extern "system" fn(*mut c_char) -> i32;

// J2534 04.04
const ISO15765: u32 = 6;
/// J2534-2: ISO 15765 on the pins asked for (J1962_PINS)
const ISO15765_PS: u32 = 0x8005;
/// J2534-2: the first extra ISO 15765 channel (its pins set with J1962_PINS too)
const ISO15765_CH1: u32 = 0x9400;
const J1962_PINS: u32 = 0x8001;
const HS_CAN_BAUD: u32 = 500_000;
const FLOW_CONTROL_FILTER: u32 = 3;
const ISO15765_FRAME_PAD: u32 = 0x40;
const READ_VBATT: u32 = 0x03;
const SET_CONFIG: u32 = 0x02;
const CLEAR_RX_BUFFER: u32 = 0x08;
const ISO15765_BS: u32 = 0x1E;
const ISO15765_STMIN: u32 = 0x1F;
/// RxStatus: a copy of what was sent, the start of a multi-frame answer, a "sent" note: not answers
const NOT_AN_ANSWER: u32 = 0x01 | 0x02 | 0x08;
const ERR_TIMEOUT: i32 = 0x09;
const ERR_BUFFER_EMPTY: i32 = 0x10;

fn status_name(code: i32) -> &'static str {
    match code {
        0x01 => "not supported",
        0x02 => "invalid channel",
        0x03 => "invalid protocol",
        0x07 => "failed",
        0x08 => "the VCI isn't connected",
        0x09 => "timed out",
        0x0E => "the VCI is in use by another program",
        0x10 => "nothing to read",
        0x14 => "the channel is in use",
        0x17 => "no flow control filter",
        0x19 => "invalid baud rate",
        0x1A => "invalid device",
        _ => "error",
    }
}

fn c_text(buf: &[c_char]) -> String {
    let bytes: Vec<u8> = buf.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).trim().to_string()
}

/// A channel on a network: its protocol, pins (0: 6 / 14) and speed
#[derive(Debug, Clone, Copy)]
struct On {
    channel: u32,
    protocol: u32,
    pins: u32,
    baud: u32,
}

/// The interface, open; its channel on the CAN bus while joined. Closed (and the driver let go of) when dropped.
pub struct Interface {
    module: *mut c_void,
    device: u32,
    /// The channel, while on a network
    channel: Option<On>,
    pub versions: (String, String, String),
    f: Calls,
}

struct Calls {
    close: Close,
    connect: Connect,
    disconnect: Disconnect,
    read: ReadMsgs,
    write: WriteMsgs,
    start_filter: StartFilter,
    stop_filter: StopFilter,
    ioctl: Ioctl,
    last_error: GetLastError,
}

impl Interface {
    /// Loads the driver and opens its interface (the VCI connected as VCI Manager set it up)
    pub fn open(dll: &str) -> Result<Interface, String> {
        let wide: Vec<u16> = dll.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: loading the vendor's J2534 driver, as every J2534 program does
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        if module.is_null() {
            return Err(format!("Couldn't load the VCI's driver ({dll})"));
        }
        macro_rules! get {
            ($name:literal, $t:ty) => {{
                // SAFETY: the J2534 04.04 functions, with their documented signatures
                let p = unsafe { GetProcAddress(module, concat!($name, "\0").as_ptr()) };
                if p.is_null() {
                    unsafe { FreeLibrary(module) };
                    return Err(format!("The VCI's driver has no {}", $name));
                }
                unsafe { std::mem::transmute::<*mut c_void, $t>(p) }
            }};
        }
        let open: Open = get!("PassThruOpen", Open);
        let read_version: ReadVersion = get!("PassThruReadVersion", ReadVersion);
        let f = Calls {
            close: get!("PassThruClose", Close),
            connect: get!("PassThruConnect", Connect),
            disconnect: get!("PassThruDisconnect", Disconnect),
            read: get!("PassThruReadMsgs", ReadMsgs),
            write: get!("PassThruWriteMsgs", WriteMsgs),
            start_filter: get!("PassThruStartMsgFilter", StartFilter),
            stop_filter: get!("PassThruStopMsgFilter", StopFilter),
            ioctl: get!("PassThruIoctl", Ioctl),
            last_error: get!("PassThruGetLastError", GetLastError),
        };
        let mut device = 0u32;
        // SAFETY: J2534: a null name opens the driver's own interface
        let r = unsafe { open(std::ptr::null(), &mut device) };
        if r != 0 {
            let mut buf = [0 as c_char; 80];
            unsafe { (f.last_error)(buf.as_mut_ptr()) };
            let said = c_text(&buf);
            unsafe { FreeLibrary(module) };
            return Err(format!(
                "Couldn't open the VCI: {}{}",
                status_name(r),
                if said.is_empty() { String::new() } else { format!(" ({said})") }
            ));
        }
        let (mut fw, mut dl, mut api) = ([0 as c_char; 80], [0 as c_char; 80], [0 as c_char; 80]);
        // SAFETY: three 80-character buffers, as J2534 has them
        unsafe { read_version(device, fw.as_mut_ptr(), dl.as_mut_ptr(), api.as_mut_ptr()) };
        Ok(Interface {
            module,
            device,
            channel: None,
            versions: (c_text(&fw), c_text(&dl), c_text(&api)),
            f,
        })
    }

    fn fail(&self, what: &str, code: i32) -> String {
        let mut buf = [0 as c_char; 80];
        // SAFETY: an 80-character buffer
        unsafe { (self.f.last_error)(buf.as_mut_ptr()) };
        let said = c_text(&buf);
        format!(
            "{what}: {}{}",
            status_name(code),
            if said.is_empty() { String::new() } else { format!(" ({said})") }
        )
    }

    /// The battery voltage at the OBD port (millivolts)
    pub fn voltage(&mut self) -> Result<u32, String> {
        let mut mv = 0u32;
        // SAFETY: READ_VBATT writes one u32
        let r = unsafe { (self.f.ioctl)(self.device, READ_VBATT, std::ptr::null_mut(), (&mut mv as *mut u32).cast()) };
        if r != 0 {
            return Err(self.fail("Couldn't read the voltage", r));
        }
        Ok(mv)
    }

    /// Opens a channel with its pins set (J2534-2), or why not
    fn open_on_pins(&mut self, protocol: u32, baud: u32, pins: u32) -> Result<u32, i32> {
        let mut channel = 0u32;
        // SAFETY: J2534 Connect (no flags)
        let r = unsafe { (self.f.connect)(self.device, protocol, 0, baud, &mut channel) };
        if r != 0 {
            return Err(r);
        }
        // its pins, before anything else on the channel
        let mut list_items = [SConfig {
            parameter: J1962_PINS,
            value: pins,
        }];
        let mut list = SConfigList {
            count: 1,
            list: list_items.as_mut_ptr(),
        };
        // SAFETY: SET_CONFIG with one parameter
        let r = unsafe { (self.f.ioctl)(channel, SET_CONFIG, (&mut list as *mut SConfigList).cast(), std::ptr::null_mut()) };
        if r != 0 {
            // SAFETY: connected above
            unsafe { (self.f.disconnect)(channel) };
            return Err(r);
        }
        Ok(channel)
    }

    /// Joins a CAN network for diagnostics: pins 6 / 14 (`pins` 0), or other pins (J1962_PINS: 0x030B is 3 / 11).
    /// Other pins are J2534-2's pin-switched ISO 15765; a VCI that refuses that is asked for its first extra CAN
    /// channel instead.
    pub fn connect(&mut self, baud: u32, pins: u32) -> Result<(), String> {
        self.disconnect();
        let (channel, protocol) = if pins == 0 {
            let mut channel = 0u32;
            // SAFETY: J2534 Connect: ISO 15765, 11-bit addresses (no flags)
            let r = unsafe { (self.f.connect)(self.device, ISO15765, 0, baud, &mut channel) };
            if r != 0 {
                return Err(self.fail("Couldn't join the CAN bus", r));
            }
            (channel, ISO15765)
        } else {
            let what = format!("Couldn't reach pins {} / {}", pins >> 8, pins & 0xFF);
            match self.open_on_pins(ISO15765_PS, baud, pins) {
                Ok(c) => (c, ISO15765_PS),
                Err(first) => match self.open_on_pins(ISO15765_CH1, baud, pins) {
                    Ok(c) => (c, ISO15765_CH1),
                    Err(_) => return Err(self.fail(&what, first)),
                },
            }
        };
        self.channel = Some(On {
            channel,
            protocol,
            pins,
            baud,
        });
        // block size and separation time 0: modules send their multi-frame answers as fast as they like
        let mut params = [
            SConfig {
                parameter: ISO15765_BS,
                value: 0,
            },
            SConfig {
                parameter: ISO15765_STMIN,
                value: 0,
            },
        ];
        let mut list = SConfigList {
            count: params.len() as u32,
            list: params.as_mut_ptr(),
        };
        // SAFETY: SET_CONFIG with a list of parameters (not fatal if the VCI keeps its own)
        unsafe { (self.f.ioctl)(channel, SET_CONFIG, (&mut list as *mut SConfigList).cast(), std::ptr::null_mut()) };
        Ok(())
    }

    pub fn disconnect(&mut self) {
        if let Some(On { channel: c, .. }) = self.channel.take() {
            // SAFETY: the channel was connected
            unsafe { (self.f.disconnect)(c) };
        }
    }

    /// One diagnostic request to a module, on the network at `pins` and `baud` (0, 0: pins 6 / 14 at 500 kbit/s;
    /// moved to first when it's on another), and its answer (none: none in time); how long it asked to wait
    pub fn ask(&mut self, tx: u32, rx: u32, data: &[u8], timeout_ms: u32, pins: u32, baud: u32) -> Result<(Option<Vec<u8>>, u32), String> {
        let baud = if baud == 0 { HS_CAN_BAUD } else { baud };
        let on = self.channel.ok_or("Not on the CAN bus")?;
        if on.pins != pins || on.baud != baud {
            self.connect(baud, pins)?;
        }
        let On { channel, protocol, .. } = self.channel.ok_or("Not on the CAN bus")?;
        let id = |a: u32| [(a >> 24) as u8, (a >> 16) as u8, (a >> 8) as u8, a as u8];
        // the flow-control filter: answers from `rx`, flow control sent to `tx`
        let mask = Msg::new(protocol, ISO15765_FRAME_PAD, &[0x00, 0x00, 0x07, 0xFF]);
        let pattern = Msg::new(protocol, ISO15765_FRAME_PAD, &id(rx));
        let flow = Msg::new(protocol, ISO15765_FRAME_PAD, &id(tx));
        let mut filter = 0u32;
        // SAFETY: three messages and the filter's ID out
        let r = unsafe { (self.f.start_filter)(channel, FLOW_CONTROL_FILTER, &*mask, &*pattern, &*flow, &mut filter) };
        if r != 0 {
            return Err(self.fail("Couldn't set up the request", r));
        }
        let result = self.ask_filtered(channel, protocol, tx, rx, data, timeout_ms);
        // SAFETY: the filter started above
        unsafe { (self.f.stop_filter)(channel, filter) };
        result
    }

    fn ask_filtered(
        &mut self,
        channel: u32,
        protocol: u32,
        tx: u32,
        rx: u32,
        data: &[u8],
        timeout_ms: u32,
    ) -> Result<(Option<Vec<u8>>, u32), String> {
        // SAFETY: CLEAR_RX_BUFFER takes nothing
        unsafe { (self.f.ioctl)(channel, CLEAR_RX_BUFFER, std::ptr::null_mut(), std::ptr::null_mut()) };
        let mut bytes = vec![(tx >> 24) as u8, (tx >> 16) as u8, (tx >> 8) as u8, tx as u8];
        bytes.extend_from_slice(data);
        let msg = Msg::new(protocol, ISO15765_FRAME_PAD, &bytes);
        let mut count = 1u32;
        // SAFETY: one message
        let r = unsafe { (self.f.write)(channel, &*msg, &mut count, 1000) };
        if r != 0 {
            return Err(self.fail("Couldn't send the request", r));
        }
        let start = Instant::now();
        let mut deadline = start + Duration::from_millis(u64::from(timeout_ms));
        let mut waited = 0u32;
        let mut got = Msg::new(protocol, 0, &[]);
        while Instant::now() < deadline {
            let mut count = 1u32;
            // SAFETY: room for one message
            let r = unsafe { (self.f.read)(channel, &mut *got, &mut count, 50) };
            if r == ERR_BUFFER_EMPTY || r == ERR_TIMEOUT || count == 0 {
                continue;
            }
            if r != 0 {
                return Err(self.fail("Couldn't read the answer", r));
            }
            let b = got.bytes();
            if got.rx_status & NOT_AN_ANSWER != 0 || b.len() <= 4 {
                continue;
            }
            let from = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
            if from != rx {
                continue;
            }
            let answer = b[4..].to_vec();
            // "still working on it": its answer's coming (up to 5 seconds more each time)
            if answer.len() >= 3 && answer[0] == 0x7F && answer[2] == 0x78 {
                deadline = Instant::now() + Duration::from_secs(5);
                waited = start.elapsed().as_millis() as u32;
                continue;
            }
            return Ok((Some(answer), waited));
        }
        Ok((None, waited))
    }
}

impl Drop for Interface {
    fn drop(&mut self) {
        self.disconnect();
        // SAFETY: opened in `open`; the driver let go of last
        unsafe {
            (self.f.close)(self.device);
            FreeLibrary(self.module);
        }
    }
}