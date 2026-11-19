//! Xtime View: the app's own page for an RO in Xtime (the Electron app's xtime-view/ro-view.js). The header (stage,
//! waiter / express…), Service Details, Vehicle Info, then the lines: customer concerns with the recommendations added
//! under them, the other recommendations, and the inspection lines (hidden until asked for). Each line shows its
//! approval, labor type, hours and price, and a Done box that changes it in Xtime; click a line for its concern, cause,
//! correction, notes and parts. Read from Xtime every minute while it's showing. No customer details are read.
//!
//! Each line has a handle on its left: drag it to reorder the lines, or to put a recommendation under a customer
//! concern (or back out on its own), as on Xtime's RO page. Where it'll land opens up under the cursor.

use std::collections::HashMap;

use iced::alignment::Horizontal;
use iced::widget::{Column, Row, Space, button, checkbox, column, container, horizontal_space, mouse_area, row, text};
use iced::{Alignment, Color, Element, Length, mouse};
use xtime::ro::{self, Line, Place, RoView, Sections};

use ui_kit::{anim, pill, style};

use crate::ro_view::{Drag, Target};
use crate::{Ask, LoadError, Msg, XtimeView};

/// What the Xtime View shows from the rest of the app
pub struct Shown {
    /// The time (seconds since 1970), for "Updated 2 min ago"
    pub now: i64,
    /// By Xtime's line ID: the lines that count toward your hours, and the hours flagged when they're flagged
    pub flags: HashMap<String, Option<f64>>,
    /// The RO's spiff, when it's been turned in: what it came to
    pub spiffed: Option<f64>,
    /// Nothing's printing (Print Spiff can be pressed)
    pub can_print: bool,
    /// No Add / Edit Service window is up
    pub can_add_service: bool,
}

fn money(n: Option<f64>) -> String {
    n.map(|n| format!("${n:.2}")).unwrap_or_default()
}

fn hours(n: Option<f64>) -> String {
    n.map(|n| format!("{n:.1} h")).unwrap_or_default()
}

/// Room for the Flag button (kept on lines without one, so the prices line up)
const FLAG_WIDTH: f32 = 104.0;

/// Room for the Done box (kept on lines without one, so the prices line up)
const DONE_WIDTH: f32 = 64.0;

const GREEN: [u8; 3] = [0x15, 0x80, 0x3d];
const RED: [u8; 3] = [0xb4, 0x23, 0x18];
const AMBER: [u8; 3] = [0xd9, 0x77, 0x06];
const GREY: [u8; 3] = [0x5d, 0x66, 0x70];
const BLUE: [u8; 3] = [0x1a, 0x5f, 0xb4];

fn color(c: [u8; 3]) -> Color {
    Color::from_rgb8(c[0], c[1], c[2])
}

/// What a line card needs besides the line: the tab's state, and the line's flag (your hours)
struct Ctx<'a, 'b> {
    xv: &'a XtimeView,
    flags: &'b HashMap<String, Option<f64>>,
}

