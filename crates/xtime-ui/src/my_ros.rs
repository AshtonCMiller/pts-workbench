//! Your ROs, as Xtime's dashboard has them: the dashboard table (with search) and the My ROs list (grouped by where
//! they are: Inspection, Approval, Repair, Review, Done), and the helpers both use (the vehicle's name, the stage's
//! label and colour). Clicking an RO asks the app to open it.

use std::collections::HashMap;

use iced::widget::{Column, Space, button, column, container, horizontal_space, row, text, text_input};
// scrolling that glides (crates/ui-kit)
use iced::{Alignment, Element, Length};
use ui_kit::scroll::scrollable;
use ui_kit::{pill, style};
use xtime::dashboard::AssignedRo;

/// What the dashboard and My ROs do
#[derive(Debug, Clone)]
pub enum RosMsg {
    /// Typing in the dashboard's search
    Search(String),
    /// Open an RO (its number) in the Workbench
    Open(String),
}

/// An RO's spiff, for My ROs: turned in already, or there to be turned in (what the recommendations come to)
#[derive(Debug, Clone, PartialEq)]
pub enum SpiffBadge {
    Spiffed,
    Available(f64),
}

/// "2026 Ford Bronco"
pub fn vehicle_name(ro: &AssignedRo) -> String {
    [ro.year.as_str(), ro.make.as_str(), ro.model.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

/// My ROs' groups in the sidebar, in order
pub const STAGES: [&str; 5] = ["Inspection", "Approval", "Repair", "Review", "Done"];

/// Which of [`STAGES`] an RO is in by its Xtime mode: not started yet (Pre-RO, Dispatch) counts as Inspection; closed,
/// or a stage the app doesn't know, as Done
pub fn stage(mode: Option<f64>) -> usize {
    match mode.map(|m| m as i64) {
        Some(2 | 3 | 9) | None => 0,
        Some(4 | 5) => 1,
        Some(6) => 2,
        Some(7) => 3,
        _ => 4,
    }
}

/// Xtime's mode numbers, as the dashboard shows them
pub fn mode_label(mode: Option<f64>) -> (&'static str, [u8; 3]) {
    match mode.map(|m| m as i64) {
        Some(3) => ("In Inspection", [0x7a, 0x5a, 0xf5]),
        Some(4) => ("Pending Approval", [0xd9, 0x77, 0x06]),
        Some(5) => ("Pending Approval (parts?)", [0xd9, 0x77, 0x06]),
        Some(6) => ("In Repair", [0x15, 0x80, 0x3d]),
        Some(7) => ("Pending Review", [0x0e, 0x74, 0x90]),
        _ => ("", [0x5d, 0x66, 0x70]),
    }
}

/// My ROs, grouped by where they are (empty groups left out). `selected`: the RO showing; `spiffs`: each RO's spiff
pub fn list<'a>(ros: &'a [AssignedRo], selected: Option<&str>, spiffs: &HashMap<String, SpiffBadge>) -> Element<'a, RosMsg> {
    let mut list = Column::new().spacing(4);
    for (i, name) in STAGES.iter().enumerate() {
        let group: Vec<&AssignedRo> = ros.iter().filter(|r| stage(r.mode) == i).collect();
        if group.is_empty() {
            continue;
        }
        list = list.push(separator(name, group.len()));
        for ro in group {
            list = list.push(ro_item(ro, selected == Some(ro.ro.as_str()), spiffs.get(&ro.ro)));
        }
    }
    list.into()
}

const COLS: [(&str, f32); 6] = [
    ("RO", 90.0),
    ("Tag", 60.0),
    ("Vehicle", 0.0),
    ("VIN", 190.0),
    ("Status", 170.0),
    ("Advisor", 150.0),
];

fn cell<'a>(content: impl Into<Element<'a, RosMsg>>, i: usize) -> Element<'a, RosMsg> {
    let w = if COLS[i].1 == 0.0 { Length::Fill } else { Length::Fixed(COLS[i].1) };
    container(content).width(w).into()
}

fn matches(ro: &AssignedRo, q: &str) -> bool {
    q.is_empty() || [&ro.ro, &ro.tag, &ro.vin].iter().any(|s| s.to_lowercase().contains(q)) || vehicle_name(ro).to_lowercase().contains(q)
}

