//! What you see of your hours (the data: crates/hours; the look: crates/ui-kit). The hours report (a card over
//! everything, from the bottom bar): one pay period at a time. What's flagged, confirmed
//! and still to check, the time on the clock and your efficiency, the spiffs turned in; then every flagged line by
//! RO. Tick lines confirmed once they're on your time sheet, click hours to change them, take a flag off. Spiffs: of
//! the ROs worked on in the period that could be spiffed, how many were, and the ones that weren't.

use chrono::TimeZone;
use hours::Hours;
use iced::Task;
use iced::widget::{Column, Row, Space, button, checkbox, column, container, horizontal_space, row, text, text_input};
// scrolling that glides (crates/ui-kit)
use iced::{Alignment, Element, Length};
use ui_kit::scroll::scrollable;

use ui_kit::{anim, pill, style};

mod pay;
pub use pay::{Field as PayField, PayForm, PayMsg, RateForm, StubField, rate_field_id};

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

fn stat<'a>(label: &'a str, value: String, sub: String) -> Element<'a, HoursMsg> {
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

fn cell<'a>(e: impl Into<Element<'a, HoursMsg>>, w: f32) -> Element<'a, HoursMsg> {
    container(e).width(if w == 0.0 { Length::Fill } else { Length::Fixed(w) }).into()
}

/// The report, as a card (the app shows it over everything). `pay`: Predictive Pay's setup (none: not set up);
/// `now`: milliseconds since 1970; `spiff_over`: what approved recommendations must come to for a spiff;
/// `can_print`: nothing's printing (Print Spiff can be pressed).
pub fn card<'a>(
    hv: &'a HoursView,
    h: &'a Hours,
    pay: Option<&'a hours::pay::PaySetup>,
    now: i64,
    spiff_over: f64,
    can_print: bool,
) -> Element<'a, HoursMsg> {
    // Predictive Pay's setup, in place of the report
    if let Some(form) = &hv.pay_form {
        return container(scrollable(container(pay::form(form)).padding(24)))
            .width(900)
            .style(style::floating_card)
            .into();
    }
    let r = h.report(&chrono::Local, now, hv.offset, spiff_over);

    let head = row![
        text("Hours").size(22).font(style::BOLD),
        Space::with_width(16),
        button(text("<").size(15))
            .padding([2, 10])
            .style(style::ghost)
            .on_press(HoursMsg::Period(hv.offset - 1)),
        text(format!("Pay period {}", period_label(r.period.start, r.period.end)))
            .size(15)
            .font(style::BOLD),
        button(text(">").size(15))
            .padding([2, 10])
            .style(style::ghost)
            .on_press_maybe((hv.offset < 0).then_some(HoursMsg::Period(hv.offset + 1))),
        if r.is_current {
            Element::from(pill("This pay period", [0x15, 0x80, 0x3d]))
        } else {
            button(text("Back to this one").size(12))
                .padding([2, 8])
                .style(style::ghost)
                .on_press(HoursMsg::Period(0))
                .into()
        },
        horizontal_space(),
        button(text("Close")).padding([5, 12]).style(style::ghost).on_press(HoursMsg::Close),
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

    let body = column![
        stats,
        pay::predicted(pay, r.total, r.period.start, hv.pay_open, hv.rate_form.as_ref()),
        flags_table(h, hv, &r),
        spiffs(&r, !can_print)
    ]
    .spacing(18);
    let phase = hv.changed.map(anim::Phase::Entering).unwrap_or_default();
    container(scrollable(
        container(column![head, anim::page(body, phase).on_card()].spacing(16)).padding(24),
    ))
    .width(900)
    .style(style::floating_card)
    .into()
}