pub fn view<'a>(xv: &'a XtimeView, shown: &Shown) -> Element<'a, Msg> {
    let Some(v) = &xv.view else {
        return first_read(xv);
    };
    let cx = Ctx { xv, flags: &shown.flags };
    let sections = v.sections();
    // Add Service: Xtime's Add Service window; then the moves Xtime offers you (Send to Approval, Send to Review…)
    let mut toolbar = row![
        button(text("Add Service").size(13).font(style::BOLD))
            .padding([6, 14])
            .style(button::primary)
            .on_press_maybe(shown.can_add_service.then_some(Msg::Ask(Ask::AddService(None)))),
        button(text("Message about this RO").size(13))
            .padding([6, 14])
            .style(style::ghost)
            .on_press(Msg::Ask(Ask::Chat)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    toolbar = toolbar.push(horizontal_space());
    if let Some(c) = &xv.stage {
        for &mv in &c.moves {
            let busy = xv.moving == Some(mv);
            toolbar = toolbar.push(
                button(
                    text(if busy { "Sending…" } else { mv.label(c.parts_enabled) })
                        .size(13)
                        .font(style::BOLD),
                )
                .padding([6, 14])
                .style(button::success)
                .on_press_maybe((xv.moving.is_none() && xv.rating.is_none()).then_some(Msg::AskMove(Some(mv)))),
            );
        }
        if c.mode == 3 && c.yours && c.required_missing > 0 && c.required_missing != usize::MAX {
            toolbar = toolbar.push(
                text(format!(
                    "{} Required inspection item{} to fill in before it can go on",
                    c.required_missing,
                    if c.required_missing == 1 { "" } else { "s" }
                ))
                .size(12)
                .style(style::muted),
            );
        }
    }
    let mut page = column![header(xv, shown, v), top_cards(v), toolbar.wrap()].spacing(12);
    if let (Some(mv), Some(c)) = (xv.confirm, &xv.stage) {
        page = page.push(confirm_card(&v.ro, mv, c.parts_enabled, c.send_to_parts));
    }
    if let Some(r) = &xv.rating {
        page = page.push(rating_card(xv, r));
    }
    if let Some(e) = &xv.error {
        page = page.push(
            text(format!("Couldn't refresh: {}", e.text().trim_start_matches("Xtime: ")))
                .size(13)
                .style(style::muted),
        );
    }
    let mut lists = column![].spacing(12);
    if let Some(d) = &xv.drag {
        lists = lists.push(drag_hint(d));
    }
    lists = lists.push(section(
        &cx,
        "Customer Initial Concerns",
        Place::Concerns,
        sections.concerns.iter().map(|(c, kids)| (*c, kids.clone())).collect(),
        v,
    ));
    lists = lists.push(section(
        &cx,
        "Additional Service Recommendations",
        Place::Recommendations,
        sections.loose.iter().map(|&i| (i, vec![])).collect(),
        v,
    ));
    // dragging: off the lists, it goes back where it was if let go
    page = page.push(match &xv.drag {
        Some(_) => Element::from(mouse_area(lists).on_exit(Msg::DragOff).interaction(mouse::Interaction::Grabbing)),
        None => lists.into(),
    });
    if v.lines.iter().all(Line::is_inspection) {
        page = page.push(empty_card("No lines on this RO yet."));
    }
    page = page.push(inspection(&cx, v, &sections));
    page = page.push(crate::inspection::section(xv));
    page.into()
}

/// "Send RO 123 to Approval?" (a stage change goes to the advisor and parts: asked once more)
fn confirm_card(ro: &str, mv: xtime::stage::Move, parts_enabled: bool, to_parts: bool) -> Element<'_, Msg> {
    use xtime::stage::Move;
    let about = match mv {
        Move::CompleteToApproval { .. } | Move::BackToApproval if parts_enabled && to_parts => {
            "It goes to the advisor for approval, and to parts for a quote (the parts status becomes Waiting)."
        }
        Move::CompleteToApproval { .. } | Move::BackToApproval => "It goes to the advisor for approval.",
        Move::CompleteToRepair => "The inspection's done: you work on the primary lines.",
        Move::CompleteToReview | Move::ToReview => "All the work's done: it goes to the advisor for review.",
        Move::StartWorking => "It's on hold no more: you're working on it.",
    };
    let question = match mv {
        Move::StartWorking => format!("Start working on RO {ro}?"),
        Move::CompleteToRepair => format!("Complete the inspection on RO {ro}?"),
        _ => format!("{} for RO {ro}?", mv.label(parts_enabled)),
    };
    container(
        column![
            text(question).size(15).font(style::BOLD),
            text(about).size(13).style(style::muted),
            row![
                horizontal_space(),
                button(text("Cancel").size(13))
                    .padding([6, 14])
                    .style(style::ghost)
                    .on_press(Msg::AskMove(None)),
                button(text("Yes, send it").size(13).font(style::BOLD))
                    .padding([6, 14])
                    .style(button::success)
                    .on_press(Msg::MakeMove(mv)),
            ]
            .spacing(8),
        ]
        .spacing(8),
    )
    .padding(14)
    .width(Length::Fill)
    .style(style::card_highlight)
    .into()
}

/// The store's "Repair Order Quality": the advisor rated before the RO goes on (as Xtime's page asks)
fn rating_card<'a>(xv: &'a XtimeView, r: &'a crate::ro_view::RatingState) -> Element<'a, Msg> {
    let mut stars = row![].spacing(4);
    for n in 1..=5u8 {
        let on = n <= r.stars;
        stars = stars.push(
            button(
                text(if on { "★" } else { "☆" })
                    .size(26)
                    .color(if on { color(AMBER) } else { color(GREY) }),
            )
            .padding([0, 2])
            .style(button::text)
            .on_press(Msg::RateStars(n)),
        );
    }
    let mut c = column![
        text(r.ask.title.as_str()).size(15).font(style::BOLD),
        text("Your store asks you to rate the advisor on this RO before it goes on.")
            .size(13)
            .style(style::muted),
        stars,
    ]
    .spacing(8);
    let low = r.stars > 0 && r.stars <= 3;
    if low && r.ask.drop_down && !r.ask.options.is_empty() {
        let mut opts = Column::new().spacing(4);
        for o in &r.ask.options {
            let picked = r.option.as_deref() == Some(o.as_str());
            opts = opts.push(
                button(text(o.as_str()).size(13))
                    .width(Length::Fill)
                    .padding([5, 10])
                    .style(style::toggle(picked))
                    .on_press(Msg::RateOption(o.clone())),
            );
        }
        c = c.push(text("Why?").size(13).font(style::BOLD)).push(opts);
    }
    let typed = low && ((r.ask.drop_down && r.option.as_deref() == Some("Other")) || (!r.ask.drop_down && r.ask.text_field));
    if typed {
        c = c.push(
            iced::widget::text_input("Why? (a few words)", &r.text)
                .on_input(Msg::RateText)
                .padding(8)
                .size(13),
        );
    }
    let sending = xv.moving.is_some();
    c = c.push(
        row![
            horizontal_space(),
            button(text("Cancel").size(13))
                .padding([6, 14])
                .style(style::ghost)
                .on_press(Msg::CancelRating),
            button(text(if sending { "Sending…" } else { "Rate and send" }).size(13).font(style::BOLD))
                .padding([6, 14])
                .style(button::success)
                .on_press_maybe((r.ready() && !sending).then_some(Msg::SendRating)),
        ]
        .spacing(8),
    );
    container(c).padding(14).width(Length::Fill).style(style::card_highlight).into()
}

