//! The hours report (a card over everything, from the bottom bar): one pay period at a time. What's flagged, confirmed
//! and still to check, the time on the clock and your efficiency, the spiffs turned in; then every flagged line by
//! RO. Tick lines confirmed once they're on your time sheet, click hours to change them, take a flag off. Spiffs: of
//! the ROs worked on in the period that could be spiffed, how many were, and the ones that weren't.

use chrono::TimeZone;
use iced::widget::{Column, Row, Space, button, checkbox, column, container, horizontal_space, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

use super::{anim, pill, style};
use crate::app::{App, HoursView, Message};

/// The box a flag's hours are typed in
pub fn edit_id() -> text_input::Id {
    text_input::Id::new("flag-hours")
}

/// 1.5 → "1.5 h", 0.25 → "0.25 h", 2 → "2.0 h"
pub fn fmt_h(h: f64) -> String {
    let s = format!("{h:.2}");
    let s = s.trim_end_matches('0');
    let s = if s.ends_with('.') { format!("{s}0") } else { s.to_string() };
    format!("{s} h")
}

/// Milliseconds → "62h 15m"
fn hm(ms: i64) -> String {
    let mins = ms.max(0) / 60_000;
    format!("{}h {:02}m", mins / 60, mins % 60)
}

fn local(ms: i64, fmt: &str) -> String {
    chrono::Local
        .timestamp_millis_opt(ms)
        .earliest()
        .map(|t| t.format(fmt).to_string())
        .unwrap_or_default()
}

/// "Nov 16 – 30, 2026"
fn period_label(start: i64, end: i64) -> String {
    let last = end - 1;
    if local(start, "%Y%m") == local(last, "%Y%m") {
        format!("{} – {}", local(start, "%b %-d"), local(last, "%-d, %Y"))
    } else {
        format!("{} – {}", local(start, "%b %-d, %Y"), local(last, "%b %-d, %Y"))
    }
}

fn stat<'a>(label: &'a str, value: String, sub: String) -> Element<'a, Message> {
    container(
        column![
            text(label).size(12).style(style::muted),
            text(value).size(22).font(style::BOLD),
            text(sub).size(12).style(style::muted),
        ]
        .spacing(2),
    )
    .padding(12)
    .width(Length::Fill)
    .style(style::card)
    .into()
}

// the table's columns
const LETTER: f32 = 44.0;
const DATE: f32 = 80.0;
const HOURS: f32 = 90.0;
const CONFIRMED: f32 = 90.0;
const REMOVE: f32 = 84.0;

fn cell<'a>(e: impl Into<Element<'a, Message>>, w: f32) -> Element<'a, Message> {
    container(e).width(if w == 0.0 { Length::Fill } else { Length::Fixed(w) }).into()
}

pub fn card(app: &App) -> Element<'_, Message> {
    let hv = &app.hours_view;
    let now = app.now * 1000;
    let r = app.hours.report(&chrono::Local, now, hv.offset, xtime::ro::SPIFF_OVER);

    let head = row![
        text("Hours").size(22).font(style::BOLD),
        Space::with_width(16),
        button(text("<").size(15))
            .padding([2, 10])
            .style(style::ghost)
            .on_press(Message::HoursPeriod(hv.offset - 1)),
        text(format!("Pay period {}", period_label(r.period.start, r.period.end)))
            .size(15)
            .font(style::BOLD),
        button(text(">").size(15))
            .padding([2, 10])
            .style(style::ghost)
            .on_press_maybe((hv.offset < 0).then_some(Message::HoursPeriod(hv.offset + 1))),
        if r.is_current {
            Element::from(pill("This pay period", [0x15, 0x80, 0x3d]))
        } else {
            button(text("Back to this one").size(12))
                .padding([2, 8])
                .style(style::ghost)
                .on_press(Message::HoursPeriod(0))
                .into()
        },
        horizontal_space(),
        button(text("Close"))
            .padding([5, 12])
            .style(style::ghost)
            .on_press(Message::CloseHours),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let lines = r.flags.len();
    let eff = r
        .efficiency()
        .map(|e| format!("efficiency {e:.0}%"))
        .unwrap_or_else(|| "not clocked in yet".into());
    let stats = row![
        stat(
            "Flagged",
            fmt_h(r.total),
            format!("{lines} line{}", if lines == 1 { "" } else { "s" })
        ),
        stat("Confirmed", fmt_h(r.confirmed), "on your time sheet".into()),
        stat(
            "Not confirmed",
            fmt_h(r.unconfirmed),
            if r.unconfirmed > 0.0 {
                "still to check".into()
            } else {
                "all checked".into()
            }
        ),
        stat("Clocked", hm(r.worked), eff),
        stat("Spiffs", spiff_value(&r.spiffs), spiff_sub(&r.spiffs)),
    ]
    .spacing(10);

    let body = column![stats, flags_table(app, hv, &r), spiffs(&r, app.print_job.is_some())].spacing(18);
    let phase = hv.changed.map(anim::Phase::Entering).unwrap_or_default();
    container(scrollable(
        container(column![head, anim::page(body, phase).on_card()].spacing(16)).padding(24),
    ))
    .width(900)
    .style(style::floating_card)
    .into()
}

