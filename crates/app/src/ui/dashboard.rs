//! Your Xtime dashboard: your ROs as a table, with search. Clicking one opens it in the Workbench.

use iced::widget::{Column, button, column, container, horizontal_space, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};
use xtime::dashboard::AssignedRo;

use super::{pill, style};
use crate::app::{App, Message};
use crate::workspace::{mode_label, vehicle_name};

const COLS: [(&str, f32); 6] = [
    ("RO", 90.0),
    ("Tag", 60.0),
    ("Vehicle", 0.0),
    ("VIN", 190.0),
    ("Status", 170.0),
    ("Advisor", 150.0),
];

fn cell<'a>(content: impl Into<Element<'a, Message>>, i: usize) -> Element<'a, Message> {
    let w = if COLS[i].1 == 0.0 { Length::Fill } else { Length::Fixed(COLS[i].1) };
    container(content).width(w).into()
}

fn matches(ro: &AssignedRo, q: &str) -> bool {
    q.is_empty() || [&ro.ro, &ro.tag, &ro.vin].iter().any(|s| s.to_lowercase().contains(q)) || vehicle_name(ro).to_lowercase().contains(q)
}

pub fn view(app: &App) -> Element<'_, Message> {
    let q = app.search.trim().to_lowercase();
    let shown: Vec<&AssignedRo> = app.my_ros.iter().filter(|r| matches(r, &q)).collect();
    let count = shown.len();

    let head = row![
        text("My dashboard").size(18).font(style::BOLD),
        text(format!("{} of {}", count, app.my_ros.len())).style(style::muted),
        horizontal_space(),
        text_input("Find RO, tag, vehicle, VIN…", &app.search)
            .on_input(Message::Search)
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

fn ro_row<'a>(ro: &'a AssignedRo) -> Element<'a, Message> {
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
        .on_press(Message::OpenRo(ro.ro.clone()))
        .into()
}