/// Before the RO has been read: reading, or why it couldn't be
fn first_read(xv: &XtimeView) -> Element<'_, Msg> {
    let body: Element<'_, Msg> = match &xv.error {
        None => text("Reading this RO from Xtime…").style(style::muted).into(),
        Some(e) => {
            let (title, about) = match e {
                LoadError::SignedOut => (
                    "Sign in to Xtime",
                    "Xtime needs you to sign in again (in the bottom bar or Settings).".to_string(),
                ),
                other => ("Couldn't read this RO", other.text()),
            };
            column![
                text(title).size(16).font(style::BOLD),
                text(about).style(style::muted),
                button(text("Try again").font(style::BOLD))
                    .padding([6, 14])
                    .style(button::primary)
                    .on_press_maybe((!xv.loading).then_some(Msg::Load(true))),
            ]
            .spacing(8)
            .into()
        }
    };
    container(body).padding(16).width(Length::Fill).style(style::card).into()
}

fn header<'a>(xv: &'a XtimeView, shown: &Shown, v: &'a RoView) -> Element<'a, Msg> {
    let title = if v.tag.is_empty() {
        format!("RO {}", v.ro)
    } else {
        format!("RO {} · Tag {}", v.ro, v.tag)
    };
    let ymm = [v.vehicle.year.as_str(), v.vehicle.make.as_str(), v.vehicle.model.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    let mut sub = row![pill(ro::mode_name(v.mode), BLUE)].spacing(6).align_y(Alignment::Center);
    let mut since = String::new();
    if !v.mode_since.is_empty() {
        since = format!("since {}", ro::when(&v.mode_since));
    }
    if !ymm.is_empty() {
        since = if since.is_empty() { ymm } else { format!("{since} · {ymm}") };
    }
    if !since.is_empty() {
        sub = sub.push(text(since).size(13).style(style::muted));
    }
    for (on, label, c) in [
        (v.waiter, "Waiter", AMBER),
        (v.express, "Express", BLUE),
        (v.internal, "Internal", GREY),
        (v.warranty, "Warranty", [0x7c, 0x3a, 0xed]),
        (v.cpo, "CPO", GREY),
    ] {
        if on {
            sub = sub.push(pill(label, c));
        }
    }
    let updated = match (xv.loading, xv.at) {
        (true, _) => "Reading…".to_string(),
        (false, Some(at)) => format!("Updated {}", crate::ago(shown.now - at)),
        (false, None) => String::new(),
    };
    // the spiff: approved recommendations over $150 can be printed and turned in
    let total = v.spiff_total();
    let spiff: Element<'a, Msg> = if let Some(done) = shown.spiffed {
        // turned in already: say so (with what it came to then); it can be printed again, or unmarked
        row![
            pill(format!("Spiffed · {}", money(Some(done))), GREEN),
            button(text("Print again").size(13))
                .padding([5, 10])
                .style(style::ghost)
                .on_press_maybe((v.spiff_earned() && shown.can_print).then_some(Msg::Ask(Ask::PrintSpiff))),
            button(text("Unmark").size(13))
                .padding([5, 10])
                .style(style::ghost)
                .on_press(Msg::Ask(Ask::MarkSpiffed(false))),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    } else if v.spiff_earned() {
        row![
            text(format!("Spiff {}", money(Some(total))))
                .size(13)
                .font(style::BOLD)
                .style(text::success),
            button(text("Print Spiff").size(13).font(style::BOLD))
                .padding([5, 12])
                .style(button::success)
                .on_press_maybe(shown.can_print.then_some(Msg::Ask(Ask::PrintSpiff))),
            button(text("Mark spiffed").size(13))
                .padding([5, 10])
                .style(style::ghost)
                .on_press(Msg::Ask(Ask::MarkSpiffed(true))),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    } else if total > 0.0 {
        text(format!(
            "Recommendations {} · spiff over {}",
            money(Some(total)),
            money(Some(ro::SPIFF_OVER))
        ))
        .size(12)
        .style(style::muted)
        .into()
    } else {
        Space::with_width(0).into()
    };
    container(
        row![
            column![text(title).size(20).font(style::BOLD), sub].spacing(6),
            horizontal_space(),
            spiff,
            text(updated).size(12).style(style::muted),
            button(text("Refresh").size(13))
                .padding([5, 12])
                .style(style::ghost)
                .on_press_maybe((!xv.loading).then_some(Msg::Load(true))),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding(14)
    .width(Length::Fill)
    .style(style::card)
    .into()
}

/// One label / value line of a card; `copy`: click the value to copy it (named for the notice)
fn fact<'a>(label: &'a str, value: String, copy: Option<&'a str>) -> Element<'a, Msg> {
    let shown: Element<'a, Msg> = match copy {
        Some(what) if !value.is_empty() => button(text(value.clone()).font(style::MONO).size(13))
            .padding([1, 6])
            .style(style::ghost)
            .on_press(Msg::Ask(Ask::Copy(value, what.to_string())))
            .into(),
        _ if value.is_empty() => text("—").style(style::muted).into(),
        _ => text(value).into(),
    };
    row![text(label).width(130).style(style::muted), shown]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
}

fn card_of<'a>(title: &'a str, rows: Vec<Element<'a, Msg>>) -> Element<'a, Msg> {
    container(column![text(title).size(16).font(style::BOLD), Column::with_children(rows).spacing(6)].spacing(10))
        .padding(14)
        .width(Length::Fill)
        .style(style::card)
        .into()
}

/// Service Details and Vehicle Info, side by side
fn top_cards(v: &RoView) -> Element<'_, Msg> {
    let (p_date, p_time) = ro::promised(&v.promised_date, &v.promised_time);
    let mut flags = Row::new().spacing(4);
    let mut any = false;
    for (on, label, c) in [
        (v.waiting_parts, "PA · Parts", RED),
        (v.waiting_customer, "CA · Customer Approval", AMBER),
        (v.waiting_warranty, "WA · Warranty Approval", [0x7c, 0x3a, 0xed]),
        (v.on_hold, "OH · On Hold", GREY),
    ] {
        if on {
            flags = flags.push(pill(label, c));
            any = true;
        }
    }
    for f in &v.flags {
        flags = flags.push(pill(f.as_str(), GREY));
        any = true;
    }
    let flags_row: Element<'_, Msg> = row![
        text("Flags").width(130).style(style::muted),
        if any {
            flags.into()
        } else {
            Element::from(text("None").style(style::muted))
        }
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into();
    let ymm = [v.vehicle.year.as_str(), v.vehicle.make.as_str(), v.vehicle.model.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    let mileage = v.vehicle.mileage.map(thousands).unwrap_or_default();
    row![
        card_of(
            "Service Details",
            vec![
                fact("Tag Number", v.tag.clone(), Some("Tag")),
                fact("Advisor", v.advisor.label(), None),
                fact("Technician", v.technician.label(), None),
                fact("Parts Employee", v.parts_employee.clone(), None),
                fact("Promise Date", p_date, None),
                fact("Promise Time", p_time, None),
                flags_row,
            ]
        ),
        card_of(
            "Vehicle Info",
            vec![
                fact("VIN", v.vehicle.vin.clone(), Some("VIN")),
                fact("Vehicle", ymm, None),
                fact("Mileage", mileage, None),
            ]
        ),
    ]
    .spacing(12)
    .into()
}

/// 123456 → "123,456"
fn thousands(n: f64) -> String {
    let s = format!("{}", n.round() as i64);
    let (sign, digits) = s.strip_prefix('-').map(|d| ("-", d)).unwrap_or(("", &s));
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    format!("{sign}{out}")
}

fn empty_card<'a>(msg: &'a str) -> Element<'a, Msg> {
    container(text(msg).style(style::muted))
        .padding(14)
        .width(Length::Fill)
        .style(style::card)
        .into()
}

/// A section's title, its count, and what's approved in it (recommendations only)
fn section_head<'a>(title: &'a str, count: usize, approved: &[&Line]) -> Row<'a, Msg> {
    let mut h = row![text(title).size(16).font(style::BOLD), pill(count.to_string(), GREY)]
        .spacing(8)
        .align_y(Alignment::Center);
    if !approved.is_empty() {
        let hrs: f64 = approved.iter().filter_map(|l| l.hours).sum();
        let total: f64 = approved.iter().filter_map(|l| l.total).sum();
        h = h.push(
            text(format!(
                "{} approved · {} · {}",
                approved.len(),
                hours(Some(hrs)),
                money(Some(total))
            ))
            .size(13)
            .style(style::muted),
        );
    }
    h
}

