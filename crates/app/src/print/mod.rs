//! Printing, inside the app (no web browser): a page is laid out here as text and lines in points (1/72 inch, US
//! Letter), then drawn straight onto the printer.
//!
//!   mod.rs      the page: text and lines, measuring and wrapping text, money; the printers, preview and printing
//!   spiff.rs    the spiff sheet
//!   windows.rs  Windows: the printers, measuring text, drawing the pages on the printer
//!   pdf.rs      elsewhere (Linux, while developing): the page as a small PDF, printed with `lp`
//!
//! The Print window itself is the app's own (ui/print.rs): a preview of every page, the printer and copies.

#[cfg(not(windows))]
mod pdf;
pub mod spiff;
#[cfg(windows)]
mod windows;

/// US Letter, in points
pub const PAGE_W: f32 = 612.0;
pub const PAGE_H: f32 = 792.0;
/// Room left around the page's edge
pub const MARGIN: f32 = 54.0;

/// Something drawn on a page. `y` is the top of the text (not its baseline).
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Text {
        x: f32,
        y: f32,
        size: f32,
        bold: bool,
        /// Grey (labels, notes) rather than black
        grey: bool,
        text: String,
    },
    /// A filled line across: `w` long, `h` thick
    Rule { x: f32, y: f32, w: f32, h: f32, grey: bool },
}

/// What's printed: a name (the printer's queue shows it) and its pages
#[derive(Debug, Clone, Default)]
pub struct Doc {
    pub title: String,
    pub pages: Vec<Vec<Op>>,
}

/// How wide text comes out on the printer (in points), so it can be lined up and wrapped
pub trait Measure {
    fn width(&self, text: &str, size: f32, bold: bool) -> f32;
}

/// Lines of text no wider than `width` (words kept whole; a word too long on its own is cut)
pub fn wrap(m: &dyn Measure, text: &str, size: f32, bold: bool, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let tried = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if m.width(&tried, size, bold) <= width || line.is_empty() && m.width(word, size, bold) <= width {
            line = tried;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        // a word wider than the column on its own: cut it where it fits
        let mut rest = word.to_string();
        while m.width(&rest, size, bold) > width && rest.chars().count() > 1 {
            let mut cut = rest.chars().count() - 1;
            while cut > 1 && m.width(&rest.chars().take(cut).collect::<String>(), size, bold) > width {
                cut -= 1;
            }
            lines.push(rest.chars().take(cut).collect());
            rest = rest.chars().skip(cut).collect();
        }
        line = rest;
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// $1,234.50
pub fn money(n: f64) -> String {
    let cents = (n * 100.0).round() as i64;
    let (whole, part) = (cents.abs() / 100, cents.abs() % 100);
    let digits = whole.to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("{}${grouped}.{part:02}", if cents < 0 { "-" } else { "" })
}

/// The printers there are, and which is the default
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Printers {
    pub names: Vec<String>,
    pub default: Option<String>,
}

/// The printers this computer has (asks the system: run it away from the app's window, `spawn_blocking`)
pub fn printers() -> Printers {
    #[cfg(windows)]
    return windows::printers();
    #[cfg(not(windows))]
    return pdf::printers();
}

/// Opens the system's own place to add and set up printers (Windows: Settings › Printers & scanners)
pub fn manage_printers() -> Result<(), String> {
    #[cfg(windows)]
    let opened = std::process::Command::new("explorer").arg("ms-settings:printers").spawn();
    #[cfg(not(windows))]
    let opened = std::process::Command::new("system-config-printer")
        .spawn()
        .or_else(|_| std::process::Command::new("xdg-open").arg("http://localhost:631/printers/").spawn());
    opened.map(|_| ()).map_err(|e| format!("couldn't open the printer settings: {e}"))
}

/// The document as it'll print, for the app's Print window to show (measured on the screen, close to the printer)
pub fn preview(build: &dyn Fn(&dyn Measure) -> Doc) -> Doc {
    #[cfg(windows)]
    return windows::preview(build);
    #[cfg(not(windows))]
    return pdf::preview(build);
}

/// Prints a document on a printer, `copies` times. `build` lays it out for that printer (text measured as it draws
/// it). Waits for the printer to take it, so it's run away from the app's window (`spawn_blocking`).
pub fn print_to(printer: &str, copies: u32, build: &dyn Fn(&dyn Measure) -> Doc) -> Result<(), String> {
    #[cfg(windows)]
    return windows::print_to(printer, copies.clamp(1, 99), build);
    #[cfg(not(windows))]
    return pdf::print_to(printer, copies.clamp(1, 99), build);
}

/// Lays a document out (for whichever printer, or the preview)
pub type Build = std::sync::Arc<dyn Fn(&dyn Measure) -> Doc + Send + Sync>;

/// Something waiting in the app's Print window: what it is, how it looks, and where it'll go
pub struct Job {
    /// "Spiff for RO 820367"
    pub what: String,
    pub build: Build,
    pub preview: Doc,
    /// None while they're being looked up
    pub printers: Option<Printers>,
    pub printer: Option<String>,
    pub copies: u32,
    /// Sent to the printer, waiting for it to take it
    pub busy: bool,
    pub error: Option<String>,
}

impl Job {
    pub fn new(what: String, build: Build) -> Self {
        let preview = preview(&*build);
        Self {
            what,
            build,
            preview,
            printers: None,
            printer: None,
            copies: 1,
            busy: false,
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every letter 0.5 × the size wide
    pub struct Even;
    impl Measure for Even {
        fn width(&self, text: &str, size: f32, _bold: bool) -> f32 {
            text.chars().count() as f32 * size * 0.5
        }
    }

    #[test]
    fn wraps_words_to_the_width() {
        // 10 pt: 5 pt a letter, so 50 pt holds 10 letters
        assert_eq!(
            wrap(&Even, "Front brake pads and rotors", 10.0, false, 50.0),
            ["Front", "brake pads", "and rotors"]
        );
        assert_eq!(wrap(&Even, "short", 10.0, false, 50.0), ["short"]);
        assert_eq!(wrap(&Even, "", 10.0, false, 50.0), [""]);
        assert_eq!(wrap(&Even, "ABCDEFGHIJKLMNO", 10.0, false, 50.0), ["ABCDEFGHIJ", "KLMNO"]);
    }

    #[test]
    fn writes_money() {
        assert_eq!(money(1234.5), "$1,234.50");
        assert_eq!(money(40.0), "$40.00");
        assert_eq!(money(0.005), "$0.01");
        assert_eq!(money(-3.0), "-$3.00");
    }
}
