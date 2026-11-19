//! Printing away from Windows (Linux, while developing): the page as a small PDF (Courier, which every PDF reader and
//! printer has, every letter the same width), printed with CUPS' `lp`.

use std::io::Write;
use std::process::{Command, Stdio};

use super::{Doc, Measure, Op, PAGE_H, Printers};

/// Courier: every letter 0.6 × the size wide
struct Courier;

impl Measure for Courier {
    fn width(&self, text: &str, size: f32, _bold: bool) -> f32 {
        text.chars().count() as f32 * size * 0.6
    }
}

/// Text for a PDF string: Windows-1252 letters, with ( ) \ escaped
fn pdf_text(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for c in s.chars() {
        let b = match c {
            '(' | ')' | '\\' => {
                out.push(b'\\');
                c as u8
            }
            ' '..='~' => c as u8,
            '·' => 0xb7,
            '—' => 0x97,
            '–' => 0x96,
            '…' => 0x85,
            '\u{a0}'..='\u{ff}' => c as u32 as u8,
            _ => b'?',
        };
        out.push(b);
    }
    out
}

/// The document as a PDF file
pub fn pdf(doc: &Doc) -> Vec<u8> {
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        Vec::new(), // the page list, once the pages are numbered
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>".to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier-Bold /Encoding /WinAnsiEncoding >>".to_vec(),
    ];
    let mut kids = Vec::new();
    for page in &doc.pages {
        let mut s: Vec<u8> = Vec::new();
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
                    let base = PAGE_H - y - size * 0.8; // PDF counts up from the bottom, to the text's baseline
                    let _ = write!(
                        s,
                        "BT /F{} {size:.2} Tf {} g 1 0 0 1 {x:.2} {base:.2} Tm (",
                        if *bold { 2 } else { 1 },
                        if *grey { "0.33" } else { "0" }
                    );
                    s.extend(pdf_text(text));
                    s.extend_from_slice(b") Tj ET\n");
                }
                Op::Rule { x, y, w, h, grey } => {
                    let _ = writeln!(
                        s,
                        "{} g {x:.2} {:.2} {w:.2} {h:.2} re f",
                        if *grey { "0.73" } else { "0" },
                        PAGE_H - y - h
                    );
                }
            }
        }
        let content = objects.len() + 1;
        let mut stream = format!("<< /Length {} >>\nstream\n", s.len()).into_bytes();
        stream.extend(s);
        stream.extend_from_slice(b"\nendstream");
        objects.push(stream);
        kids.push(objects.len() + 1);
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {content} 0 R >>"
            )
            .into_bytes(),
        );
    }
    objects[1] = format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.iter().map(|k| format!("{k} 0 R")).collect::<Vec<_>>().join(" "),
        kids.len()
    )
    .into_bytes();

    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).into_bytes());
        out.extend(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).into_bytes());
    for o in offsets {
        out.extend(format!("{o:010} 00000 n \n").into_bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).into_bytes());
    out
}

/// The printers CUPS knows (`lpstat -e`), and its default (`lpstat -d`)
pub fn printers() -> Printers {
    let run = |args: &[&str]| {
        Command::new("lpstat")
            .args(args)
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    let names = run(&["-e"])
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    // "system default destination: Office"
    let default = run(&["-d"])
        .split_once(':')
        .map(|(_, d)| d.trim().to_string())
        .filter(|d| !d.is_empty() && !d.contains(' '));
    Printers { names, default }
}

pub fn preview(build: &dyn Fn(&dyn Measure) -> Doc) -> Doc {
    build(&Courier)
}

pub fn print_to(printer: &str, copies: u32, build: &dyn Fn(&dyn Measure) -> Doc) -> Result<(), String> {
    let doc = build(&Courier);
    let file = pdf(&doc);
    let mut lp = Command::new("lp")
        .args(["-d", printer, "-n", &copies.to_string(), "-t", &doc.title, "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't start `lp` (CUPS) to print: {e}"))?;
    lp.stdin.take().expect("lp's input").write_all(&file).map_err(|e| e.to_string())?;
    let out = lp.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("`lp` said: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_a_pdf_with_every_page() {
        let doc = Doc {
            title: "t".into(),
            pages: vec![
                vec![Op::Text {
                    x: 54.0,
                    y: 54.0,
                    size: 12.0,
                    bold: true,
                    grey: false,
                    text: "RO (1) · $5".into(),
                }],
                vec![Op::Rule {
                    x: 54.0,
                    y: 80.0,
                    w: 100.0,
                    h: 1.0,
                    grey: true,
                }],
            ],
        };
        let f = pdf(&doc);
        let s = String::from_utf8_lossy(&f);
        assert!(s.starts_with("%PDF-1.4") && s.ends_with("%%EOF\n"));
        assert!(s.contains("/Count 2"));
        assert!(f.windows(10).any(|w| w == b"RO \\(1\\) \xb7"));
    }
}