/// Lines with the recommendations under each (indexes into the RO's lines); `place`: which of Xtime's lists it is
fn section<'a>(cx: &Ctx<'a, '_>, title: &'a str, place: Place, groups: Vec<(usize, Vec<usize>)>, v: &'a RoView) -> Element<'a, Msg> {
    let dragging_here = cx.xv.drag.as_ref().is_some_and(|d| d.fits(&place));
    if groups.is_empty() && !dragging_here {
        return Space::with_height(0).into();
    }
    let recs: Vec<&Line> = groups
        .iter()
        .flat_map(|(i, kids)| std::iter::once(*i).chain(kids.iter().copied()))
        .map(|i| &v.lines[i])
        .filter(|l| !l.primary && l.approved && !l.declined)
        .collect();
    let mut list = column![section_head(title, groups.len(), &recs)].spacing(8);
    for (i, kids) in groups {
        let l = &v.lines[i];
        if let Some(m) = marker(cx.xv, &place, Some(l)) {
            list = list.push(m);
        }
        list = list.push(line_card(cx, l, &place, kids.iter().map(|&k| &v.lines[k]).collect()));
    }
    // the end of the list
    if dragging_here {
        let label = match place {
            Place::Concerns => "Move it to the end of the concerns",
            _ => "Move it here, on its own",
        };
        list = list.push(slot(cx.xv, Target { place, before: None }, label.to_string()));
    }
    list.into()
}

