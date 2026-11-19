//! The sidebar: the vehicles you have open, then your ROs (crates/xtime-ui's My ROs; click one to open it).

use iced::widget::{Row, Space, button, column, container, horizontal_space, row, text, text_input};
// scrolling that glides (crates/ui-kit)
use iced::{Alignment, Element, Length};
use ui_kit::scroll::scrollable;

use super::style;
use crate::app::{AddRo, App, Message, add_ro_id};
use crate::workspace::Workspace;
use ui_kit::pill;

/// How wide My ROs is
pub const WIDTH: f32 = 320.0;

pub fn view(app: &App) -> Element<'_, Message> {
    let head = |t: String| text(t).size(11).font(style::BOLD).style(style::muted);
    let add = button(text("+ Add an RO").size(12))
        .padding([2, 8])
        .style(style::ghost)
        .on_press(Message::AddRo(app.add_ro.is_none()));
    let mut list =
        column![row![head(format!("OPEN VEHICLES  {}", app.open.len())), horizontal_space(), add].align_y(Alignment::Center)].spacing(4);
    if let Some(a) = &app.add_ro {
        list = list.push(add_form(a));
    }
    if app.open.is_empty() && app.add_ro.is_none() {
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
    // ROs in Dispatch nobody has yet (folded away until opened): Accept makes one yours (it goes to Inspection)
    if !app.dispatch.is_empty() {
        let open = app.show_dispatch;
        list = list.push(Space::with_height(12)).push(
            button(
                row![
                    text(if open { "▾" } else { "▸" }).size(11).style(style::muted),
                    head(format!("UNASSIGNED IN DISPATCH  {}", app.dispatch.len())),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([2, 0])
            .style(button::text)
            .on_press(Message::ToggleDispatch),
        );
        if open {
            for r in dispatch_order(&app.dispatch) {
                list = list.push(dispatch_item(app, r));
            }
        }
    }
    container(scrollable(list.padding(10)).height(Length::Fill).remember("sidebar"))
        .width(WIDTH)
        .height(Length::Fill)
        .style(style::panel)
        .into()
}

/// Which group an RO's tag puts it in: T and 4 digits (T8888) first, then T and 3 digits (T777), then the rest
fn tag_group(tag: &str) -> u8 {
    let t = tag.trim();
    let digits = t
        .strip_prefix(['T', 't'])
        .filter(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()))
        .map(str::len);
    match digits {
        Some(4) => 0,
        Some(3) => 1,
        _ => 2,
    }
}

/// The Dispatch list's order: by tag group, then the highest RO number first
fn dispatch_order(list: &[xtime::dashboard::AssignedRo]) -> Vec<&xtime::dashboard::AssignedRo> {
    let mut out: Vec<&xtime::dashboard::AssignedRo> = list.iter().collect();
    let number = |ro: &str| ro.trim().parse::<u64>().ok();
    out.sort_by(|a, b| {
        tag_group(&a.tag)
            .cmp(&tag_group(&b.tag))
            // numbers highest first; anything not a number after them, by its text
            .then_with(|| match (number(&a.ro), number(&b.ro)) {
                (Some(x), Some(y)) => y.cmp(&x),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => b.ro.cmp(&a.ro),
            })
    });
    out
}

/// An RO in Dispatch with no technician, and Accept (asked once more before it's done: it can't be taken back here)
fn dispatch_item<'a>(app: &'a App, r: &'a xtime::dashboard::AssignedRo) -> Element<'a, Message> {
    let asking = app.accept_ask.as_deref() == Some(r.ro.as_str());
    let accepting = app.accepting.as_deref() == Some(r.ro.as_str());
    let mut info = column![
        row![
            text(format!("RO {}", r.ro)).font(style::BOLD),
            text(if r.tag.is_empty() {
                String::new()
            } else {
                format!("Tag {}", r.tag)
            })
            .style(style::muted)
        ]
        .spacing(8),
        text(xtime_ui::my_ros::vehicle_name(r)).size(13),
    ]
    .spacing(2);
    if !r.dash.advisor_name.is_empty() {
        info = info.push(text(format!("Advisor: {}", r.dash.advisor_name)).size(12).style(style::muted));
    }
    let body: Element<'a, Message> = if asking {
        column![
            info,
            text(format!("Accept RO {}? It becomes yours and goes to Inspection.", r.ro)).size(12),
            row![
                horizontal_space(),
                button(text("Cancel").size(12))
                    .padding([4, 10])
                    .style(style::ghost)
                    .on_press(Message::AskAccept(None)),
                button(text("Accept").size(12).font(style::BOLD))
                    .padding([4, 14])
                    .style(button::primary)
                    .on_press(Message::AcceptRo(r.ro.clone())),
            ]
            .spacing(6),
        ]
        .spacing(6)
        .into()
    } else {
        row![
            info.width(Length::Fill),
            button(text(if accepting { "Accepting…" } else { "Accept" }).size(12).font(style::BOLD))
                .padding([4, 12])
                .style(button::primary)
                .on_press_maybe((app.accepting.is_none()).then(|| Message::AskAccept(Some(r.ro.clone())))),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    };
    container(body).padding(8).width(Length::Fill).style(style::card).into()
}

/// "Add an RO": its number (Xtime is asked for the rest), and its VIN when Xtime can't say
fn add_form(a: &AddRo) -> Element<'_, Message> {
    let mut c = column![
        text_input("RO number", &a.ro)
            .id(add_ro_id())
            .on_input(Message::AddRoNumber)
            .on_submit(Message::SubmitAddRo)
            .padding(6)
            .size(13),
    ]
    .spacing(6);
    if a.ask_vin {
        c = c.push(
            text_input("VIN (optional when Xtime has it)", &a.vin)
                .on_input(Message::AddRoVin)
                .on_submit(Message::SubmitAddRo)
                .padding(6)
                .size(13)
                .font(style::MONO),
        );
    }
    if a.looking {
        c = c.push(text(format!("Looking up RO {} in Xtime…", a.ro)).size(12).style(style::muted));
    } else if let Some(e) = &a.error {
        c = c.push(text(e.as_str()).size(12).color(iced::Color::from_rgb8(0xdc, 0x26, 0x26)));
    } else {
        c = c.push(
            text("Any RO, not just yours: it's kept up to date from Xtime (lines, approvals, parts) while it's open.")
                .size(12)
                .style(style::muted),
        );
    }
    c = c.push(
        row![
            horizontal_space(),
            button(text("Cancel").size(12))
                .padding([4, 10])
                .style(style::ghost)
                .on_press(Message::AddRo(false)),
            button(text("Add").size(12).font(style::BOLD))
                .padding([4, 14])
                .style(button::primary)
                .on_press_maybe((!a.ro.trim().is_empty() && !a.looking).then_some(Message::SubmitAddRo)),
        ]
        .spacing(6),
    );
    container(c).padding(10).width(Length::Fill).style(style::card).into()
}

