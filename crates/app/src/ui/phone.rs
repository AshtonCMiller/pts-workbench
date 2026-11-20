//! Pair phone (bottom right): the QR code a phone scans, whether one's paired, and when it gets notifications
//! (crate::phone)

use iced::widget::{Column, button, column, container, horizontal_space, row, text};
use iced::{Alignment, Color, Element, Length};

use super::{anim, pill, style};
use crate::app::{App, Message};
use crate::phone::{AFTER_CHOICES, PhoneInfo};

/// The bottom bar's button: Pair phone, or that one's paired
pub fn button_(app: &App) -> Element<'_, Message> {
    let paired = app.phone.paired();
    let label: Element<'_, Message> = if paired {
        row![
            anim::dot(Color::from_rgb8(0x16, 0xa3, 0x4a), false),
            text(match app.phone.phones.as_ref().map_or(0, Vec::len) {
                1 => "Phone paired".to_string(),
                n => format!("{n} phones paired"),
            })
            .size(13)
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
    } else {
        text("Pair phone").size(13).into()
    };
    button(label)
        .padding([3, 10])
        .style(style::ghost)
        .on_press(Message::OpenPhone)
        .into()
}

fn ago(ms: i64, now_ms: i64) -> String {
    let m = ((now_ms - ms) / 60_000).max(0);
    match m {
        0 => "just now".into(),
        1..=59 => format!("{m} min ago"),
        60..=1439 => format!("{} h ago", m / 60),
        _ => format!("{} days ago", m / 1440),
    }
}

fn phone_line(p: &PhoneInfo, now_ms: i64) -> Element<'_, Message> {
    container(
        row![
            anim::dot(Color::from_rgb8(0x16, 0xa3, 0x4a), false),
            column![
                text(p.name.as_str()).font(style::BOLD),
                text(format!("Paired {} · last opened {}", ago(p.since, now_ms), ago(p.seen, now_ms)))
                    .size(11)
                    .style(style::muted),
            ]
            .spacing(1),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([6, 10])
    .width(Length::Fill)
    .style(style::panel)
    .into()
}

fn step<'a>(n: &'a str, t: &'a str) -> Element<'a, Message> {
    row![
        container(text(n).size(12).font(style::BOLD))
            .width(22)
            .height(22)
            .center_x(22)
            .center_y(22)
            .style(|th: &iced::Theme| container::Style {
                background: Some(style::colors(th).hover.into()),
                border: iced::Border {
                    radius: 11.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        text(t).size(13).width(Length::Fill),
    ]
    .spacing(10)
    .align_y(Alignment::Start)
    .into()
}

pub fn card(app: &App) -> Element<'_, Message> {
    let p = &app.phone;
    let now_ms = crate::timecard::now_ms();
    let head = row![
        text("Your phone").size(20).font(style::BOLD),
        horizontal_space(),
        button(text("Close"))
            .padding([5, 12])
            .style(style::ghost)
            .on_press(Message::ClosePhone),
    ]
    .align_y(Alignment::Center);

    // no store ID or employee number: nothing to pair with yet
    let Some(qr) = &p.qr else {
        let body = column![
            text("Pairing your phone uses your store ID and employee number. Fill them in under Settings first.").size(13),
            button(text("Open Settings"))
                .padding([6, 14])
                .style(button::primary)
                .on_press(Message::OpenSettings),
        ]
        .spacing(12);
        return container(column![head, body].spacing(16))
            .padding(24)
            .width(560)
            .style(style::floating_card)
            .into();
    };

    // the code, and how to use it
    let code = column![
        ui_kit::qr::view(qr, 220.0),
        text("Scan with your phone's camera").size(12).style(style::muted),
    ]
    .spacing(6)
    .align_x(Alignment::Center);
    let steps = column![
        step("1", "Point your phone's camera at the code, and open the link it shows."),
        step(
            "2",
            "On an iPhone: tap Share, then Add to Home Screen. Open PTS Workbench from your Home Screen and scan this code again."
        ),
        step("3", "Tap Turn on notifications."),
    ]
    .spacing(10);

    // whether one's paired
    let status: Element<'_, Message> = match &p.phones {
        Some(list) if !list.is_empty() => Column::with_children(list.iter().map(|x| phone_line(x, now_ms))).spacing(4).into(),
        _ => match &p.error {
            Some(e) => text(format!("Couldn't reach the server: {e}")).size(13).style(style::muted).into(),
            None => row![
                anim::dot(Color::from_rgb8(0x3b, 0x82, 0xf6), true),
                text("Waiting for your phone…").size(13)
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        },
    };
    let mut right = column![steps, status].spacing(16).width(Length::Fill);

    // after how long
    let after = app.settings.phone_after_min();
    // (in a tab row: the highlight slides to the one picked)
    let choices = anim::tabs(
        AFTER_CHOICES.iter().position(|&m| m == after).unwrap_or(0),
        AFTER_CHOICES
            .iter()
            .map(|&m| {
                button(text(if m == 0 { "Never".to_string() } else { format!("{m} min") }).size(12))
                    .padding([4, 10])
                    .style(style::tab(after == m))
                    .on_press(Message::SetPhoneAfter(m))
                    .into()
            })
            .collect(),
    )
    .spacing(2.0);
    right = right.push(
        column![
            text("Send a notification to your phone when it's still unread here after:").size(13),
            choices,
            text(
                "It shows the RO, what happened and which lines, or the chat message itself (with any VIN hidden). It's encrypted for your phone: the server can't read it, and doesn't keep it."
            )
                .size(11)
                .style(style::muted),
        ]
        .spacing(6),
    );

    // a test, and unpairing
    if p.paired() {
        let mut actions = row![
            button(text(if p.testing { "Sending…" } else { "Send a test" }))
                .padding([6, 14])
                .style(button::primary)
                .on_press_maybe((!p.testing).then_some(Message::PhoneTest)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        match &p.tested {
            Some(Ok(0)) => actions = actions.push(text("Nothing sent: turn notifications on, on the phone.").size(12)),
            Some(Ok(_)) => actions = actions.push(pill("Sent", [0x15, 0x80, 0x3d])),
            Some(Err(e)) => actions = actions.push(text(format!("Couldn't send it: {e}")).size(12)),
            None => {}
        }
        actions = actions.push(horizontal_space()).push(
            button(text(if p.unpair_asked { "Really unpair?" } else { "Unpair phones" }).size(13))
                .padding([6, 12])
                .style(if p.unpair_asked { button::danger } else { style::ghost })
                .on_press(Message::PhoneUnpair),
        );
        right = right.push(actions);
    }

    let body = row![code, right].spacing(24).align_y(Alignment::Start);
    container(column![head, body].spacing(18))
        .padding(24)
        .width(720)
        .style(style::floating_card)
        .into()
}