fn inspection<'a>(cx: &Ctx<'a, '_>, v: &'a RoView, s: &Sections) -> Element<'a, Msg> {
    let xv = cx.xv;
    if s.inspection.is_empty() {
        return Space::with_height(0).into();
    }
    let head = section_head("Inspection lines", s.inspection.len(), &[])
        .push(horizontal_space())
        .push(
            button(text(if xv.show_inspection { "Hide" } else { "Show" }).size(13))
                .padding([4, 12])
                .style(style::ghost)
                .on_press(Msg::ToggleInspection),
        );
    let mut list = column![head].spacing(8);
    if xv.show_inspection {
        for &i in &s.inspection {
            list = list.push(line_card(cx, &v.lines[i], &Place::Recommendations, vec![]));
        }
    } else {
        list = list.push(
            text("Multipoint inspection and green tires / battery / brakes.")
                .size(13)
                .style(style::muted),
        );
    }
    list.into()
}

/// What's shown while a line's being dragged: what to do
fn drag_hint<'a>(d: &Drag) -> Element<'a, Msg> {
    let tip = if d.primary {
        "Drag between the concerns to change their order. Let go to move it, or press Esc to put it back."
    } else {
        "Drag it onto a concern to put it under that concern, or between lines to change the order. Esc puts it back."
    };
    anim::page(
        container(text(tip).size(13))
            .padding([8, 14])
            .width(Length::Fill)
            .style(|t: &iced::Theme| {
                let accent = style::colors(t).accent;
                container::Style {
                    background: Some(Color { a: 0.10, ..accent }.into()),
                    border: iced::Border {
                        color: Color { a: 0.4, ..accent },
                        width: 1.0,
                        radius: 10.0.into(),
                    },
                    ..Default::default()
                }
            }),
        anim::Phase::Entering(d.since),
    )
    .into()
}

/// Where the dragged line would land: it opens up (springing in) with what letting go does
fn landing<'a>(d: &Drag, label: String) -> Element<'a, Msg> {
    anim::page(
        container(text(label).size(13).font(style::BOLD))
            .padding([10, 14])
            .width(Length::Fill)
            .center_x(Length::Fill)
            .style(|t: &iced::Theme| {
                let accent = style::colors(t).accent;
                container::Style {
                    text_color: Some(accent),
                    background: Some(Color { a: 0.12, ..accent }.into()),
                    border: iced::Border {
                        color: accent,
                        width: 2.0,
                        radius: 10.0.into(),
                    },
                    ..Default::default()
                }
            }),
        anim::Phase::Entering(d.target_since),
    )
    .into()
}

/// The landing spot shown before `l` (or at the end: none), when that's where the dragged line would land
fn marker<'a>(xv: &XtimeView, place: &Place, l: Option<&Line>) -> Option<Element<'a, Msg>> {
    let d = xv.drag.as_ref()?;
    let t = d.target.as_ref()?;
    let here = &t.place == place && t.before.as_deref() == l.map(|l| l.key()).as_deref();
    here.then(|| {
        mouse_area(landing(d, "Move it here".into()))
            .on_enter(Msg::DragOver(t.clone()))
            .into()
    })
}