fn open_item<'a>(app: &'a App, ws: &'a Workspace) -> Element<'a, Message> {
    let selected = app.selected.as_deref() == Some(ws.ro.ro.as_str());
    // where it is (Dispatch, In Inspection, Pending Approval…), its parts, what it's waiting on
    let mut tags = Row::new().spacing(4);
    for (label, color) in xtime_ui::my_ros::status_tags(&ws.ro) {
        tags = tags.push(pill(label, color));
    }
    if !ws.is_sample() && app.ros_loaded_at.is_some() && !app.my_ros.iter().any(|r| r.ro == ws.ro.ro) {
        tags = tags.push(pill("Not on your list", [0x5d, 0x66, 0x70]));
    }
    let item = row![
        button(
            column![
                text(format!("RO {}", ws.ro.ro)).font(style::BOLD),
                text(ws.vehicle()).size(13),
                tags.wrap()
            ]
            .spacing(3)
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_order_by_tag_then_ro() {
        let ro = |ro: &str, tag: &str| xtime::dashboard::AssignedRo {
            ro: ro.into(),
            tag: tag.into(),
            ..Default::default()
        };
        let list = vec![
            ro("820100", "T777"),
            ro("820300", "AB12"),
            ro("820200", "T8888"),
            ro("820500", "T8001"),
            ro("820400", "T123"),
            ro("820600", ""),
        ];
        let order: Vec<&str> = dispatch_order(&list).iter().map(|r| r.ro.as_str()).collect();
        assert_eq!(order, ["820500", "820200", "820400", "820100", "820600", "820300"]);
    }
}
