//! The app's window on the Windows taskbar: which program pinning it pins.
//!
//! Left alone, Windows pins the program that made the window, which is the app (`pts-workbench-app.exe`, possibly a
//! downloaded update's copy in the app's folder), not the launcher people should start. So the window carries
//! the app's ID and "relaunch" details (Windows' own properties for this): pinning it pins the launcher, with its name
//! and icon. Set on the window through its property store (`SHGetPropertyStoreForWindow`), called here directly (a
//! few lines, so no more of the `windows` crate is needed).

use std::ffi::c_void;

use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[repr(C)]
#[derive(Clone, Copy)]
struct Guid {
    d1: u32,
    d2: u16,
    d3: u16,
    d4: [u8; 8],
}

#[repr(C)]
struct PropertyKey {
    fmtid: Guid,
    pid: u32,
}

/// A PROPVARIANT holding a string (VT_LPWSTR): the same size as Windows' own (16 bytes on 32-bit, 24 on 64-bit)
#[repr(C)]
struct PropVariant {
    vt: u16,
    reserved: [u16; 3],
    value: *const u16,
    pad: usize,
}

/// IPropertyStore's methods, in order (only the ones used are typed)
#[repr(C)]
struct StoreMethods {
    query_interface: usize,
    add_ref: usize,
    release: unsafe extern "system" fn(*mut Store) -> u32,
    get_count: usize,
    get_at: usize,
    get_value: usize,
    set_value: unsafe extern "system" fn(*mut Store, *const PropertyKey, *const PropVariant) -> i32,
    commit: unsafe extern "system" fn(*mut Store) -> i32,
}

#[repr(C)]
struct Store {
    methods: *const StoreMethods,
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn SHGetPropertyStoreForWindow(hwnd: isize, riid: *const Guid, ppv: *mut *mut c_void) -> i32;
}

#[link(name = "ole32")]
unsafe extern "system" {
    fn CoInitializeEx(reserved: *const c_void, coinit: u32) -> i32;
}

/// IID_IPropertyStore {886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99}
const IID_PROPERTY_STORE: Guid = Guid {
    d1: 0x886d_8eeb,
    d2: 0x8cf2,
    d3: 0x4446,
    d4: [0x8d, 0x02, 0xcd, 0xba, 0x1d, 0xbd, 0xcf, 0x99],
};

/// The System.AppUserModel properties {9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3}
const APP_USER_MODEL: Guid = Guid {
    d1: 0x9f4c_2855,
    d2: 0x9f79,
    d3: 0x4b39,
    d4: [0xa8, 0xd0, 0xe1, 0xd4, 0x2d, 0xe1, 0xd5, 0xf3],
};
const RELAUNCH_COMMAND: u32 = 2;
const RELAUNCH_ICON: u32 = 3;
const RELAUNCH_NAME: u32 = 4;
const ID: u32 = 5;

const VT_LPWSTR: u16 = 31;
const COINIT_APARTMENTTHREADED: u32 = 0x2;

/// What the window says about itself (see [`crate::Taskbar`])
pub fn set(parent: &impl HasWindowHandle, t: &crate::Taskbar) -> Result<(), String> {
    let hwnd = match parent.window_handle().map_err(|e| e.to_string())?.as_raw() {
        RawWindowHandle::Win32(h) => h.hwnd.get(),
        _ => return Err("not a Windows window".into()),
    };
    // (the relaunch details first: Windows only looks at them once the window has its ID)
    let props = [
        (RELAUNCH_COMMAND, t.command.as_str()),
        (RELAUNCH_ICON, t.icon.as_str()),
        (RELAUNCH_NAME, t.name.as_str()),
        (ID, t.app_id.as_str()),
    ];
    // SAFETY: Windows' documented calls, with a property store it hands back (released here) and strings that outlive
    // each SetValue (the store keeps its own copy)
    unsafe {
        // (COM is usually set up on this thread already; then this changes nothing)
        CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED);
        let mut store: *mut c_void = std::ptr::null_mut();
        let hr = SHGetPropertyStoreForWindow(hwnd, &IID_PROPERTY_STORE, &mut store);
        if hr < 0 || store.is_null() {
            return Err(format!("no property store for the window ({hr:#x})"));
        }
        let store = store.cast::<Store>();
        let methods = &*(*store).methods;
        let mut failed = None;
        for (pid, value) in props {
            let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
            let v = PropVariant {
                vt: VT_LPWSTR,
                reserved: [0; 3],
                value: wide.as_ptr(),
                pad: 0,
            };
            let key = PropertyKey {
                fmtid: APP_USER_MODEL,
                pid,
            };
            let hr = (methods.set_value)(store, &key, &v);
            if hr < 0 && failed.is_none() {
                failed = Some(format!("couldn't set property {pid} ({hr:#x})"));
            }
        }
        (methods.commit)(store);
        (methods.release)(store);
        failed.map_or(Ok(()), Err)
    }
}
