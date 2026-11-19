//! The J2534 interfaces Windows has: HKLM\SOFTWARE\PassThruSupport.04.04, one key each (32-bit drivers are listed in
//! the registry's 32-bit view, which is what this asks for). Ford's: "Bosch - VCM3", "Bosch - VCMM", "Bosch -
//! Ford-VCM-II", each with its driver (FunctionLibrary) and which protocols it does.

use std::ffi::c_void;

use vci::proto::Device;

type Hkey = isize;

const HKEY_LOCAL_MACHINE: Hkey = (0x8000_0002u32 as i32) as isize;
const KEY_READ: u32 = 0x2_0019;
/// The 32-bit view (where 32-bit drivers are listed, whichever this is built as)
const KEY_WOW64_32KEY: u32 = 0x0200;
const RRF_RT_REG_SZ: u32 = 0x0000_0002;
const RRF_RT_DWORD: u32 = 0x0000_0018;
const ERROR_SUCCESS: i32 = 0;

#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegOpenKeyExW(key: Hkey, sub_key: *const u16, options: u32, sam: u32, result: *mut Hkey) -> i32;
    fn RegEnumKeyExW(
        key: Hkey,
        index: u32,
        name: *mut u16,
        name_len: *mut u32,
        reserved: *const u32,
        class: *mut u16,
        class_len: *mut u32,
        last_write: *mut c_void,
    ) -> i32;
    fn RegGetValueW(
        key: Hkey,
        sub_key: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut c_void,
        data_len: *mut u32,
    ) -> i32;
    fn RegCloseKey(key: Hkey) -> i32;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn text(key: Hkey, sub: &[u16], name: &str) -> String {
    let name = wide(name);
    let mut buf = vec![0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: the buffer and its length in bytes are passed together; the key is open
    let r = unsafe {
        RegGetValueW(
            key,
            sub.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut len,
        )
    };
    if r != ERROR_SUCCESS {
        return String::new();
    }
    let n = (len as usize / 2).min(buf.len());
    String::from_utf16_lossy(&buf[..n]).trim_end_matches('\0').to_string()
}

fn number(key: Hkey, sub: &[u16], name: &str) -> u32 {
    let name = wide(name);
    let mut v: u32 = 0;
    let mut len = 4u32;
    // SAFETY: a DWORD read into a u32
    let r = unsafe {
        RegGetValueW(
            key,
            sub.as_ptr(),
            name.as_ptr(),
            RRF_RT_DWORD,
            std::ptr::null_mut(),
            (&mut v as *mut u32).cast(),
            &mut len,
        )
    };
    if r == ERROR_SUCCESS { v } else { 0 }
}

/// The interfaces listed, Ford's VCM 3 first, then the VCMM
pub fn devices() -> Vec<Device> {
    let root = wide("SOFTWARE\\PassThruSupport.04.04");
    let mut key: Hkey = 0;
    // SAFETY: opening a key for reading; closed below
    if unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, root.as_ptr(), 0, KEY_READ | KEY_WOW64_32KEY, &mut key) } != ERROR_SUCCESS {
        return Vec::new();
    }
    let mut out = Vec::new();
    for index in 0.. {
        let mut name = vec![0u16; 256];
        let mut len = name.len() as u32;
        // SAFETY: the name buffer and its length in characters are passed together
        let r = unsafe {
            RegEnumKeyExW(
                key,
                index,
                name.as_mut_ptr(),
                &mut len,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if r != ERROR_SUCCESS {
            break; // (no more)
        }
        let sub: Vec<u16> = name[..len as usize].iter().copied().chain(std::iter::once(0)).collect();
        let dll = text(key, &sub, "FunctionLibrary");
        if dll.is_empty() {
            continue;
        }
        let shown = text(key, &sub, "Name");
        out.push(Device {
            name: if shown.is_empty() {
                String::from_utf16_lossy(&name[..len as usize])
            } else {
                shown
            },
            vendor: text(key, &sub, "Vendor"),
            dll,
            iso15765: number(key, &sub, "ISO15765") != 0,
            iso15765_ps: number(key, &sub, "ISO15765_PS") != 0,
        });
    }
    // SAFETY: opened above
    unsafe { RegCloseKey(key) };
    let rank = |d: &Device| match d.name.to_ascii_uppercase().as_str() {
        "VCM3" => 0,
        "VCMM" => 1,
        _ => 2,
    };
    out.sort_by_key(rank);
    out
}
