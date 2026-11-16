//! The sidebar: the vehicles you have open, then your ROs (crates/xtime-ui's My ROs; click one to open it).

use iced::widget::{Space, button, column, container, row, text};
// scrolling that glides (crates/ui-kit)
use iced::{Alignment, Element, Length};
use ui_kit::scroll::scrollable;

use super::style;
use crate::app::{App, Message};
use crate::workspace::Workspace;

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
    // grouped by where they are: Inspection, Approval, Repair, Review, Done (crates/xtime-ui)
    let spiffs = app.spiff_badges();
    list = list.push(xtime_ui::my_ros::list(&app.my_ros, app.selected.as_deref(), &spiffs).map(Message::from));
    container(scrollable(list.padding(10)).height(Length::Fill))
        .width(WIDTH)
        .height(Length::Fill)
        .style(style::panel)
        .into()
}

fn open_item<'a>(app: &'a App, ws: &'a Workspace) -> Element<'a, Message> {
    let selected = app.selected.as_deref() == Some(ws.ro.ro.as_str());
    let item = row![
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
    .align_y(Alignment::Center);
    if !selected {
        return item.into();
    }
    // the vehicle you're on: its lines under it (to flag, change their hours…)
    column![item, super::lines::panel(app, &ws.ro.ro, app.hours.ro_summary(&ws.ro.ro))]
        .spacing(4)
        .into()
}
