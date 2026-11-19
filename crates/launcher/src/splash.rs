//! The loading window: a small window that shows the moment the launcher starts ("PTS Workbench · Loading the
//! workbench…") and closes when the app's own window is up. So starting the app always shows something right away.
//!
//! Windows only, drawn with the plain Windows API (no UI library, so the launcher stays small and starts instantly).
//! Elsewhere it does nothing.

use std::sync::{Arc, Mutex};

/// The loading window; `close()` from any thread (safe to call more than once, or before it's even shown)
#[derive(Clone, Default)]
pub struct Splash {
    inner: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    /// The window, once made (as a number: window handles can't cross threads as pointers)
    #[cfg_attr(not(windows), allow(dead_code))]
    hwnd: isize,
    closed: bool,
}

impl Splash {
    /// Shows it, on a thread of its own (its window needs that thread's message loop)
    pub fn show(title: &str, line: &str) -> Self {
        let splash = Splash::default();
        #[cfg(windows)]
        {
            let s = splash.clone();
            let (title, line) = (title.to_string(), line.to_string());
            std::thread::spawn(move || win::run(&s, &title, &line));
        }
        #[cfg(not(windows))]
        let _ = (title, line);
        splash
    }

    pub fn close(&self) {
        let mut st = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        st.closed = true;
        #[cfg(windows)]
        if st.hwnd != 0 {
            win::close(st.hwnd);
            st.hwnd = 0;
        }
    }
}

#[cfg(windows)]
mod win {
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DwmSetWindowAttribute};
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem, SetProcessDpiAwarenessContext};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    use super::Splash;

    /// The app's colors (light): panel, text, grey text, accent
    const PANEL: u32 = rgb(0xff, 0xff, 0xff);
    const TEXT: u32 = rgb(0x16, 0x19, 0x1d);
    const MUTED: u32 = rgb(0x5d, 0x66, 0x70);
    const ACCENT: u32 = rgb(0x1a, 0x5f, 0xb4);

    /// Windows colors are 0x00BBGGRR
    const fn rgb(r: u8, g: u8, b: u8) -> u32 {
        r as u32 | (g as u32) << 8 | (b as u32) << 16
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    // what the window shows (set before it's made; one loading window per launcher at a time)
    static TEXTS: std::sync::Mutex<(Vec<u16>, Vec<u16>)> = std::sync::Mutex::new((Vec::new(), Vec::new()));
    static SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(96);

    fn px(n: i32) -> i32 {
        n * SCALE.load(std::sync::atomic::Ordering::Relaxed) as i32 / 96
    }

    pub fn close(hwnd: isize) {
        unsafe {
            PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0);
        }
    }

    pub fn run(splash: &Splash, title: &str, line: &str) {
        *TEXTS.lock().unwrap_or_else(|e| e.into_inner()) = (wide(title), wide(line));
        unsafe {
            // sharp text on high-DPI screens
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            SCALE.store(GetDpiForSystem().max(96), std::sync::atomic::Ordering::Relaxed);

            let instance = GetModuleHandleW(null());
            let class = wide("PtsWorkbenchLoading");
            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(proc),
                hInstance: instance,
                hCursor: LoadCursorW(null_mut(), IDC_APPSTARTING), // the "starting" pointer
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            RegisterClassW(&wc);

            // small, in the middle of the screen, on top, no title bar, not in the taskbar
            let (w, h) = (px(360), px(120));
            let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
            let title_w = wide("PTS Workbench");
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                class.as_ptr(),
                title_w.as_ptr(),
                WS_POPUP,
                (sw - w) / 2,
                (sh - h) / 2,
                w,
                h,
                null_mut(),
                null_mut(),
                instance,
                null(),
            );
            if hwnd.is_null() {
                return;
            }
            // rounded corners on Windows 11 (ignored on 10)
            let round: i32 = 2; // DWMWCP_ROUND
            DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE as _, &round as *const i32 as *const _, 4);

            {
                let mut st = splash.inner.lock().unwrap_or_else(|e| e.into_inner());
                if st.closed {
                    // the app was quicker
                    DestroyWindow(hwnd);
                    return;
                }
                st.hwnd = hwnd as isize;
            }
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            UpdateWindow(hwnd);

            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe {
            match msg {
                WM_PAINT => {
                    let mut ps: PAINTSTRUCT = std::mem::zeroed();
                    let dc = BeginPaint(hwnd, &mut ps);
                    let mut rc: RECT = std::mem::zeroed();
                    GetClientRect(hwnd, &mut rc);
                    // the card, with a thin accent line along the top
                    let panel = CreateSolidBrush(PANEL);
                    FillRect(dc, &rc, panel);
                    DeleteObject(panel as _);
                    let accent = CreateSolidBrush(ACCENT);
                    let bar = RECT {
                        left: rc.left,
                        top: rc.top,
                        right: rc.right,
                        bottom: rc.top + px(4),
                    };
                    FillRect(dc, &bar, accent);
                    DeleteObject(accent as _);

                    SetBkMode(dc, TRANSPARENT as _);
                    let face = wide("Segoe UI");
                    let font = |size: i32, weight: u32| {
                        CreateFontW(
                            -px(size),
                            0,
                            0,
                            0,
                            weight as _,
                            0,
                            0,
                            0,
                            DEFAULT_CHARSET as _,
                            OUT_DEFAULT_PRECIS as _,
                            CLIP_DEFAULT_PRECIS as _,
                            CLEARTYPE_QUALITY as _,
                            (DEFAULT_PITCH as u32 | FF_SWISS as u32) as _,
                            face.as_ptr(),
                        )
                    };
                    let texts = TEXTS.lock().unwrap_or_else(|e| e.into_inner());
                    let mid = (rc.top + rc.bottom) / 2;

                    let big = font(20, FW_SEMIBOLD as u32);
                    let old = SelectObject(dc, big as _);
                    SetTextColor(dc, TEXT);
                    let mut top = RECT {
                        left: rc.left,
                        top: rc.top + px(4),
                        right: rc.right,
                        bottom: mid + px(2),
                    };
                    DrawTextW(dc, texts.0.as_ptr(), -1, &mut top, DT_CENTER | DT_BOTTOM | DT_SINGLELINE);

                    let small = font(13, FW_NORMAL as u32);
                    SelectObject(dc, small as _);
                    SetTextColor(dc, MUTED);
                    let mut bottom = RECT {
                        left: rc.left,
                        top: mid + px(8),
                        right: rc.right,
                        bottom: rc.bottom,
                    };
                    DrawTextW(dc, texts.1.as_ptr(), -1, &mut bottom, DT_CENTER | DT_TOP | DT_SINGLELINE);

                    SelectObject(dc, old);
                    DeleteObject(big as _);
                    DeleteObject(small as _);
                    EndPaint(hwnd, &ps);
                    0
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    0
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
    }
}
