//! The lines of the vehicle showing, in the sidebar (the Electron app's `linesPanel`): each line that counts (approved,
//! the concern, or done), what it pays, Flag / ✓ Flagged, and when it was flagged. Click the hours to change what the
//! line really pays (after warranty, say: empty goes back to Xtime's), the date to move the flag to the day it was
//! really flagged (an earlier pay period). "Lines" folds them away (remembered per RO, on this computer).

use chrono::{Local, TimeZone};
use hours::{LineRow, LineStatus, RoSummary};
use iced::widget::{Column, button, column, container, horizontal_space, row, text, text_input};
use iced::{Alignment, Element, Length};
use ui_kit::{pill, style};

use crate::app::{App, LineEdit, LineField, Message};

/// The box a line's hours or date are typed in
pub fn edit_id() -> text_input::Id {
    text_input::Id::new("sidebar-line-edit")
}

fn fmt_h(h: f64) -> String {
    hours_ui::fmt_h(h)
}

/// A time (ms) as "Nov 2"
pub fn short_date(ms: i64) -> String {
    Local
        .timestamp_millis_opt(ms)
        .earliest()
        .map(|t| t.format("%b %-d").to_string())
        .unwrap_or_default()
}

/// A time (ms) as "11/02/2026", for changing it
pub fn edit_date(ms: i64) -> String {
    Local
        .timestamp_millis_opt(ms)
        .earliest()
        .map(|t| t.format("%m/%d/%Y").to_string())
        .unwrap_or_default()
}

/// "11/2/2026", "11/2/26", "11-2" (this year) → noon that day (ms), so it's in that day's pay period
pub fn parse_date(s: &str, this_year: i32) -> Option<i64> {
    let parts: Vec<&str> = s.trim().split(['/', '-', '.']).map(str::trim).filter(|p| !p.is_empty()).collect();
    let (m, d, y) = match parts.as_slice() {
        [m, d] => (m.parse::<u32>().ok()?, d.parse::<u32>().ok()?, this_year),
        [m, d, y] => {
            let y: i32 = y.parse().ok()?;
            (m.parse().ok()?, d.parse().ok()?, if y < 100 { 2000 + y } else { y })
        }
        _ => return None,
    };
    let day = chrono::NaiveDate::from_ymd_opt(y, m, d)?;
    Local
        .from_local_datetime(&day.and_hms_opt(12, 0, 0)?)
        .earliest()
        .map(|t| t.timestamp_millis())
}

/// The line's box being typed in, if it's this one
fn editing<'a>(app: &'a App, ro: &str, key: &str, field: LineField) -> Option<&'a LineEdit> {
    app.line_edit.as_ref().filter(|e| e.ro == ro && e.key == key && e.field == field)
}

/// The box being typed in, with ✓ and ×
fn editor<'a>(e: &'a LineEdit, placeholder: &'a str, width: f32) -> Element<'a, Message> {
    row![
        text_input(placeholder, &e.value)
            .id(edit_id())
            .on_input(Message::LineInput)
            .on_submit(Message::SaveLine)
            .size(13)
            .padding([3, 6])
            .width(width),
        button(text("✓").size(13))
            .padding([3, 7])
            .style(button::primary)
            .on_press(Message::SaveLine),
        button(text("×").size(13))
            .padding([3, 7])
            .style(style::ghost)
            .on_press(Message::CancelLine),
    ]
    .spacing(3)
    .align_y(Alignment::Center)
    .into()
}

fn status_badge<'a>(l: &LineRow) -> Element<'a, Message> {
    match l.status {
        LineStatus::Flagged => pill("Flagged", [0x15, 0x80, 0x3d]),
        LineStatus::Done => pill("Done in Xtime", [0x0e, 0x74, 0x90]),
        LineStatus::Todo => pill("To do", [0x1a, 0x5f, 0xb4]),
    }
}

