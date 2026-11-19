//! Printing on Windows: the printers there are, text measured with Windows' own drawing (GDI) on the screen (for the
//! preview) or on the printer, and each page drawn straight onto the printer the app's Print window picked.

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Graphics::Printing::{
    EnumPrintersW, GetDefaultPrinterW, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_INFO_4W,
};
use windows_sys::Win32::Storage::Xps::{AbortDoc, DOCINFOW, EndDoc, EndPage, StartDocW, StartPage};

use super::{Doc, Measure, Op, Printers};

const BLACK: u32 = 0x0011_1111;
const GREY: u32 = 0x0055_5555; // 0x00BBGGRR
const LIGHT: u32 = 0x00bb_bbbb;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A text the system wrote (ends in 0)
unsafe fn from_wide(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe {
        let mut n = 0;
        while *p.add(n) != 0 {
            n += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
    }
}

pub fn printers() -> Printers {
    unsafe {
        let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
        let (mut needed, mut count) = (0u32, 0u32);
        EnumPrintersW(flags, null(), 4, null_mut(), 0, &mut needed, &mut count);
        // a buffer of u64s: the list holds pointers, so it must be lined up for them
        let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)];
        let ok = EnumPrintersW(flags, null(), 4, buf.as_mut_ptr().cast(), needed, &mut needed, &mut count);
        let mut names = Vec::new();
        if ok != 0 {
            let list = std::slice::from_raw_parts(buf.as_ptr() as *const PRINTER_INFO_4W, count as usize);
            names = list.iter().map(|p| from_wide(p.pPrinterName)).filter(|n| !n.is_empty()).collect();
        }
        let mut len = 0u32;
        GetDefaultPrinterW(null_mut(), &mut len);
        let default = if len > 0 {
            let mut name = vec![0u16; len as usize];
            if GetDefaultPrinterW(name.as_mut_ptr(), &mut len) != 0 {
                Some(from_wide(name.as_ptr())).filter(|n| !n.is_empty())
            } else {
                None
            }
        } else {
            None
        };
        Printers { names, default }
    }
}

/// Something GDI draws on (the screen or a printer), where its page is, and how to give it back
struct Device {
    dc: HDC,
    dpi_x: f32,
    dpi_y: f32,
    /// How far the printable area is from the paper's corner (a printer can't print right to the edge)
    off_x: f32,
    off_y: f32,
    screen: bool,
}

impl Device {
    unsafe fn new(dc: HDC, screen: bool) -> Self {
        unsafe {
            let cap = |i: u32| GetDeviceCaps(dc, i as i32) as f32;
            Device {
                dc,
                dpi_x: cap(LOGPIXELSX as u32),
                dpi_y: cap(LOGPIXELSY as u32),
                off_x: if screen { 0.0 } else { cap(PHYSICALOFFSETX as u32) },
                off_y: if screen { 0.0 } else { cap(PHYSICALOFFSETY as u32) },
                screen,
            }
        }
    }

    /// A point across → the device's dots
    fn x(&self, pt: f32) -> i32 {
        (pt * self.dpi_x / 72.0 - self.off_x).round() as i32
    }
    fn y(&self, pt: f32) -> i32 {
        (pt * self.dpi_y / 72.0 - self.off_y).round() as i32
    }

    /// Segoe UI at a size in points (made for this device); give it back with DeleteObject
    fn font(&self, size: f32, bold: bool) -> HFONT {
        let face = wide("Segoe UI");
        unsafe {
            CreateFontW(
                -((size * self.dpi_y / 72.0).round() as i32),
                0,
                0,
                0,
                if bold { 700 } else { 400 },
                0,
                0,
                0,
                DEFAULT_CHARSET as _,
                OUT_DEFAULT_PRECIS as _,
                CLIP_DEFAULT_PRECIS as _,
                DEFAULT_QUALITY as _,
                (DEFAULT_PITCH as u32 | FF_SWISS as u32) as _,
                face.as_ptr(),
            )
        }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            if self.screen {
                ReleaseDC(null_mut(), self.dc);
            } else {
                DeleteDC(self.dc);
            }
        }
    }
}