/// A place to drop at the end of a list (while dragging a line that can go there): a thin strip that opens up when
/// the line's over it
fn slot<'a>(xv: &XtimeView, t: Target, label: String) -> Element<'a, Msg> {
    let Some(d) = &xv.drag else { return Space::with_height(0).into() };
    let active = d.target.as_ref() == Some(&t);
    let inner: Element<'a, Msg> = if active {
        landing(d, label)
    } else {
        container(text(label).size(12).style(style::muted))
            .padding([6, 14])
            .width(Length::Fill)
            .center_x(Length::Fill)
            .style(|th: &iced::Theme| container::Style {
                border: iced::Border {
                    color: style::colors(th).line,
                    width: 1.0,
                    radius: 10.0.into(),
                },
                ..Default::default()
            })
            .into()
    };
    mouse_area(inner).on_enter(Msg::DragOver(t)).into()
}

/// The handle a line's dragged by: three short bars (drawn, so it shows in every font)
fn grip<'a>(key: String, on: bool) -> Element<'a, Msg> {
    let bar = || {
        container(Space::new(12, 2)).style(move |t: &iced::Theme| container::Style {
            background: Some(style::colors(t).muted.into()),
            border: iced::Border {
                radius: 1.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
    };
    let g = container(column![bar(), bar(), bar()].spacing(3)).padding([10, 6]);
    if !on {
        return g.into();
    }
    mouse_area(g)
        .on_press(Msg::DragStart(key))
        .interaction(mouse::Interaction::Grab)
        .into()
}

/// A line: its head (click to open), what's under it when open, and the recommendations added under it. `place`:
/// which of Xtime's lists it's in (where dropping on it puts a dragged line).
fn line_card<'a>(cx: &Ctx<'a, '_>, l: &'a Line, place: &Place, children: Vec<&'a Line>) -> Element<'a, Msg> {
    let xv = cx.xv;
    let key = l.key();
    let opened = xv.open.get(&key).copied();
    let busy = xv.busy.contains(&key);
    // a customer concern has no approve / decline: only recommendations do
    let (edge, status): (Color, Option<(&str, [u8; 3])>) = if l.primary {
        (color(BLUE), None)
    } else if l.declined {
        (color(RED), Some(("Declined", RED)))
    } else if l.approved {
        (color(GREEN), Some(("Approved", GREEN)))
    } else {
        (color(GREY), Some(("Undecided", GREY)))
    };

    let mut name = column![text(if l.name.is_empty() { l.op_code.clone() } else { l.name.clone() }).font(style::BOLD)].spacing(2);
    if opened.is_none() && !l.concern.is_empty() {
        name = name.push(text(l.concern.as_str()).size(12).style(style::muted));
    }
    let mut head = row![
        container(text(if l.dms.is_empty() { "·" } else { l.dms.as_str() }).font(style::BOLD))
            .width(28)
            .align_x(Alignment::Center),
        container(name).width(Length::Fill),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    if let Some((label, c)) = status {
        head = head.push(pill(label, c));
    }
    if !l.pay_type.is_empty() {
        head = head.push(pill(l.pay_type.as_str(), GREY));
    }
    head = head
        .push(text(hours(l.hours)).size(13).width(56).align_x(Horizontal::Right))
        .push(text(money(l.total)).size(13).font(style::BOLD).width(80).align_x(Horizontal::Right));
    // + to open, – to close (plain characters: arrows like ▸ aren't in every Windows font)
    let twisty = match (l.has_detail(), opened.is_some()) {
        (false, _) => " ",
        (true, true) => "–",
        (true, false) => "+",
    };
    let head_button = button(head.push(text(twisty).size(16).width(14).style(style::muted)))
        .width(Length::Fill)
        .padding([8, 10])
        .style(style::list_item(false))
        .on_press(Msg::ToggleLine(key.clone()));
    // Done: changes the line in Xtime. Only for work that can be done (not declined, not waiting on approval); the
    // space stays so the prices still line up. Not while the last change is on its way.
    let done_key = key.clone();
    let done: Element<'a, Msg> = if l.can_be_done() {
        checkbox("Done", l.completed)
            .size(16)
            .text_size(13)
            .on_toggle_maybe((!busy).then(|| move |on| Msg::SetDone(done_key.clone(), on)))
            .into()
    } else {
        Space::with_width(DONE_WIDTH).into()
    };
    let done = container(done).width(DONE_WIDTH);
    // Flag: the line's hours go on your hours report (only lines that count: approved, a concern, or done)
    let flag: Element<'a, Msg> = match cx.flags.get(&key) {
        Some(Some(h)) => button(text(format!("Flagged {}", hours(Some(*h)))).size(12).font(style::BOLD))
            .padding([4, 10])
            .style(button::success)
            .on_press(Msg::Ask(Ask::Flag(key.clone(), false)))
            .into(),
        Some(None) => button(text("Flag").size(12))
            .padding([4, 10])
            .style(style::ghost)
            .on_press(Msg::Ask(Ask::Flag(key.clone(), true)))
            .into(),
        None => Space::with_width(0).into(),
    };
    let flag = container(flag).width(FLAG_WIDTH).align_x(Alignment::End);
    // the handle (not on inspection lines, or while a line's being moved)
    let draggable = !l.is_inspection();
    let handle: Element<'a, Msg> = if draggable {
        grip(key.clone(), xv.line_moving.is_none() && !busy)
    } else {
        Space::with_width(0).into()
    };
    let head_row = row![handle, head_button, flag, done]
        .spacing(10)
        .align_y(Alignment::Center)
        .padding(iced::Padding {
            top: 0.0,
            right: 10.0,
            bottom: 0.0,
            left: if draggable { 2.0 } else { 10.0 },
        });
    // dragging: over this line, the dragged one would land before it; over a concern, a recommendation goes under it
    let dragged = xv.drag.as_ref().is_some_and(|d| d.key == key);
    let head: Element<'a, Msg> = match &xv.drag {
        Some(d) if draggable && !dragged => {
            let t = if l.primary && !d.primary {
                Target {
                    place: Place::Under(l.id.clone()),
                    before: children.first().map(|c| c.key()),
                }
            } else {
                Target {
                    place: place.clone(),
                    before: Some(key.clone()),
                }
            };
            mouse_area(head_row).on_enter(Msg::DragOver(t)).into()
        }
        _ => head_row.into(),
    };
    let mut card = column![head].spacing(0);

    if let Some(at) = opened {
        // Edit (Xtime's Add Service window, filled in from the line); Delete for recommendations (press twice)
        let mut actions = row![
            horizontal_space(),
            button(text("Edit service").size(12))
                .padding([4, 10])
                .style(style::ghost)
                .on_press_maybe((!busy).then(|| Msg::Ask(Ask::EditService(key.clone())))),
        ]
        .spacing(8);
        if !l.primary {
            let asking = xv.deleting.as_deref() == Some(key.as_str());
            actions = actions.push(
                button(text(if asking { "Delete? Press again" } else { "Delete service" }).size(12))
                    .padding([4, 10])
                    .style(if asking {
                        button::danger as fn(&iced::Theme, button::Status) -> button::Style
                    } else {
                        style::ghost
                    })
                    .on_press_maybe((!busy).then(|| Msg::DeleteLine(key.clone()))),
            );
        }
        let body = column![
            line_body(l),
            container(actions).padding(iced::Padding {
                top: 0.0,
                right: 14.0,
                bottom: 10.0,
                left: 48.0,
            })
        ];
        card = card.push(anim::page(body, anim::Phase::Entering(at)).on_card());
    }
    // a recommendation being dragged can go under this concern (at the end of what's under it)
    let under = Place::Under(l.id.clone());
    let can_take = l.primary && xv.drag.as_ref().is_some_and(|d| !dragged && d.fits(&under));
    if !children.is_empty() || can_take {
        let label = if l.dms.is_empty() {
            "Recommended for this concern".to_string()
        } else {
            format!("Recommended for {}", l.dms)
        };
        let mut kids = column![text(label).size(12).font(style::BOLD).style(style::muted)].spacing(6);
        for c in children {
            if let Some(m) = marker(xv, &under, Some(c)) {
                kids = kids.push(m);
            }
            kids = kids.push(line_card(cx, c, &under, vec![]));
        }
        if can_take {
            let to = if l.dms.is_empty() {
                "this concern".to_string()
            } else {
                format!("line {}", l.dms)
            };
            kids = kids.push(slot(
                xv,
                Target {
                    place: under,
                    before: None,
                },
                format!("Put it under {to}"),
            ));
        }
        card = card.push(container(kids).padding(iced::Padding {
            top: 4.0,
            right: 10.0,
            bottom: 10.0,
            left: 34.0,
        }));
    }
    let faded = l.completed;
    container(card)
        .width(Length::Fill)
        .style(move |theme: &iced::Theme| {
            let mut s = style::card(theme);
            s.border.color = edge;
            s.border.width = if faded { 1.0 } else { 1.5 };
            // picked up: it shows lifted out of its place
            if dragged {
                let accent = style::colors(theme).accent;
                s.border.color = accent;
                s.border.width = 2.0;
                s.text_color = Some(style::colors(theme).muted);
                s.shadow = iced::Shadow {
                    color: Color { a: 0.25, ..Color::BLACK },
                    offset: iced::Vector::new(0.0, 6.0),
                    blur_radius: 18.0,
                };
            }
            s
        })
        .into()
}

