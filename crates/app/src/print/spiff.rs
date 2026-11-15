//! The spiff sheet: RO and tag at the top, technician (name and number) and advisor, then each approved
//! recommendation that counts toward the spiff (line, what it is, labor type, hours, price) and their total.
//! No customer details.

use xtime::ro::{RoView, SPIFF_OVER};

use super::{Doc, MARGIN, Measure, Op, PAGE_H, PAGE_W, money, wrap};

/// What the sheet needs besides the RO
pub struct Extra {
    /// "Ashton M (#456)": you, when Xtime's RO doesn't name its technician
    pub technician: String,
    /// "Printed Nov 15, 2026, 1:11 PM"
    pub printed: String,
}

/// Lays a page out from the top down
struct Page<'m> {
    m: &'m dyn Measure,
    pages: Vec<Vec<Op>>,
    y: f32,
}

impl Page<'_> {
    fn ops(&mut self) -> &mut Vec<Op> {
        self.pages.last_mut().expect("a page")
    }
    fn text(&mut self, x: f32, size: f32, bold: bool, grey: bool, text: impl Into<String>) {
        let y = self.y;
        self.ops().push(Op::Text {
            x,
            y,
            size,
            bold,
            grey,
            text: text.into(),
        });
    }
    /// Text ending at `right`
    fn text_right(&mut self, right: f32, size: f32, bold: bool, grey: bool, text: &str) {
        let x = right - self.m.width(text, size, bold);
        self.text(x, size, bold, grey, text);
    }
    /// Text centred on `mid`
    fn text_centre(&mut self, mid: f32, size: f32, bold: bool, grey: bool, text: &str) {
        let x = mid - self.m.width(text, size, bold) / 2.0;
        self.text(x, size, bold, grey, text);
    }
    fn rule(&mut self, h: f32, grey: bool) {
        let y = self.y;
        self.ops().push(Op::Rule {
            x: MARGIN,
            y,
            w: PAGE_W - 2.0 * MARGIN,
            h,
            grey,
        });
    }
    fn new_page(&mut self) {
        self.pages.push(Vec::new());
        self.y = MARGIN;
    }
}

// the table's columns
const LEFT: f32 = MARGIN;
const RIGHT: f32 = PAGE_W - MARGIN;
const LINE_MID: f32 = LEFT + 16.0;
const NAME_X: f32 = LEFT + 40.0;
const NAME_RIGHT: f32 = RIGHT - 190.0;
const TYPE_MID: f32 = RIGHT - 160.0;
const HOURS_RIGHT: f32 = RIGHT - 95.0;
const ROW: f32 = 11.0; // the rows' text size