/// The flagged lines, by RO (each RO's lines under it, with its subtotal and a box to confirm them all)
fn flags_table<'a>(h: &'a Hours, hv: &'a HoursView, r: &hours::Report) -> Element<'a, HoursMsg> {
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
        if h.spiffed(ro).is_some() {
            title = title.push(pill("Spiffed", [0x15, 0x80, 0x3d]));
        } else if r.spiffs.missed.iter().any(|(m, _, _)| m == ro) {
            title = title.push(pill("Spiff not turned in", [0xd9, 0x77, 0x06]));
        }
        let ro_row = row![
            cell(title, 0.0),
            cell(text(fmt_h(sub)).font(style::BOLD), HOURS),
            cell(checkbox("", all).on_toggle(move |on| HoursMsg::Confirm(ids.clone(), on)), CONFIRMED),
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
fn flag_row<'a>(hv: &'a HoursView, f: &hours::Flag) -> Element<'a, HoursMsg> {
    let id = f.id;
    let hours: Element<'a, HoursMsg> = match &hv.editing {
        Some((editing, typed)) if *editing == id => text_input("hours", typed)
            .id(edit_id())
            .on_input(HoursMsg::HoursInput)
            .on_submit(HoursMsg::SaveHours)
            .padding([3, 6])
            .size(13)
            .width(70)
            .into(),
        _ => button(text(fmt_h(f.hours)).size(13))
            .padding([3, 8])
            .style(style::ghost)
            .on_press(HoursMsg::EditHours(id))
            .into(),
    };
    let remove: Element<'a, HoursMsg> = if hv.removing == Some(id) {
        button(text("Remove?").size(12).font(style::BOLD))
            .padding([3, 8])
            .style(button::danger)
            .on_press(HoursMsg::Remove(id))
            .into()
    } else {
        button(text("Remove").size(12))
            .padding([3, 8])
            .style(style::ghost)
            .on_press(HoursMsg::Remove(id))
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
            checkbox("", f.confirmed).on_toggle(move |on| HoursMsg::Confirm(vec![id], on)),
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
fn spiffs<'a>(r: &hours::Report, printing: bool) -> Element<'a, HoursMsg> {
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
                        cell(text(format!("Recommendations {}", money(*total))).size(13), 200.0),
                        button(text("Print Spiff").size(12).font(style::BOLD))
                            .padding([4, 10])
                            .style(button::success)
                            .on_press_maybe(printing.not().then(|| HoursMsg::PrintSpiff(ro.clone()))),
                        button(text("Mark spiffed").size(12))
                            .padding([4, 10])
                            .style(style::ghost)
                            .on_press(HoursMsg::MarkSpiffed(ro.clone())),
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

// ---------- the report's state and what changes it ----------

/// The report's state
#[derive(Debug, Clone, Default)]
pub struct HoursView {
    /// Which pay period: 0 this one, -1 the one before…
    pub offset: i32,
    /// A flag whose hours are being changed, and what's typed
    pub editing: Option<(u64, String)>,
    /// A flag asked to be taken off (pressed once)
    pub removing: Option<u64>,
    /// When the pay period changed (it slides in)
    pub changed: Option<std::time::Instant>,
    /// Predictive Pay's details are showing
    pub pay_open: bool,
    /// Predictive Pay being set up (the form shows in place of the report)
    pub pay_form: Option<PayForm>,
    /// A new hourly rate being typed
    pub rate_form: Option<RateForm>,
}

/// What the report does
#[derive(Debug, Clone)]
pub enum HoursMsg {
    /// Which pay period (0 this one, -1 the one before…)
    Period(i32),
    /// Tick flags confirmed (on your time sheet), or not
    Confirm(Vec<u64>, bool),
    /// Change a flag's hours: start, what's typed, keep it; or stop changing anything
    EditHours(u64),
    HoursInput(String),
    SaveHours,
    CancelEdit,
    /// Take a flag off (asks first: press twice)
    Remove(u64),
    /// For the app: print an RO's spiff, mark it spiffed, close the report
    PrintSpiff(String),
    MarkSpiffed(String),
    Close,
    /// Predictive Pay: its details, and setting it up
    Pay(PayMsg),
}

/// What the app does after an update
#[derive(Debug, Clone, PartialEq)]
pub enum HoursOut {
    /// Your hours changed: save them
    Changed,
    /// A message for the bottom bar
    Notice(String),
    PrintSpiff(String),
    MarkSpiffed(String),
    Close,
    /// Predictive Pay set up: keep it (and send it to your other computers and phone). Its `at` is left to the app.
    PaySaved(hours::pay::PaySetup),
}

impl HoursView {
    /// The report, starting on this pay period
    pub fn new() -> Self {
        Self::default()
    }

    /// `pay`: Predictive Pay's setup as it is (the form starts from it)
    pub fn update(&mut self, h: &mut Hours, pay: Option<&hours::pay::PaySetup>, msg: HoursMsg) -> (Task<HoursMsg>, Vec<HoursOut>) {
        let out = match msg {
            HoursMsg::Period(offset) => {
                *self = HoursView {
                    offset: offset.min(0),
                    changed: Some(std::time::Instant::now()),
                    pay_open: self.pay_open,
                    ..HoursView::default()
                };
                vec![]
            }
            HoursMsg::Pay(PayMsg::Edit) => {
                self.pay_form = Some(PayForm::from_setup(pay));
                self.editing = None;
                self.removing = None;
                vec![]
            }
            HoursMsg::Pay(PayMsg::Cancel) => {
                self.pay_form = None;
                self.changed = Some(std::time::Instant::now());
                vec![]
            }
            HoursMsg::Pay(PayMsg::Details) => {
                self.pay_open = !self.pay_open;
                vec![]
            }
            HoursMsg::Pay(PayMsg::NewRate) => {
                self.rate_form = if self.rate_form.is_some() {
                    None
                } else {
                    Some(RateForm::default())
                };
                let focus = if self.rate_form.is_some() {
                    text_input::focus(rate_field_id())
                } else {
                    Task::none()
                };
                return (focus, vec![]);
            }
            HoursMsg::Pay(PayMsg::RateText(v)) => {
                if let Some(f) = &mut self.rate_form {
                    f.text = v;
                    f.error = None;
                }
                vec![]
            }
            HoursMsg::Pay(PayMsg::RateNext(next)) => {
                if let Some(f) = &mut self.rate_form {
                    f.next = next;
                }
                vec![]
            }
            HoursMsg::Pay(PayMsg::RateCancel) => {
                self.rate_form = None;
                vec![]
            }
            HoursMsg::Pay(PayMsg::RateSave) => {
                let (Some(f), Some(p)) = (&mut self.rate_form, pay) else {
                    return (Task::none(), vec![]);
                };
                match hours::pay::parse_amount(&f.text) {
                    Ok(Some(rate)) if rate > 0.0 && rate < 1000.0 => {
                        // from this pay period (the one it is now, whichever's shown), or the next
                        let now = chrono::Local::now().timestamp_millis();
                        let from = hours::pay_period(&chrono::Local, now, if f.next { 1 } else { 0 }).start;
                        let mut p = p.clone();
                        p.set_rate(from, rate);
                        self.rate_form = None;
                        self.pay_open = true;
                        vec![HoursOut::PaySaved(p)]
                    }
                    _ => {
                        f.error = Some("Type the new rate an hour, like 27.50".into());
                        vec![]
                    }
                }
            }
            HoursMsg::Pay(PayMsg::Save) => match self.pay_form.as_mut().map(|f| f.to_setup()) {
                Some(Ok(mut setup)) => {
                    // (the new rates since the stubs stay)
                    setup.rates = pay.map(|p| p.rates.clone()).unwrap_or_default();
                    self.pay_form = None;
                    self.pay_open = true;
                    self.changed = Some(std::time::Instant::now());
                    vec![HoursOut::PaySaved(setup)]
                }
                Some(Err(e)) => {
                    if let Some(f) = &mut self.pay_form {
                        f.error = Some(e);
                    }
                    vec![]
                }
                None => vec![],
            },
            HoursMsg::Pay(m) => {
                if let Some(f) = &mut self.pay_form {
                    f.update(m);
                }
                vec![]
            }
            HoursMsg::Confirm(ids, on) => {
                for id in ids {
                    h.set_confirmed(id, on);
                }
                vec![HoursOut::Changed]
            }
            HoursMsg::EditHours(id) => {
                let now = h.flags.iter().find(|f| f.id == id).map(|f| f.hours).unwrap_or(0.0);
                self.editing = Some((id, format!("{now}")));
                self.removing = None;
                return (text_input::focus(edit_id()), vec![]);
            }
            HoursMsg::HoursInput(text) => {
                if let Some((_, t)) = &mut self.editing {
                    *t = text;
                }
                vec![]
            }
            HoursMsg::SaveHours => match self.editing.take() {
                Some((id, text)) => match text.trim().parse::<f64>() {
                    Ok(v) if v.is_finite() && (0.0..100.0).contains(&v) => {
                        h.set_flag_hours(id, v);
                        vec![HoursOut::Changed]
                    }
                    _ => vec![HoursOut::Notice(format!("\"{}\" isn't a number of hours", text.trim()))],
                },
                None => vec![],
            },
            HoursMsg::CancelEdit => {
                self.editing = None;
                self.removing = None;
                vec![]
            }
            HoursMsg::Remove(id) => {
                // twice: the first press asks
                if self.removing == Some(id) {
                    h.delete_flag(id);
                    self.removing = None;
                    vec![HoursOut::Changed]
                } else {
                    self.removing = Some(id);
                    self.editing = None;
                    vec![]
                }
            }
            HoursMsg::PrintSpiff(ro) => vec![HoursOut::PrintSpiff(ro)],
            HoursMsg::MarkSpiffed(ro) => vec![HoursOut::MarkSpiffed(ro)],
            HoursMsg::Close => vec![HoursOut::Close],
        };
        (Task::none(), out)
    }
}

/// $1,234.50
fn money(n: f64) -> String {
    hours::pay::money(n)
}

#[cfg(test)]
mod tests {
    #[test]
    fn writes_hours() {
        assert_eq!(super::fmt_h(1.5), "1.5 h");
        assert_eq!(super::fmt_h(0.25), "0.25 h");
        assert_eq!(super::fmt_h(2.0), "2.0 h");
        assert_eq!(super::hm(3_720_000), "1h 02m");
        assert_eq!(super::money(1234.5), "$1,234.50");
    }
}
