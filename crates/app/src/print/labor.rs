//! Labor Times' printout (the Electron app's "Print"): the operations picked with their warranty and customer pay
//! times, the totals, Ford's warnings and each operation's one-time use parts; or one parts list on its own.

use ford::labor::{CUSTOMER_PAY, hrs, num};
use ford_ui::labor::LaborPrint;

use super::{Doc, MARGIN, Measure, Op, PAGE_H, PAGE_W, wrap};

const RIGHT: f32 = PAGE_W - MARGIN;
const WIDTH: f32 = PAGE_W - 2.0 * MARGIN;
const SIZE: f32 = 10.0;
const LINE: f32 = 13.5;

/// Pages filled from the top, a new one when what's next doesn't fit
struct Pages<'a> {
    m: &'a dyn Measure,
    pages: Vec<Vec<Op>>,
    ops: Vec<Op>,
    y: f32,
}

impl<'a> Pages<'a> {
    fn new(m: &'a dyn Measure) -> Self {
        Pages {
            m,
            pages: Vec::new(),
            ops: Vec::new(),
            y: MARGIN,
        }
    }

    /// Room for `h` more, or a new page
    fn need(&mut self, h: f32) {
        if self.y + h > PAGE_H - MARGIN && !self.ops.is_empty() {
            self.pages.push(std::mem::take(&mut self.ops));
            self.y = MARGIN;
        }
    }

    fn text(&mut self, x: f32, y: f32, size: f32, bold: bool, grey: bool, text: impl Into<String>) {
        let text = text.into();
        if !text.is_empty() {
            self.ops.push(Op::Text {
                x,
                y,
                size,
                bold,
                grey,
                text,
            });
        }
    }

    /// Text ending at `right`
    fn right(&mut self, right: f32, y: f32, size: f32, bold: bool, grey: bool, text: &str) {
        let w = self.m.width(text, size, bold);
        self.text(right - w, y, size, bold, grey, text);
    }

    fn rule(&mut self, grey: bool) {
        self.ops.push(Op::Rule {
            x: MARGIN,
            y: self.y,
            w: WIDTH,
            h: if grey { 0.5 } else { 1.0 },
            grey,
        });
    }

    /// A paragraph, wrapped
    fn para(&mut self, x: f32, size: f32, bold: bool, grey: bool, text: &str) {
        for line in wrap(self.m, text, size, bold, RIGHT - x) {
            self.need(size * 1.35);
            let y = self.y;
            self.text(x, y, size, bold, grey, line);
            self.y += size * 1.35;
        }
    }

    fn done(mut self) -> Vec<Vec<Op>> {
        if !self.ops.is_empty() || self.pages.is_empty() {
            self.pages.push(self.ops);
        }
        self.pages
    }
}

/// The table of operations: op code, operation (wrapped), warranty, customer pay
fn operations(p: &mut Pages, job: &LaborPrint) {
    let (x_code, x_name) = (MARGIN, MARGIN + 82.0);
    let (r_w, r_cp) = (RIGHT - 92.0, RIGHT);
    let name_w = r_w - 64.0 - x_name;
    p.need(LINE * 3.0);
    let y = p.y;
    for (x, label) in [(x_code, "OP CODE"), (x_name, "OPERATION")] {
        p.text(x, y, 8.0, true, true, label);
    }
    p.right(r_w, y, 8.0, true, true, "WARRANTY");
    p.right(r_cp, y, 8.0, true, true, "CUSTOMER PAY");
    p.y += 12.0;
    p.rule(false);
    p.y += 5.0;
    for t in &job.tray {
        let name = [t.name.as_str(), t.qual.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" · ");
        let lines = wrap(p.m, &name, SIZE, false, name_w);
        p.need(LINE * lines.len() as f32 + 6.0);
        let y = p.y;
        p.text(x_code, y, SIZE, true, false, t.code.as_str());
        let w = num(&t.time);
        let (wt, cp) = match w {
            Some(n) => (hrs(n), hrs(n * CUSTOMER_PAY)),
            None => (if t.time.is_empty() { "—".to_string() } else { t.time.clone() }, "—".to_string()),
        };
        p.right(r_w, y, SIZE, true, false, &wt);
        p.right(r_cp, y, SIZE, false, false, &cp);
        for (i, l) in lines.into_iter().enumerate() {
            p.text(x_name, y + i as f32 * LINE, SIZE, false, false, l);
        }
        p.y = y + LINE * wrap(p.m, &name, SIZE, false, name_w).len() as f32 + 3.0;
        p.rule(true);
        p.y += 5.0;
    }
    // the totals
    let (w, actual) = ford::labor::tray_total(&job.tray);
    p.need(LINE * 2.0);
    let y = p.y + 2.0;
    p.text(x_name, y, SIZE + 1.0, true, false, "Total");
    p.right(r_w, y, SIZE + 1.0, true, false, &format!("{} hrs", hrs(w)));
    p.right(r_cp, y, SIZE + 1.0, true, false, &format!("{} hrs", hrs(w * CUSTOMER_PAY)));
    p.y = y + LINE + 4.0;
    if actual > 0 {
        let note = format!(
            "{actual} operation{} without a set time (actual time) not included. Customer pay = warranty × {CUSTOMER_PAY}.",
            if actual == 1 { "" } else { "s" }
        );
        p.para(MARGIN, 8.5, false, true, &note);
    } else {
        p.para(MARGIN, 8.5, false, true, &format!("Customer pay = warranty × {CUSTOMER_PAY}."));
    }
    if !job.warnings.is_empty() {
        p.y += 6.0;
        for w in &job.warnings {
            p.para(MARGIN, 9.0, false, false, &format!("⚠ {w}"));
        }
    }
}

