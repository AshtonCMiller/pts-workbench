//! The sidebar: the vehicles you have open, then your ROs (click one to open it).

use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};
use xtime::dashboard::AssignedRo;

use super::{pill, style};
use crate::app::{App, Message};
use crate::workspace::{STAGES, Workspace, stage, vehicle_name};

/// How wide My ROs is
pub const WIDTH: f32 = 320.0;

pub fn view(app: &App) -> Element<'_, Message> {
    let head = |t: String| text(t).size(11).font(style::BOLD).style(style::muted);
    let mut list = column![head(format!("OPEN VEHICLES  {}", app.open.len()))].spacing(4);
    if app.open.is_empty() {
        list = list.push(text("Pick one of your ROs below to open it.").size(13).style(style::muted));
    }
    for ws in &app.open {
        list = list.push(open_item(app, ws));
    }
    list = list
        .push(Space::with_height(12))
        .push(head(format!("MY ROS  {}", app.my_ros.len())));
    // grouped by where they are: Inspection, Approval, Repair, Review, Done (empty groups left out)
    for (i, name) in STAGES.iter().enumerate() {
        let ros: Vec<&AssignedRo> = app.my_ros.iter().filter(|r| stage(r.mode) == i).collect();
        if ros.is_empty() {
            continue;
        }
        list = list.push(separator(name, ros.len()));
        for ro in ros {
            list = list.push(ro_item(app, ro));
        }
    }
    container(scrollable(list.padding(10)).height(Length::Fill))
        .width(WIDTH)
        .height(Length::Fill)
        .style(style::panel)
        .into()
}

fn open_item<'a>(app: &'a App, ws: &'a Workspace) -> Element<'a, Message> {
    let selected = app.selected.as_deref() == Some(ws.ro.ro.as_str());
    row![
        button(column![text(format!("RO {}", ws.ro.ro)).font(style::BOLD), text(ws.vehicle()).size(13)].spacing(2))
            .width(Length::Fill)
            .padding(8)
            .style(style::list_item(selected))
            .on_press(Message::OpenRo(ws.ro.ro.clone())),
        button(text("×"))
            .padding([2, 8])
            .style(button::text)
            .on_press(Message::CloseRo(ws.ro.ro.clone())),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// A group's name and count, with a thin line after it
fn separator<'a>(name: &'a str, count: usize) -> Element<'a, Message> {
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

fn ro_item<'a>(app: &'a App, ro: &'a AssignedRo) -> Element<'a, Message> {
    let selected = app.selected.as_deref() == Some(ro.ro.as_str());
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
    if app.hours.spiffed(&ro.ro).is_some() {
        tags = tags.push(pill("Spiffed", [0x15, 0x80, 0x3d]));
    } else if let Some(total) = app.spiff_totals.get(&ro.ro).copied().filter(|t| xtime::ro::spiff_over(*t)) {
        tags = tags.push(pill(format!("Spiff {}", crate::print::money(total)), [0x0e, 0x74, 0x90]));
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
        .on_press(Message::OpenRo(ro.ro.clone()))
        .into()
}
