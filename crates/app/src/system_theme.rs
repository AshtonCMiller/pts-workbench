//! Whether the system is set to dark, for the Auto appearance.
//!
//! Windows: the "Choose your app mode" setting (Settings › Personalization › Colors), read from the registry
//! (`HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`, `AppsUseLightTheme`: 0 is dark).
//! Linux: GNOME's `color-scheme` (`prefer-dark`), else a GTK theme with "dark" in its name.
//! Anything unreadable counts as light.

#[cfg(windows)]
pub fn is_dark() -> bool {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};

    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let value = wide("AppsUseLightTheme");
    let mut data: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: both names end in 0, and `data` is a u32 with its size in `size`
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut data as *mut u32).cast(),
            &mut size,
        )
    };
    status == ERROR_SUCCESS && data == 0
}

#[cfg(not(windows))]
pub fn is_dark() -> bool {
    let get = |key: &str| {
        std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", key])
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).to_ascii_lowercase())
            .unwrap_or_default()
    };
    let scheme = get("color-scheme");
    if scheme.contains("dark") {
        return true;
    }
    if scheme.contains("light") {
        return false;
    }
    get("gtk-theme").contains("dark")
}