fn line_row<'a>(app: &'a App, ro: &'a str, l: LineRow) -> Element<'a, Message> {
    // its hours: yours marked *, click to change
    let hours: Element<'a, Message> = match editing(app, ro, &l.key, LineField::Hours) {
        Some(e) => editor(e, "hours", 58.0),
        None => button(text(format!("{}{}", fmt_h(l.paid), if l.override_hours.is_some() { "*" } else { "" })).size(13))
            .padding([3, 8])
            .style(style::toggle(l.override_hours.is_some()))
            .on_press(Message::EditLine(ro.to_string(), l.key.clone(), LineField::Hours))
            .into(),
    };
    let asking = app.unflag_ask.as_ref().is_some_and(|(r, k)| r == ro && k == &l.key);
    let flag: Element<'a, Message> = match &l.flag {
        Some(f) => button(
            text(if asking {
                "Un-flag?"
            } else if f.confirmed {
                "✓ Confirmed"
            } else {
                "✓ Flagged"
            })
            .size(13),
        )
        .padding([3, 8])
        .style(if asking { button::danger } else { button::success })
        .on_press(Message::SidebarFlag(ro.to_string(), l.key.clone(), false))
        .into(),
        None => button(text("Flag").size(13))
            .padding([3, 10])
            .style(button::primary)
            .on_press(Message::SidebarFlag(ro.to_string(), l.key.clone(), true))
            .into(),
    };
    let mut actions = row![status_badge(&l), horizontal_space(), hours, flag]
        .spacing(4)
        .align_y(Alignment::Center);
    let mut c = column![
        row![
            container(
                text(if l.dms.is_empty() { "·".to_string() } else { l.dms.clone() })
                    .size(12)
                    .font(style::BOLD)
            )
            .width(22),
            column![
                text(if l.text.is_empty() {
                    "(no description)".to_string()
                } else {
                    l.text.clone()
                })
                .size(13)
            ]
            .push_maybe((!l.original.is_empty()).then(|| text(format!("Xtime: {}", l.original)).size(11).style(style::muted)))
            .width(Length::Fill),
        ]
        .spacing(4)
    ]
    .spacing(5);
    // when it was flagged: click to move it (an earlier pay period)
    if let Some(f) = &l.flag {
        let date: Element<'a, Message> = match editing(app, ro, &l.key, LineField::Date) {
            Some(e) => editor(e, "MM/DD/YYYY", 96.0),
            None => button(text(format!("Flagged {} ✎", short_date(f.flagged_at))).size(12))
                .padding([2, 6])
                .style(button::text)
                .on_press(Message::EditLine(ro.to_string(), l.key.clone(), LineField::Date))
                .into(),
        };
        c = c.push(actions);
        actions = row![horizontal_space(), date].align_y(Alignment::Center);
    }
    c = c.push(actions);
    container(c).padding([6, 8]).width(Length::Fill).style(style::card).into()
}

/// The vehicle's lines under it in the sidebar: "Lines · 2/5 flagged · pays 3.4 h" (folds them), then the lines
pub fn panel<'a>(app: &'a App, ro: &'a str, summary: Option<RoSummary>) -> Element<'a, Message> {
    let folded = app.folded_lines.contains(ro);
    let counted = summary.filter(|s| !s.lines.is_empty());
    let label = match &counted {
        Some(s) => format!(
            "{} Lines · {}/{} flagged · pays {}",
            if folded { "▸" } else { "▾" },
            s.flagged_count,
            s.lines.len(),
            fmt_h(s.total)
        ),
        None => format!("{} Lines", if folded { "▸" } else { "▾" }),
    };
    let mut c = Column::new().spacing(5).push(
        button(text(label).size(12))
            .width(Length::Fill)
            .padding([4, 8])
            .style(button::text)
            .on_press(Message::ToggleLines(ro.to_string())),
    );
    if folded {
        return c.into();
    }
    match counted {
        None => {
            c = c.push(
                text(if app.sample_data {
                    "Lines show once Xtime is connected."
                } else {
                    "No approved lines on this RO yet (they show once its Xtime View has loaded)."
                })
                .size(12)
                .style(style::muted),
            )
        }
        Some(s) => {
            let mut foot = format!("Job pays {} · flagged {}", fmt_h(s.total), fmt_h(s.flagged));
            if s.lines.iter().any(|l| l.override_hours.is_some()) {
                foot.push_str(" · * your hours");
            }
            for l in s.lines {
                c = c.push(line_row(app, ro, l));
            }
            c = c.push(text(foot).size(12).style(style::muted));
        }
    }
    c.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    #[test]
    fn reads_dates() {
        let at = |ms: i64| Local.timestamp_millis_opt(ms).unwrap();
        let t = at(parse_date("11/2/2026", 2026).unwrap());
        assert_eq!((t.year(), t.month(), t.day(), t.hour()), (2026, 11, 2, 12));
        let t = at(parse_date("3-15", 2025).unwrap());
        assert_eq!((t.year(), t.month(), t.day()), (2025, 3, 15));
        let t = at(parse_date(" 12/31/25 ", 2026).unwrap());
        assert_eq!((t.year(), t.month(), t.day()), (2025, 12, 31));
        assert_eq!(parse_date("2/30/2026", 2026), None);
        assert_eq!(parse_date("soon", 2026), None);
        // what the editor starts with reads back
        let ms = parse_date("01/09/2026", 2026).unwrap();
        assert_eq!(edit_date(ms), "01/09/2026");
    }
}