/// A label over its text (Concern, Cause…)
fn labeled<'a>(k: &'a str, v: String) -> Element<'a, Msg> {
    column![text(k).size(12).font(style::BOLD).style(style::muted), text(v)]
        .spacing(2)
        .into()
}

/// The parts table's column widths (0: the rest)
const PART_COLS: [f32; 5] = [150.0, 0.0, 50.0, 160.0, 80.0];

fn cell(e: Element<'_, Msg>, i: usize) -> Element<'_, Msg> {
    let width = if PART_COLS[i] == 0.0 {
        Length::Fill
    } else {
        Length::Fixed(PART_COLS[i])
    };
    container(e).width(width).into()
}

/// What's under a line when it's open: concern, cause, correction, notes, parts, and the rest in one line
fn line_body(l: &Line) -> Element<'_, Msg> {
    let mut body = column![].spacing(8);
    for (k, v) in [
        ("Concern", &l.concern),
        ("Cause", &l.cause),
        ("Correction", &l.correction),
        ("Technician note", &l.tech_note),
        ("Advisor note", &l.advisor_note),
        ("Manager note", &l.manager_note),
        ("Parts note", &l.parts_note),
    ] {
        if !v.is_empty() {
            body = body.push(labeled(k, v.clone()));
        }
    }
    if !l.sub_lines.is_empty() {
        body = body.push(labeled("Sub-operations", l.sub_lines.join(" · ")));
    }
    if !l.part_list.is_empty() {
        let head = row(["Part #", "Description", "Qty", "Status", "Price"]
            .iter()
            .enumerate()
            .map(|(i, h)| cell(text(*h).size(12).font(style::BOLD).style(style::muted).into(), i)))
        .spacing(8);
        let mut table = column![head].spacing(4);
        for p in &l.part_list {
            let number: Element<'_, Msg> = if p.number.is_empty() {
                text("—").into()
            } else {
                button(text(p.number.as_str()).font(style::MONO).size(13))
                    .padding([1, 4])
                    .style(style::ghost)
                    .on_press(Msg::Ask(Ask::Copy(p.number.clone(), "Part number".into())))
                    .into()
            };
            let status = [p.status.as_str(), if p.picked_up { "picked up" } else { "" }]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" · ");
            table = table.push(
                row![
                    cell(number, 0),
                    cell(text(p.description.as_str()).size(13).into(), 1),
                    cell(text(p.quantity.map(|q| q.to_string()).unwrap_or_default()).size(13).into(), 2),
                    cell(text(status).size(13).into(), 3),
                    cell(text(money(p.price)).size(13).into(), 4),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        body = body.push(column![text("Parts").size(12).font(style::BOLD).style(style::muted), table].spacing(4));
    } else if l.parts_not_needed {
        body = body.push(text("No parts needed").size(12).style(style::muted));
    }
    let mut meta: Vec<String> = Vec::new();
    if !l.dispatch.is_empty() {
        meta.push(format!("Dispatch {}", l.dispatch));
    }
    if let Some(e) = l.estimate.filter(|e| *e > 0.0) {
        meta.push(format!("Estimated {}", hours(Some(e))));
    }
    if l.labor.is_some() {
        meta.push(format!("Labor {}", money(l.labor)));
    }
    if l.parts.is_some() {
        meta.push(format!("Parts {}", money(l.parts)));
    }
    if !l.techs.is_empty() {
        meta.push(format!("Tech: {}", l.techs.join(", ")));
    }
    if !l.tech_start.is_empty() {
        meta.push(format!("Started {}", ro::when(&l.tech_start)));
    }
    if !l.tech_end.is_empty() {
        meta.push(format!("Finished {}", ro::when(&l.tech_end)));
    }
    if l.images > 0 {
        meta.push(format!("{} picture{} in Xtime", l.images, if l.images == 1 { "" } else { "s" }));
    }
    if !l.added_by.is_empty() {
        meta.push(format!("Added by {}", l.added_by));
    }
    if l.parts_picked_up {
        meta.push("Parts picked up".into());
    }
    if !meta.is_empty() {
        body = body.push(text(meta.join(" · ")).size(12).style(style::muted));
    }
    container(body)
        .padding(iced::Padding {
            top: 2.0,
            right: 14.0,
            bottom: 12.0,
            left: 48.0,
        })
        .width(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn writes_thousands() {
        assert_eq!(super::thousands(1234.0), "1,234");
        assert_eq!(super::thousands(123456.0), "123,456");
        assert_eq!(super::thousands(12.0), "12");
        assert_eq!(super::thousands(1000000.0), "1,000,000");
    }
}
