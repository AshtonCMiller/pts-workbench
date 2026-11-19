//! The "What's new" card (crate::whats_new): each update's changes, grouped by category, the **bold** and `code` in
//! them drawn so; "Got it" (or Restart to update, when they're an update's that's waiting).

use iced::widget::{Column, button, column, container, horizontal_space, rich_text, row, span, text};
use iced::{Alignment, Element, Font, Length};
use ui_kit::scroll::scrollable;

use super::style;
use crate::app::{App, Message};
use crate::whats_new::{Group, Shown, spans};

fn item<'a>(t: &str) -> Element<'a, Message> {
    let words: Vec<iced::widget::text::Span<'a, Message>> = spans(t)
        .into_iter()
        .map(|(w, bold, code)| {
            let s = span(w);
            match (bold, code) {
                (_, true) => s.font(Font::MONOSPACE),
                (true, false) => s.font(style::BOLD),
                _ => s,
            }
        })
        .collect();
    row![
        text("•").size(14).style(style::muted).width(14),
        rich_text(words).size(14).width(Length::Fill)
    ]
    .spacing(4)
    .into()
}

fn group<'a>(g: &Group) -> Element<'a, Message> {
    let mut c = Column::new().spacing(6);
    if !g.head.is_empty() {
        c = c.push(text(g.head.clone()).size(16).font(style::BOLD));
    }
    let mut cat: Option<&str> = None;
    for (k, t) in &g.items {
        if cat != Some(k.as_str()) && !k.is_empty() {
            c = c.push(container(text(k.to_uppercase()).size(11).font(style::BOLD).style(style::muted)).padding([4, 0]));
        }
        cat = Some(k.as_str());
        c = c.push(item(t));
    }
    c.into()
}

pub fn card(app: &App) -> Element<'_, Message> {
    let Some(shown) = &app.whats_new else {
        return text("").into();
    };
    let Shown { ready, all, groups } = shown;
    let sub = match (ready, all) {
        (Some(v), _) => format!("v{v} is downloaded: restart to update"),
        (None, true) => format!("You're on v{}", app.updates.running),
        (None, false) => format!("Updated to v{}", app.updates.running),
    };
    let mut body = Column::new().spacing(18);
    for g in groups {
        body = body.push(group(g));
    }
    let mut actions = row![horizontal_space()].spacing(8).align_y(Alignment::Center);
    if ready.is_some() {
        actions = actions.push(
            button(text("Later"))
                .padding([8, 16])
                .style(style::ghost)
                .on_press(Message::CloseWhatsNew),
        );
        actions = actions.push(
            button(text("Restart to update").font(style::BOLD))
                .padding([8, 20])
                .style(button::primary)
                .on_press(Message::RestartToUpdate),
        );
    } else {
        actions = actions.push(
            button(text("Got it").font(style::BOLD))
                .padding([8, 24])
                .style(button::primary)
                .on_press(Message::CloseWhatsNew),
        );
    }
    container(
        column![
            column![
                text("What's new").size(22).font(style::BOLD),
                text(sub).size(13).style(style::muted)
            ]
            .spacing(2),
            container(scrollable(container(body).padding([0, 4])).height(Length::Shrink)).max_height(440),
            actions,
        ]
        .spacing(16)
        .padding(24),
    )
    .width(560)
    .style(style::floating_card)
    .into()
}