/// The flagged lines, by RO (each RO's lines under it, with its subtotal and a box to confirm them all)
fn flags_table<'a>(app: &'a App, hv: &'a HoursView, r: &hours::Report) -> Element<'a, Message> {
    if r.flags.is_empty() {
        return container(text("Nothing flagged in this pay period yet. Flag lines from an RO's Xtime View.").style(style::muted))
            .padding(14)
            .width(Length::Fill)
            .style(style::card)
            .into();
    }
    let th = |t: &'a str, w: f32| cell(text(t).size(12).font(style::BOLD).style(style::muted), w);
    let mut table = Column::new().spacing(2).push(
        row![
            th("Line", LETTER),
            th("Concern", 0.0),
            th("Flagged", DATE),
            th("Hours", HOURS),
            th("Confirmed", CONFIRMED),
            th("", REMOVE)
        ]
        .spacing(8)
        .padding([4, 10]),
    );

    // ROs in the order their first line was flagged
    let mut ros: Vec<&str> = Vec::new();
    for f in &r.flags {
        if !ros.contains(&f.ro.as_str()) {
            ros.push(&f.ro);
        }
    }
    for ro in ros {
        let mut flags: Vec<&hours::Flag> = r.flags.iter().filter(|f| f.ro == ro).collect();
        flags.sort_by(|a, b| {
            a.dms
                .cmp(&b.dms)
                .then(b.label.starts_with("Diag").cmp(&a.label.starts_with("Diag")))
        });
        let sub: f64 = flags.iter().map(|f| f.hours).sum();
        let all = flags.iter().all(|f| f.confirmed);
        let ids: Vec<u64> = flags.iter().map(|f| f.id).collect();
        let ymm = flags.iter().find(|f| !f.ymm.is_empty()).map(|f| f.ymm.clone()).unwrap_or_default();
        let mut title = Row::new()
            .spacing(8)
            .align_y(Alignment::Center)
            .push(text(format!("RO {ro}")).font(style::BOLD))
            .push(text(ymm).size(13).style(style::muted));
        if app.hours.spiffed(ro).is_some() {
            title = title.push(pill("Spiffed", [0x15, 0x80, 0x3d]));
        } else if r.spiffs.missed.iter().any(|(m, _, _)| m == ro) {
            title = title.push(pill("Spiff not turned in", [0xd9, 0x77, 0x06]));
        }
        let ro_row = row![
            cell(title, 0.0),
            cell(text(fmt_h(sub)).font(style::BOLD), HOURS),
            cell(
                checkbox("", all).on_toggle(move |on| Message::ConfirmFlags(ids.clone(), on)),
                CONFIRMED
            ),
            cell(Space::with_width(0), REMOVE),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding([8, 10]);
        let mut group = column![ro_row].spacing(0);
        for f in flags {
            group = group.push(flag_row(hv, f));
        }
        table = table.push(container(group).width(Length::Fill).style(style::card));
    }
    table = table.push(
        row![
            cell(text("Total").font(style::BOLD), 0.0),
            cell(text(fmt_h(r.total)).font(style::BOLD), HOURS),
            cell(
                text(format!("{} confirmed", fmt_h(r.confirmed))).size(12).style(style::muted),
                CONFIRMED + REMOVE + 8.0
            ),
        ]
        .spacing(8)
        .padding([8, 10]),
    );
    table.into()
}

/// One flagged line: its hours can be changed (click them), it can be confirmed, or taken off (press twice)
fn flag_row<'a>(hv: &'a HoursView, f: &hours::Flag) -> Element<'a, Message> {
    let id = f.id;
    let hours: Element<'a, Message> = match &hv.editing {
        Some((editing, typed)) if *editing == id => text_input("hours", typed)
            .id(edit_id())
            .on_input(Message::FlagHoursInput)
            .on_submit(Message::SaveFlagHours)
            .padding([3, 6])
            .size(13)
            .width(70)
            .into(),
        _ => button(text(fmt_h(f.hours)).size(13))
            .padding([3, 8])
            .style(style::ghost)
            .on_press(Message::EditFlagHours(id))
            .into(),
    };
    let remove: Element<'a, Message> = if hv.removing == Some(id) {
        button(text("Remove?").size(12).font(style::BOLD))
            .padding([3, 8])
            .style(button::danger)
            .on_press(Message::RemoveFlag(id))
            .into()
    } else {
        button(text("Remove").size(12))
            .padding([3, 8])
            .style(style::ghost)
            .on_press(Message::RemoveFlag(id))
            .into()
    };
    let label = if f.label.is_empty() {
        "(no description)".to_string()
    } else {
        f.label.clone()
    };
    row![
        cell(
            text(if f.dms.is_empty() { "—".to_string() } else { f.dms.clone() })
                .size(13)
                .style(style::muted),
            LETTER
        ),
        cell(text(label).size(13), 0.0),
        cell(text(local(f.flagged_at, "%b %-d")).size(13).style(style::muted), DATE),
        cell(hours, HOURS),
        cell(
            checkbox("", f.confirmed).on_toggle(move |on| Message::ConfirmFlags(vec![id], on)),
            CONFIRMED
        ),
        cell(remove, REMOVE),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .padding([4, 10])
    .into()
}