impl Measure for Device {
    fn width(&self, text: &str, size: f32, bold: bool) -> f32 {
        let w: Vec<u16> = text.encode_utf16().collect();
        if w.is_empty() {
            return 0.0;
        }
        unsafe {
            let font = self.font(size, bold);
            let old = SelectObject(self.dc, font as _);
            let mut s = SIZE { cx: 0, cy: 0 };
            GetTextExtentPoint32W(self.dc, w.as_ptr(), w.len() as i32, &mut s);
            SelectObject(self.dc, old);
            DeleteObject(font as _);
            s.cx as f32 * 72.0 / self.dpi_x
        }
    }
}

/// Laid out with text measured on the screen
pub fn preview(build: &dyn Fn(&dyn Measure) -> Doc) -> Doc {
    unsafe {
        let dc = GetDC(null_mut());
        if dc.is_null() {
            return build(&Rough);
        }
        build(&Device::new(dc, true))
    }
}

/// When the screen can't be measured: about half the size per letter
struct Rough;
impl Measure for Rough {
    fn width(&self, text: &str, size: f32, _bold: bool) -> f32 {
        text.chars().count() as f32 * size * 0.5
    }
}

pub fn print_to(printer: &str, copies: u32, build: &dyn Fn(&dyn Measure) -> Doc) -> Result<(), String> {
    unsafe {
        let driver = wide("WINSPOOL");
        let name = wide(printer);
        let dc = CreateDCW(driver.as_ptr(), name.as_ptr(), null(), null());
        if dc.is_null() {
            return Err(format!("couldn't open the printer \"{printer}\""));
        }
        let device = Device::new(dc, false);
        let doc = build(&device);
        draw(&device, &doc, copies)
    }
}

unsafe fn draw(p: &Device, doc: &Doc, copies: u32) -> Result<(), String> {
    unsafe {
        let name = wide(&doc.title);
        let info = DOCINFOW {
            cbSize: std::mem::size_of::<DOCINFOW>() as i32,
            lpszDocName: name.as_ptr(),
            lpszOutput: null(),
            lpszDatatype: null(),
            fwType: 0,
        };
        if StartDocW(p.dc, &info) <= 0 {
            return Err("the printer didn't take the document".into());
        }
        SetBkMode(p.dc, TRANSPARENT as _);
        // copies one after another, each in page order (collated)
        for _ in 0..copies {
            for page in &doc.pages {
                if StartPage(p.dc) <= 0 {
                    AbortDoc(p.dc);
                    return Err("the printer didn't start the page".into());
                }
                for op in page {
                    match op {
                        Op::Text {
                            x,
                            y,
                            size,
                            bold,
                            grey,
                            text,
                        } => {
                            let w: Vec<u16> = text.encode_utf16().collect();
                            let font = p.font(*size, *bold);
                            let old = SelectObject(p.dc, font as _);
                            SetTextColor(p.dc, if *grey { GREY } else { BLACK });
                            TextOutW(p.dc, p.x(*x), p.y(*y), w.as_ptr(), w.len() as i32);
                            SelectObject(p.dc, old);
                            DeleteObject(font as _);
                        }
                        Op::Rule { x, y, w, h, grey } => {
                            let rect = RECT {
                                left: p.x(*x),
                                top: p.y(*y),
                                right: p.x(x + w),
                                bottom: p.y(y + h).max(p.y(*y) + 1),
                            };
                            let brush = CreateSolidBrush(if *grey { LIGHT } else { BLACK });
                            FillRect(p.dc, &rect, brush);
                            DeleteObject(brush as _);
                        }
                    }
                }
                if EndPage(p.dc) <= 0 {
                    AbortDoc(p.dc);
                    return Err("the printer didn't finish the page".into());
                }
            }
        }
        if EndDoc(p.dc) <= 0 {
            return Err("the printer didn't finish the document".into());
        }
        Ok(())
    }
}
