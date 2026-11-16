//! Puts PDFium (the PDF engine the Service Manual's document tabs draw PDFs with, crates/ford-ui/pdf.rs) beside the
//! app when it's built: `pdfium.dll` on Windows, `libpdfium.so` on Linux, `libpdfium.dylib` on macOS, in the same
//! folder as the app (target/debug or target/release). It's downloaded once from the PDFium builds the
//! pdfium-render crate is made for (github.com/bblanchon/pdfium-binaries), then kept.
//!
//! Without a connection the build still works (with a warning): PDFs then open in the computer's PDF viewer.
//! It also puts the app's icon in the .exe on Windows (assets/icon.ico).
//!
//! `PTS_NO_PDFIUM=1` skips it; `PDFIUM_URL` downloads from somewhere else (a .tgz laid out the same way).

use std::io::Read;
use std::path::{Path, PathBuf};

const RELEASES: &str = "https://github.com/bblanchon/pdfium-binaries/releases/latest/download";

fn main() {
    exe_icon();
    println!("cargo:rerun-if-env-changed=PTS_NO_PDFIUM");
    println!("cargo:rerun-if-env-changed=PDFIUM_URL");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var_os("PTS_NO_PDFIUM").is_some() {
        return;
    }
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    // the download for this computer, and where the library is in it
    let (package, inside, name) = match (os.as_str(), arch.as_str()) {
        ("windows", "x86_64") => ("pdfium-win-x64", "bin/pdfium.dll", "pdfium.dll"),
        ("windows", "aarch64") => ("pdfium-win-arm64", "bin/pdfium.dll", "pdfium.dll"),
        ("linux", "x86_64") => ("pdfium-linux-x64", "lib/libpdfium.so", "libpdfium.so"),
        ("linux", "aarch64") => ("pdfium-linux-arm64", "lib/libpdfium.so", "libpdfium.so"),
        ("macos", "aarch64") => ("pdfium-mac-arm64", "lib/libpdfium.dylib", "libpdfium.dylib"),
        ("macos", "x86_64") => ("pdfium-mac-x64", "lib/libpdfium.dylib", "libpdfium.dylib"),
        _ => {
            println!("cargo:warning=No PDFium download for {os} {arch}: PDFs will open in the PDF viewer");
            return;
        }
    };
    let Some(dir) = app_folder() else {
        println!("cargo:warning=Couldn't tell where the app is built to: PDFium wasn't put beside it");
        return;
    };
    let dest = dir.join(name);
    if dest.exists() {
        return; // there already (kept between builds)
    }
    let url = std::env::var("PDFIUM_URL").unwrap_or_else(|_| format!("{RELEASES}/{package}.tgz"));
    match fetch(&url, inside, &dest) {
        Ok(()) => println!("cargo:warning=PDFium put beside the app ({})", dest.display()),
        Err(e) => println!("cargo:warning=Couldn't get PDFium ({e}): PDFs will open in the PDF viewer until it's beside the app"),
    }
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

/// target/<profile>: three folders up from this build's OUT_DIR (target/<profile>/build/<crate>-<hash>/out)
fn app_folder() -> Option<PathBuf> {
    let out = PathBuf::from(std::env::var_os("OUT_DIR")?);
    let dir = out.ancestors().nth(3)?.to_path_buf();
    dir.is_dir().then_some(dir)
}

/// Downloads the .tgz, and takes the one file out of it (written whole, or not at all)
fn fetch(url: &str, inside: &str, dest: &Path) -> Result<(), String> {
    let res = ureq::get(url).call().map_err(|e| e.to_string())?;
    let mut tgz = Vec::new();
    res.into_reader()
        .take(200 * 1024 * 1024)
        .read_to_end(&mut tgz)
        .map_err(|e| e.to_string())?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(tgz.as_slice()));
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
        if path.trim_start_matches("./") == inside {
            let tmp = dest.with_extension("part");
            entry.unpack(&tmp).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    Err(format!("{inside} wasn't in the download"))
}