/// "2 of 3" spiffed (of the ROs worked on that could be)
fn spiff_value(c: &hours::SpiffCount) -> String {
    if c.eligible == 0 {
        "None".into()
    } else {
        format!("{} of {}", c.spiffed, c.eligible)
    }
}

fn spiff_sub(c: &hours::SpiffCount) -> String {
    let missed = c.eligible - c.spiffed;
    match (c.eligible, missed) {
        (0, _) => "no ROs over the spiff amount".into(),
        (_, 0) => "all turned in".into(),
        (_, 1) => "1 not turned in".into(),
        (_, n) => format!("{n} not turned in"),
    }
}

/// The ROs worked on this pay period that could have been spiffed but weren't (to turn in)
fn spiffs<'a>(r: &hours::Report, printing: bool) -> Element<'a, Message> {
    use std::ops::Not;
    let c = &r.spiffs;
    let mut list = column![].spacing(6);
    if !c.missed.is_empty() {
        list = list.push(text("Spiffs not turned in").size(16).font(style::BOLD));
        for (ro, ymm, total) in &c.missed {
            list = list.push(
                container(
                    row![
                        cell(text(format!("RO {ro}")).font(style::BOLD), 120.0),
                        cell(text(ymm.clone()).size(13).style(style::muted), 0.0),
                        cell(text(format!("Recommendations {}", crate::print::money(*total))).size(13), 200.0),
                        button(text("Print Spiff").size(12).font(style::BOLD))
                            .padding([4, 10])
                            .style(button::success)
                            .on_press_maybe(printing.not().then(|| Message::PrintSpiffFor(ro.clone()))),
                        button(text("Mark spiffed").size(12))
                            .padding([4, 10])
                            .style(style::ghost)
                            .on_press(Message::MarkSpiffed(ro.clone(), true)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .padding([8, 10])
                .width(Length::Fill)
                .style(style::card),
            );
        }
    }
    if c.unknown > 0 {
        let n = c.unknown;
        list = list.push(
            text(format!(
                "{n} RO{} from before the app kept recommendation totals {} counted for spiffs.",
                if n == 1 { "" } else { "s" },
                if n == 1 { "isn't" } else { "aren't" }
            ))
            .size(12)
            .style(style::muted),
        );
    }
    list.into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn writes_hours() {
        assert_eq!(super::fmt_h(1.5), "1.5 h");
        assert_eq!(super::fmt_h(0.25), "0.25 h");
        assert_eq!(super::fmt_h(2.0), "2.0 h");
        assert_eq!(super::hm(3_720_000), "1h 02m");
    }
}
