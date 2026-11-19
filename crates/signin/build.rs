//! Puts the app's icon (assets/icon.ico) in the .exe on Windows.

fn main() {
    exe_icon();
}

/// The icon in the .exe (Explorer, the taskbar, shortcuts): assets/icon.ico, put in by Windows' resource compiler
/// (rc.exe, from the Visual Studio Build Tools Rust already needs). Without it the build still works, iconless.
fn exe_icon() {
    let ico = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/icon.ico");
    println!("cargo:rerun-if-changed={ico}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon(ico);
        if let Err(e) = res.compile() {
            println!("cargo:warning=The icon wasn't put in the .exe ({e})");
        }
    }
}
