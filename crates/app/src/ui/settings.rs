//! First setup (the store ID and dealer code, before anything else) and Settings.

use iced::Task;
use iced::widget::{button, checkbox, column, container, horizontal_space, pick_list, row, text, text_input};
// scrolling that glides (crates/ui-kit)
use iced::{Alignment, Element, Length};
use ui_kit::scroll::scrollable;

use super::style;
use crate::app::{App, Field, Message};
use crate::settings::ThemeChoice;
use crate::signin::Site;
use crate::updates::State;

/// Each field's name, for moving to it
fn field_id(f: Field) -> text_input::Id {
    text_input::Id::new(match f {
        Field::StoreId => "store-id",
        Field::DealerCode => "dealer-code",
        Field::EmployeeNumber => "employee-number",
    })
}

/// Puts the cursor in the form's first field (when first setup or Settings opens)
pub fn focus_first_field() -> Task<Message> {
    text_input::focus(field_id(Field::StoreId))
}

fn field<'a>(label: &'a str, help: &'a str, value: &'a str, f: Field) -> Element<'a, Message> {
    column![
        text(label).font(style::BOLD),
        text_input("", value)
            .id(field_id(f))
            .on_input(move |v| Message::Edit(f, v))
            .on_submit(Message::SaveSettings)
            .padding(8),
        text(help).size(12).style(style::muted),
    ]
    .spacing(4)
    .into()
}

fn fields(app: &App) -> Element<'_, Message> {
    let d = &app.draft;
    column![
        field("Store ID", "Your dealership's Xtime store ID.", &d.store_id, Field::StoreId),
        field("Dealer code", "Your dealership's Ford P&A code.", &d.dealer_code, Field::DealerCode),
        field(
            "Employee number",
            "Your Xtime employee number: your name and ROs are found with it.",
            &d.employee_number,
            Field::EmployeeNumber
        ),
        from_xtime(app),
    ]
    .spacing(14)
    .into()
}

/// Who Xtime says you are (found from your employee number; not typed in)
fn from_xtime(app: &App) -> Element<'_, Message> {
    let s = &app.settings;
    let changed = app.draft.employee_number.trim() != s.employee_number.trim();
    let line = if changed || s.tech_id.is_empty() {
        "Your name and technician ID come from Xtime once you're signed in.".to_string()
    } else {
        let name = if s.name.is_empty() { "Found in Xtime" } else { s.name.as_str() };
        format!("{name} · technician ID {}", s.tech_id)
    };
    column![text("From Xtime").font(style::BOLD), text(line).size(13).style(style::muted)]
        .spacing(4)
        .into()
}

fn notice(app: &App) -> Element<'_, Message> {
    match &app.notice {
        Some(n) => text(n.as_str()).size(13).into(),
        None => text("").into(),
    }
}

/// Sync between your computers: on or off, how it's going, Sync now
fn sync(app: &App) -> Element<'_, Message> {
    let on = !app.settings.sync_off;
    column![
        text("Sync").font(style::BOLD),
        row![
            checkbox("Keep my hours, flags and open ROs the same on all my computers", on)
                .text_size(13)
                .on_toggle(Message::SetSync),
            horizontal_space(),
            button(text("Sync now").size(13))
                .padding([5, 12])
                .style(style::ghost)
                .on_press_maybe((on && !app.sync.running).then_some(Message::SyncNow)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        text(format!(
            "{} · Encrypted on this computer with your store ID and employee number; never VINs or customer details.",
            app.sync.describe(crate::timecard::now_ms())
        ))
        .size(12)
        .style(style::muted),
    ]
    .spacing(6)
    .into()
}

/// The version running, how updates are going, Beta updates, and Restart to update when one's ready
fn version(app: &App) -> Element<'_, Message> {
    let u = &app.updates;
    // the checkbox on a line of its own: next to it, the buttons were squeezed down to a letter a line
    let beta = checkbox("Beta updates: new features first (less tested)", u.beta)
        .text_size(13)
        .on_toggle_maybe(u.on().then_some(Message::SetBetaUpdates));
    let mut actions = row![
        button(text("What's new").size(13))
            .padding([5, 12])
            .style(style::ghost)
            .on_press(Message::ShowWhatsNew),
        button(text("Check now").size(13))
            .padding([5, 12])
            .style(style::ghost)
            .on_press_maybe((u.on() && !u.busy()).then_some(Message::CheckUpdates)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if matches!(u.state, State::Ready(_)) {
        actions = actions.push(
            button(text("Restart to update").size(13))
                .padding([5, 12])
                .style(button::primary)
                .on_press(Message::RestartToUpdate),
        );
    }
    column![
        text("Version & updates").font(style::BOLD),
        text(format!("PTS Workbench v{} · {}", u.running, u.describe()))
            .size(13)
            .style(style::muted),
        beta,
        actions,
    ]
    .spacing(6)
    .into()
}

/// Xtime and Ford PTS: signed in or not, and a button to sign in (again)
fn accounts(app: &App) -> Element<'_, Message> {
    let mut list = column![text("Accounts").font(style::BOLD)].spacing(6);
    for site in Site::ALL {
        let (state, again) = if app.signing_in == Some(site) {
            ("Signing in…", "Sign in")
        } else if app.signed_in(site) {
            ("Signed in", "Sign in again")
        } else {
            ("Not signed in", "Sign in")
        };
        list = list.push(
            row![
                text(site.name()).width(120),
                text(state).style(style::muted),
                horizontal_space(),
                button(text(again))
                    .padding([5, 12])
                    .style(style::ghost)
                    .on_press_maybe(app.signing_in.is_none().then_some(Message::SignIn(site))),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        );
    }
    list.push(
        text("Sign-ins open Xtime's or Ford's own sign-in page in a small window.")
            .size(12)
            .style(style::muted),
    )
    .into()
}

/// The first screen on a new computer
pub fn setup(app: &App) -> Element<'_, Message> {
    let card = container(
        column![
            text("Welcome to PTS Workbench").size(22).font(style::BOLD),
            text("A few details about your dealership first. You can change them later in Settings.").style(style::muted),
            fields(app),
            row![
                notice(app),
                horizontal_space(),
                button(text("Start").font(style::BOLD))
                    .padding([8, 20])
                    .style(button::primary)
                    .on_press(Message::SaveSettings)
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(16),
    )
    .padding(24)
    .width(520)
    .style(style::card);
    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(style::page)
        .into()
}

/// Settings, as a card over everything (from the top bar)
pub fn card(app: &App) -> Element<'_, Message> {
    // the card scrolls when the window is short
    container(scrollable(
        container(
            column![
                row![
                    text("Settings").size(20).font(style::BOLD),
                    horizontal_space(),
                    button(text("Close"))
                        .padding([5, 12])
                        .style(style::ghost)
                        .on_press(Message::CloseSettings)
                ]
                .align_y(Alignment::Center),
                fields(app),
                accounts(app),
                column![
                    text("Appearance").font(style::BOLD),
                    pick_list(&ThemeChoice::ALL[..], Some(app.draft.theme), Message::SetTheme).padding(6),
                ]
                .spacing(4),
                sync(app),
                version(app),
                row![
                    notice(app),
                    horizontal_space(),
                    button(text("Save").font(style::BOLD))
                        .padding([8, 20])
                        .style(button::primary)
                        .on_press(Message::SaveSettings)
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(16),
        )
        .padding(24),
    ))
    .width(560)
    .style(style::floating_card)
    .into()
}