/// Each operation's one-time use parts: a heading, then its table (whatever columns SLTS gave), or why there's none
fn parts(p: &mut Pages, job: &LaborPrint, heading: bool) {
    if job.parts.is_empty() {
        return;
    }
    if heading {
        p.y += 14.0;
        p.need(40.0);
        let y = p.y;
        p.text(MARGIN, y, 12.0, true, false, "One-time use parts");
        p.y += 18.0;
    }
    for b in &job.parts {
        p.y += 4.0;
        p.need(LINE * 3.0);
        let y = p.y;
        p.text(MARGIN, y, SIZE, true, false, b.code.as_str());
        let x = MARGIN + p.m.width(&b.code, SIZE, true) + 10.0;
        let name: String = b.name.clone();
        p.text(x, y, SIZE, false, true, name);
        p.y += LINE + 2.0;
        let t = &b.parts;
        if t.rows.is_empty() {
            let note = if t.note.is_empty() {
                "No one-time use parts listed."
            } else {
                t.note.as_str()
            };
            p.para(MARGIN + 10.0, 9.0, false, true, note);
            continue;
        }
        let cols = t.head.len().max(t.rows.iter().map(Vec::len).max().unwrap_or(0)).max(1);
        // the description (the second column, when there are three or more) gets the most room
        let shares: Vec<f32> = (0..cols).map(|j| if j == 1 && cols > 2 { 3.0 } else { 1.0 }).collect();
        let total: f32 = shares.iter().sum();
        let avail = WIDTH - 10.0 - 8.0 * (cols as f32 - 1.0);
        let widths: Vec<f32> = shares.iter().map(|s| avail * s / total).collect();
        let xs: Vec<f32> = widths
            .iter()
            .scan(MARGIN + 10.0, |x, w| {
                let at = *x;
                *x += w + 8.0;
                Some(at)
            })
            .collect();
        let row = |p: &mut Pages, cells: &[String], size: f32, bold: bool, grey: bool| {
            let wrapped: Vec<Vec<String>> = (0..cols)
                .map(|j| wrap(p.m, cells.get(j).map(String::as_str).unwrap_or(""), size, bold, widths[j]))
                .collect();
            let n = wrapped.iter().map(Vec::len).max().unwrap_or(1);
            p.need(n as f32 * size * 1.35 + 4.0);
            let y = p.y;
            for (j, lines) in wrapped.into_iter().enumerate() {
                for (i, l) in lines.into_iter().enumerate() {
                    p.text(xs[j], y + i as f32 * size * 1.35, size, bold, grey, l);
                }
            }
            p.y = y + n as f32 * size * 1.35 + 3.0;
        };
        if !t.head.is_empty() {
            let head: Vec<String> = t.head.iter().map(|h| h.to_uppercase()).collect();
            row(p, &head, 7.5, true, true);
            p.rule(true);
            p.y += 3.0;
        }
        for r in &t.rows {
            row(p, r, 9.0, false, false);
        }
    }
}

pub fn sheet(m: &dyn Measure, job: &LaborPrint, printed: &str) -> Doc {
    let mut p = Pages::new(m);
    let y = p.y;
    p.text(MARGIN, y, 16.0, true, false, job.title.as_str());
    p.right(RIGHT, y + 4.0, 8.5, false, true, printed);
    p.y += 22.0;
    if !job.vehicle.is_empty() {
        p.para(MARGIN, 10.0, false, true, &job.vehicle);
    }
    p.y += 10.0;
    let tray = !job.tray.is_empty();
    if tray {
        operations(&mut p, job);
    }
    parts(&mut p, job, tray);
    Doc {
        title: job.title.clone(),
        pages: p.done(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ford::labor::{Parts, TrayItem};
    use ford_ui::labor::PartsBlock;

    /// Every letter 5 points wide
    struct Even;
    impl Measure for Even {
        fn width(&self, text: &str, _: f32, _: bool) -> f32 {
            text.chars().count() as f32 * 5.0
        }
    }

    fn texts(d: &Doc) -> Vec<String> {
        d.pages
            .iter()
            .flatten()
            .filter_map(|o| match o {
                Op::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn lays_out_the_tray() {
        let item = |code: &str, time: &str| TrayItem {
            code: code.into(),
            time: time.into(),
            name: "Compressor - A/C".into(),
            ..Default::default()
        };
        let job = LaborPrint {
            title: "Labor operations".into(),
            vehicle: "2023 Ford F-150 · RO 1".into(),
            tray: vec![item("19703A1", "2.4"), item("19703A40", "0.3"), item("10300A", "AT")],
            warnings: vec!["19703A1 and 19704A overlap".into()],
            parts: vec![PartsBlock {
                code: "19703A1".into(),
                name: "Compressor - A/C".into(),
                parts: Parts {
                    head: vec!["Part".into(), "Description".into(), "Qty".into()],
                    rows: vec![vec!["W123".into(), "O-ring".into(), "2".into()]],
                    note: String::new(),
                },
            }],
        };
        let d = sheet(&Even, &job, "Printed today");
        let t = texts(&d);
        for want in [
            "Labor operations",
            "19703A1",
            "2.4",
            "3.6",
            "2.7 hrs",
            "4.05 hrs",
            "One-time use parts",
            "W123",
            "O-ring",
        ] {
            assert!(t.iter().any(|x| x == want), "{want} missing from {t:?}");
        }
        assert!(t.iter().any(|x| x.starts_with("1 operation without a set time")));
        // a long list goes over pages
        let long = LaborPrint {
            tray: (0..80).map(|i| item(&format!("1{i:04}A"), "1.0")).collect(),
            ..job
        };
        assert!(sheet(&Even, &long, "").pages.len() >= 2);
    }
}