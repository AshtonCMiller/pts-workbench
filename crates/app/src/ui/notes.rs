//! Notifications: the list (it slides in from the right, like Chat) and the pop-up in the corner for a new one.
//! Clicking one goes to its RO (crates/app/src/notes.rs has what's noticed, and when).

use iced::widget::{Column, Space, button, column, container, horizontal_space, row, text};
use iced::{Alignment, Color, Element, Length, Padding};
use ui_kit::scroll::scrollable;

use super::{anim, style};
use crate::app::{App, Message};
use crate::notes::Note;

pub const WIDTH: f32 = 340.0;

fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgb8(c[0], c[1], c[2])
}

/// A coloured dot for the kind (filled while it's unread)
fn dot<'a>(n: &Note) -> Element<'a, Message> {
    let c = rgb(n.kind.color());
    let read = n.read;
    container(Space::new(9, 9))
        .style(move |_: &iced::Theme| container::Style {
            background: Some(if read { Color { a: 0.25, ..c } } else { c }.into()),
            border: iced::Border {
                radius: 5.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn item<'a>(n: &'a Note, now_ms: i64) -> Element<'a, Message> {
    let title = text(n.title.as_str())
        .size(13)
        .font(if n.read { iced::Font::DEFAULT } else { style::BOLD });
    let mut body = column![
        row![
            title.width(Length::Fill),
            text(crate::notes::when(n.at, now_ms)).size(11).style(style::muted)
        ]
        .spacing(6)
        .align_y(Alignment::Start)
    ]
    .spacing(2);
    if !n.body.is_empty() {
        body = body.push(text(n.body.as_str()).size(12).style(style::muted));
    }
    button(
        row![
            container(dot(n)).padding(Padding {
                top: 4.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0
            }),
            body
        ]
        .spacing(9)
        .align_y(Alignment::Start),
    )
    .width(Length::Fill)
    .padding([8, 10])
    .style(style::list_item(false))
    .on_press(Message::OpenNote(n.id))
    .into()
}

/// The list, newest first
pub fn panel(app: &App) -> Element<'_, Message> {
    let now = crate::timecard::now_ms();
    let unread = app.notes.unread();
    let head = row![
        text("Notifications").size(15).font(style::BOLD),
        horizontal_space(),
        button(text("Mark all read").size(12))
            .padding([3, 8])
            .style(style::ghost)
            .on_press_maybe((unread > 0).then_some(Message::NotesAllRead)),
        button(text("Clear").size(12))
            .padding([3, 8])
            .style(style::ghost)
            .on_press_maybe((!app.notes.list.is_empty()).then_some(Message::ClearNotes)),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    let mut list = Column::new().spacing(2);
    if app.notes.list.is_empty() {
        list = list.push(
            text(
                "Nothing yet. You'll be told here when an RO's assigned to you, goes to Approval or Repair, lines are \
                 approved, parts come in, or someone messages you.",
            )
            .size(13)
            .style(style::muted),
        );
    }
    for n in &app.notes.list {
        list = list.push(item(n, now));
    }
    container(column![head, scrollable(list).height(Length::Fill)].spacing(10).padding(12))
        .width(WIDTH)
        .height(Length::Fill)
        .style(style::panel)
        .into()
}

/// The newest one, popped up in the bottom-right corner for a few seconds (it slides in and away)
pub fn toast(app: &App) -> Option<Element<'_, Message>> {
    let (id, phase) = app.toast?;
    let n = app.notes.get(id)?;
    let more = app.notes.unread().saturating_sub(1);
    let mut body = column![text(n.title.as_str()).size(14).font(style::BOLD)].spacing(3);
    if !n.body.is_empty() {
        body = body.push(text(n.body.as_str()).size(12).style(style::muted));
    }
    if more > 0 {
        body = body.push(text(format!("{more} more in Notifications")).size(11).style(style::muted));
    }
    let card = button(
        row![
            container(dot(n)).padding(Padding {
                top: 5.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0
            }),
            body.width(Length::Fill)
        ]
        .spacing(10)
        .align_y(Alignment::Start),
    )
    .width(320)
    .padding([10, 12])
    .style(|theme: &iced::Theme, status| {
        let mut s = button::text(theme, status);
        let c = style::floating_card(theme);
        s.background = c.background;
        s.border = c.border;
        s.shadow = c.shadow;
        s.text_color = theme.palette().text;
        s
    })
    .on_press(Message::OpenNote(id));
    Some(
        container(anim::page(card, phase))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::End)
            .align_y(Alignment::End)
            // (above the bottom bar)
            .padding(Padding {
                top: 0.0,
                right: 16.0,
                bottom: 48.0,
                left: 0.0,
            })
            .into(),
    )
}
