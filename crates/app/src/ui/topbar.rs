//! The top bar: getting around (Xtime / Workbench / Diagnostics, Chat, My ROs, Settings). What's going on is in the bottom bar.

use iced::widget::{button, container, horizontal_space, row, text};
use iced::{Alignment, Element, Length};

use super::{anim, style};
use crate::app::{App, Message, Mode};

pub fn view(app: &App) -> Element<'_, Message> {
    let mode = |label: String, m: Mode| {
        button(text(label).font(style::BOLD))
            .padding([6, 14])
            .style(style::tab(app.mode == m))
            .on_press(Message::SetMode(m))
    };
    let workbench = if app.open.is_empty() {
        "Workbench".to_string()
    } else {
        format!("Workbench  {}", app.open.len())
    };

    // a dot when a vehicle's connected
    let diagnostics = if app.diag.connected() {
        "Diagnostics  •".to_string()
    } else {
        "Diagnostics".to_string()
    };
    let modes = anim::tabs(
        match app.mode {
            Mode::Xtime => 0,
            Mode::Workbench => 1,
            Mode::Diagnostics => 2,
        },
        vec![
            mode("Xtime".into(), Mode::Xtime).into(),
            mode(workbench, Mode::Workbench).into(),
            mode(diagnostics, Mode::Diagnostics).into(),
        ],
    );
    let mut bar = row![modes, horizontal_space()].spacing(6).align_y(Alignment::Center);
    let toggle = |label: String, on: bool, msg: Message| button(text(label)).padding([5, 10]).style(style::toggle(on)).on_press(msg);
    // Chat, with how many conversations have something new
    let chat = match app.chat_panel.unread() {
        0 => "Chat".to_string(),
        n => format!("Chat  {n}"),
    };
    // an update downloaded: one click starts it
    if matches!(app.updates.state, crate::updates::State::Ready(_)) {
        bar = bar.push(
            button(text("Restart to update").font(style::BOLD))
                .padding([5, 12])
                .style(button::primary)
                .on_press(Message::RestartToUpdate),
        );
    }
    // Notifications, with how many are new
    let notes = match app.notes.unread() {
        0 => "Notifications".to_string(),
        n => format!("Notifications  {n}"),
    };
    bar = bar
        .push(toggle(notes, app.notes_open, Message::ToggleNotes))
        .push(toggle(chat, app.chat, Message::ToggleChat))
        .push(toggle("My ROs".into(), app.sidebar, Message::ToggleSidebar))
        .push(
            button(text("Settings"))
                .padding([5, 10])
                .style(style::ghost)
                .on_press(Message::OpenSettings),
        );

    container(bar.padding([0, 12]))
        .height(48)
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .style(style::panel)
        .into()
}