/// Your ROs as a table (Xtime's dashboard), with search
pub fn dashboard<'a>(ros: &'a [AssignedRo], search: &str) -> Element<'a, RosMsg> {
    let q = search.trim().to_lowercase();
    let shown: Vec<&AssignedRo> = ros.iter().filter(|r| matches(r, &q)).collect();
    let count = shown.len();

    let head = row![
        text("My dashboard").size(18).font(style::BOLD),
        text(format!("{} of {}", count, ros.len())).style(style::muted),
        horizontal_space(),
        text_input("Find RO, tag, vehicle, VIN…", search)
            .on_input(RosMsg::Search)
            .padding(7)
            .width(300),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let header = row(COLS
        .iter()
        .enumerate()
        .map(|(i, (label, _))| cell(text(*label).size(12).font(style::BOLD).style(style::muted), i)))
    .spacing(8);
    let rows = Column::with_children(shown.into_iter().map(ro_row)).spacing(2);

    let table = container(column![container(header).padding([8, 10]), scrollable(rows).height(Length::Fill)].spacing(2))
        .style(style::card)
        .height(Length::Fill);
    container(column![head, table].spacing(12))
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn ro_row<'a>(ro: &'a AssignedRo) -> Element<'a, RosMsg> {
    let (mode, color) = mode_label(ro.mode);
    let mut status = row![].spacing(4);
    if !mode.is_empty() {
        status = status.push(pill(mode, color));
    }
    if ro.waiting_parts {
        status = status.push(pill("Parts", [0xdc, 0x26, 0x26]));
    }
    if ro.waiting_customer {
        status = status.push(pill("Customer", [0xb4, 0x53, 0x09]));
    }
    let r = row![
        cell(text(ro.ro.as_str()).font(style::BOLD), 0),
        cell(text(ro.tag.as_str()), 1),
        cell(text(vehicle_name(ro)), 2),
        cell(text(ro.vin.as_str()).font(style::MONO).size(13), 3),
        cell(status, 4),
        cell(text(ro.dash.advisor_name.as_str()).style(style::muted), 5),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    button(r)
        .width(Length::Fill)
        .padding([7, 10])
        .style(style::list_item(false))
        .on_press(RosMsg::Open(ro.ro.clone()))
        .into()
}

/// A group's name and count, with a thin line after it
fn separator<'a>(name: &'a str, count: usize) -> Element<'a, RosMsg> {
    let line = container(Space::with_height(1))
        .width(Length::Fill)
        .style(|theme: &iced::Theme| container::Style {
            background: Some(style::colors(theme).line.into()),
            ..Default::default()
        });
    row![
        text(name).size(12).font(style::BOLD).style(style::muted),
        text(count.to_string()).size(12).style(style::muted),
        line,
    ]
    .spacing(6)
    .padding([6, 2])
    .align_y(Alignment::Center)
    .into()
}

fn ro_item<'a>(ro: &'a AssignedRo, selected: bool, spiff: Option<&SpiffBadge>) -> Element<'a, RosMsg> {
    // the group says where it is; only what it's waiting on shows here
    let mut tags = row![].spacing(4);
    for (on, label, color) in [
        (ro.waiting_parts, "Waiting on Parts", [0xdc, 0x26, 0x26]),
        (ro.waiting_customer, "Waiting on Customer", [0xb4, 0x53, 0x09]),
        (ro.waiting_warranty, "Waiting on Warranty", [0x7c, 0x3a, 0xed]),
    ] {
        if on {
            tags = tags.push(pill(label, color));
        }
    }
    // the spiff: turned in already, or there to be turned in (approved recommendations over $150)
    match spiff {
        Some(SpiffBadge::Spiffed) => tags = tags.push(pill("Spiffed", [0x15, 0x80, 0x3d])),
        Some(SpiffBadge::Available(total)) => tags = tags.push(pill(format!("Spiff ${total:.2}"), [0x0e, 0x74, 0x90])),
        None => {}
    }
    let title = row![
        text(format!("RO {}", ro.ro)).font(style::BOLD),
        text(format!("Tag {}", ro.tag)).style(style::muted)
    ]
    .spacing(8);
    button(column![title, text(vehicle_name(ro)).size(13), tags].spacing(4))
        .width(Length::Fill)
        .padding(8)
        .style(style::list_item(selected))
        .on_press(RosMsg::Open(ro.ro.clone()))
        .into()
}
