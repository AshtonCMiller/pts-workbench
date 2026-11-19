//! PDFs drawn as pages, inside the app (an FSA's dealer bulletin, instructions…): with PDFium, the PDF engine
//! Chrome uses, loaded from beside the app (`pdfium.dll` on Windows, `libpdfium.so` on Linux; no installing), or from
//! the system. Without it, a PDF can still be opened in the computer's PDF viewer.
//!
//! PDFium works on one thread of its own here: each PDF is handed to it and its pages come back as pictures.

use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};

use iced::widget::image::Handle;
use pdfium_render::prelude::*;

/// Why a PDF couldn't be drawn
#[derive(Debug, Clone)]
pub enum PdfFail {
    /// PDFium isn't beside the app (the PDF itself, to open elsewhere)
    NoPdfium(Arc<Vec<u8>>),
    Failed(String),
}

/// How wide pages are drawn (pixels), and the most pages drawn
const WIDTH: i32 = 1200;
const MAX_PAGES: usize = 60;

struct Job {
    bytes: Vec<u8>,
    reply: mpsc::Sender<Result<Vec<(u32, u32, Vec<u8>)>, String>>,
}

/// PDFium's thread (started the first time a PDF is drawn); none when PDFium can't be found
fn worker() -> Option<mpsc::Sender<Job>> {
    static WORKER: OnceLock<Option<Mutex<mpsc::Sender<Job>>>> = OnceLock::new();
    WORKER
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel::<Job>();
            let (ready_tx, ready_rx) = mpsc::channel::<bool>();
            std::thread::Builder::new()
                .name("pdfium".into())
                .spawn(move || {
                    let Some(pdfium) = bind() else {
                        let _ = ready_tx.send(false);
                        return;
                    };
                    let _ = ready_tx.send(true);
                    for job in rx {
                        let _ = job.reply.send(draw(&pdfium, job.bytes));
                    }
                })
                .ok()?;
            ready_rx.recv().ok().filter(|ok| *ok).map(|_| Mutex::new(tx))
        })
        .as_ref()
        .and_then(|m| m.lock().ok().map(|s| s.clone()))
}

/// PDFium from beside the app, else the system's
fn bind() -> Option<Pdfium> {
    let beside = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let bindings = beside
        .and_then(|d| Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&d)).ok())
        .or_else(|| Pdfium::bind_to_system_library().ok())?;
    Some(Pdfium::new(bindings))
}

/// Each page as a picture: width, height, RGBA pixels
fn draw(pdfium: &Pdfium, bytes: Vec<u8>) -> Result<Vec<(u32, u32, Vec<u8>)>, String> {
    let doc = pdfium
        .load_pdf_from_byte_vec(bytes, None)
        .map_err(|e| format!("not a readable PDF ({e})"))?;
    let config = PdfRenderConfig::new().set_target_width(WIDTH);
    let mut out = Vec::new();
    for (i, page) in doc.pages().iter().enumerate() {
        if i >= MAX_PAGES {
            break;
        }
        let bmp = page
            .render_with_config(&config)
            .map_err(|e| format!("couldn't draw page {} ({e})", i + 1))?;
        let (w, h) = (bmp.width() as u32, bmp.height() as u32);
        // PDFium draws in BGRA: as RGBA
        let mut px: Vec<u8> = bmp.as_raw_bytes().to_vec();
        for c in px.chunks_exact_mut(4) {
            c.swap(0, 2);
        }
        out.push((w, h, px));
    }
    Ok(out)
}

/// Draws a PDF's pages (on PDFium's thread)
pub async fn render(bytes: Vec<u8>) -> Result<Vec<Handle>, PdfFail> {
    let Some(tx) = worker() else {
        return Err(PdfFail::NoPdfium(Arc::new(bytes)));
    };
    let (reply, answer) = mpsc::channel();
    tx.send(Job { bytes, reply })
        .map_err(|_| PdfFail::Failed("the PDF engine stopped".into()))?;
    let pages = tokio::task::spawn_blocking(move || answer.recv())
        .await
        .map_err(|e| PdfFail::Failed(e.to_string()))?
        .map_err(|_| PdfFail::Failed("the PDF engine stopped".into()))?
        .map_err(PdfFail::Failed)?;
    Ok(pages.into_iter().map(|(w, h, px)| Handle::from_rgba(w, h, px)).collect())
}