pub fn sheet(m: &dyn Measure, v: &RoView, extra: &Extra) -> Doc {
    let mut p = Page {
        m,
        pages: vec![Vec::new()],
        y: MARGIN,
    };

    // RO 820367, then the vehicle and when it was printed
    p.text(LEFT, 22.0, true, false, format!("RO {}", v.ro));
    p.y += 30.0;
    let ymm = [v.vehicle.year.as_str(), v.vehicle.make.as_str(), v.vehicle.model.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    let sub = [ymm.as_str(), extra.printed.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("  ·  ");
    p.text(LEFT, 11.0, false, true, sub);
    p.y += 30.0;

    // RO, tag, technician, advisor: two by two
    let technician = if v.technician.name.is_empty() {
        extra.technician.clone()
    } else if v.technician.number.is_empty() {
        v.technician.name.clone()
    } else {
        format!("{} (#{})", v.technician.name, v.technician.number)
    };
    let facts = [
        ("RO NUMBER", v.ro.clone()),
        ("TAG NUMBER", v.tag.clone()),
        ("TECHNICIAN", technician),
        ("ADVISOR", v.advisor.name.clone()),
    ];
    for pair in facts.chunks(2) {
        for (i, (label, value)) in pair.iter().enumerate() {
            let x = LEFT + i as f32 * (RIGHT - LEFT) / 2.0;
            let top = p.y;
            p.text(x, 8.5, false, true, *label);
            p.y += 12.0;
            p.text(x, 13.0, true, false, if value.is_empty() { "—" } else { value.as_str() });
            p.y = top;
        }
        p.y += 38.0;
    }
    p.y += 6.0;

    // the table: its head (again on each new page), a row per line, the total
    let head = |p: &mut Page| {
        p.text_centre(LINE_MID, 8.5, true, true, "LINE");
        p.text(NAME_X, 8.5, true, true, "RECOMMENDATION");
        p.text_centre(TYPE_MID, 8.5, true, true, "TYPE");
        p.text_right(HOURS_RIGHT, 8.5, true, true, "HOURS");
        p.text_right(RIGHT, 8.5, true, true, "PRICE");
        p.y += 14.0;
        p.rule(1.5, false);
        p.y += 8.0;
    };
    head(&mut p);
    let bottom = PAGE_H - MARGIN;
    let lines = v.spiff_lines();
    for (i, l) in lines.iter().enumerate() {
        let name = if l.name.is_empty() { &l.op_code } else { &l.name };
        let wrapped = wrap(m, name, ROW, false, NAME_RIGHT - NAME_X);
        let height = wrapped.len() as f32 * 14.0 + 8.0;
        if p.y + height > bottom - 50.0 {
            p.new_page();
            head(&mut p);
        }
        let top = p.y;
        p.text_centre(LINE_MID, ROW, false, false, &l.dms);
        p.text_centre(TYPE_MID, ROW, false, false, &l.pay_type);
        p.text_right(
            HOURS_RIGHT,
            ROW,
            false,
            false,
            &l.hours.map(|h| format!("{h:.1}")).unwrap_or_default(),
        );
        p.text_right(RIGHT, ROW, false, false, &money(l.total.unwrap_or(0.0)));
        for line in wrapped {
            p.text(NAME_X, ROW, false, false, line);
            p.y += 14.0;
        }
        p.y = top + height - 4.0;
        // a light line between rows (the last one has the total's line under it instead)
        if i + 1 < lines.len() {
            p.rule(0.5, true);
        }
        p.y += 8.0;
    }
    p.y -= 4.0;
    p.rule(1.5, false);
    p.y += 9.0;
    p.text(NAME_X, 12.0, true, false, "Total approved recommendations");
    p.text_right(RIGHT, 12.0, true, false, &money(v.spiff_total()));
    p.y += 26.0;
    p.text(
        LEFT,
        9.0,
        false,
        true,
        format!(
            "Approved Additional Service Recommendations on this RO, over {}.",
            money(SPIFF_OVER)
        ),
    );

    Doc {
        title: format!("RO {} spiff", v.ro),
        pages: p.pages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::print::tests::Even;
    use xtime::ro::{Line, Person};

    fn ro(n: usize) -> RoView {
        let line = |i: usize| Line {
            id: serde_json::json!(i),
            dms: format!("{}", (b'B' + (i % 20) as u8) as char),
            name: "Front brake pads and rotors, both sides".into(),
            approved: true,
            pay_type: "C".into(),
            hours: Some(1.5),
            total: Some(100.0),
            ..Default::default()
        };
        RoView {
            ro: "820367".into(),
            tag: "T12".into(),
            technician: Person {
                name: "Ashton M".into(),
                number: "456".into(),
            },
            lines: (0..n).map(line).collect(),
            ..Default::default()
        }
    }

    fn texts(doc: &Doc) -> Vec<String> {
        doc.pages
            .iter()
            .flatten()
            .filter_map(|o| match o {
                Op::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn has_what_the_service_manager_needs() {
        let extra = Extra {
            technician: String::new(),
            printed: "Printed Nov 15, 2026".into(),
        };
        let doc = sheet(&Even, &ro(2), &extra);
        let t = texts(&doc);
        assert_eq!(t[0], "RO 820367"); // the top says the RO, nothing else
        for want in ["T12", "Ashton M (#456)", "$200.00", "Total approved recommendations"] {
            assert!(t.iter().any(|s| s == want), "missing {want}");
        }
        assert_eq!(doc.pages.len(), 1);
        // everything on the page
        for op in doc.pages.iter().flatten() {
            if let Op::Text { x, y, .. } = op {
                assert!(*x >= MARGIN - 0.1 && *y < PAGE_H - MARGIN, "{op:?}");
            }
        }
    }

    #[test]
    fn long_lists_go_onto_more_pages() {
        let extra = Extra {
            technician: String::new(),
            printed: String::new(),
        };
        let doc = sheet(&Even, &ro(40), &extra);
        assert!(doc.pages.len() > 1);
        assert_eq!(texts(&doc).iter().filter(|s| *s == "RECOMMENDATION").count(), doc.pages.len());
    }
}
