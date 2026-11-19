//! Profiles (crate::profiles) on screen: the switcher at the top right (whose profile, by their initials), its menu
//! (everyone's, a PIN when one has it, adding one), the question of a new profile's kind (Main Shop or Quick Lane),
//! and the profile's part of Settings.

use iced::widget::{Column, Space, button, column, container, horizontal_space, mouse_area, row, text, text_input};
use iced::{Alignment, Color, Element, Length};

use super::{anim, style};
use crate::app::{App, Message, ProfileAsk, admin_field_id, profile_field_id};
use crate::profiles::{Kind, Profile};

const MENU_WIDTH: f32 = 300.0;

/// A profile's initials in a round badge (its color from its kind)
fn badge<'a>(p: &Profile, size: f32) -> Element<'a, Message> {
    let color = match p.kind {
        Some(Kind::Quick) => Color::from_rgb8(0xd2, 0x6b, 0x16),
        _ => Color::from_rgb8(0x1a, 0x5f, 0xb4),
    };
    container(text(p.initials()).size(size * 0.42).font(style::BOLD).color(Color::WHITE))
        .width(size)
        .height(size)
        .center_x(size)
        .center_y(size)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(color.into()),
            border: iced::Border {
                radius: (size / 2.0).into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

/// The top bar's switcher: whose profile it is
pub fn switcher(app: &App) -> Element<'_, Message> {
    let p = app.profiles.active();
    button(
        row![badge(p, 24.0), text(p.name.as_str()).size(13)]
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .padding([3, 10])
    .style(style::toggle(app.profile_menu))
    .on_press(Message::ToggleProfiles)
    .into()
}

/// The switcher's menu, dropped down from it (over the page; a click beside it puts it away)
pub fn menu(app: &App) -> Element<'_, Message> {
    let mut list = Column::new().spacing(2);
    for p in &app.profiles.list {
        let here = p.id == app.profiles.active;
        let kind = p.kind.map_or("Not set up yet", Kind::label);
        let mut right = row![].spacing(6).align_y(Alignment::Center);
        if p.pin.is_some() {
            right = right.push(text("PIN").size(10).style(style::muted));
        }
        if here {
            right = right.push(text("✓").font(style::BOLD));
        }
        list = list.push(
            button(
                row![
                    badge(p, 32.0),
                    column![text(p.name.as_str()).font(style::BOLD), text(kind).size(11).style(style::muted)]
                        .spacing(1)
                        .width(Length::Fill),
                    right,
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding([7, 10])
            .width(Length::Fill)
            .style(style::list_item(here))
            .on_press(Message::PickProfile(p.id.clone())),
        );
        // its PIN, asked for right under it
        if let Some(ProfileAsk::Pin { id, typed, wrong }) = &app.profile_ask {
            if *id == p.id {
                list = list.push(pin_field(typed, *wrong));
            }
        }
    }
    let bottom: Element<'_, Message> = match &app.profile_ask {
        Some(ProfileAsk::Add { name, number, pin, error }) => add_form(name, number, pin, error.as_deref()),
        _ => button(text("+ Add a profile"))
            .padding([8, 10])
            .width(Length::Fill)
            .style(style::ghost)
            .on_press(Message::AddProfileStart)
            .into(),
    };
    let card = container(
        column![
            text("Profiles").size(12).style(style::muted),
            list,
            container(Space::new(Length::Fill, 1)).style(|t: &iced::Theme| container::Style {
                background: Some(style::colors(t).line.into()),
                ..Default::default()
            }),
            bottom,
        ]
        .spacing(8),
    )
    .padding(12)
    .width(MENU_WIDTH)
    .style(style::floating_card);
    // dropped from the top right (sliding in), a click anywhere else puts it away
    iced::widget::stack![
        mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(Message::ToggleProfiles),
        container(anim::page(card, anim::Phase::Entering(app.profile_since)))
            .width(Length::Fill)
            .align_x(Alignment::End)
            .padding(iced::Padding {
                top: 52.0,
                right: 12.0,
                bottom: 0.0,
                left: 0.0,
            }),
    ]
    .into()
}

fn pin_field<'a>(typed: &str, wrong: bool) -> Element<'a, Message> {
    column![
        text_input("PIN", typed)
            .id(profile_field_id())
            .secure(true)
            .on_input(Message::ProfilePin)
            .on_submit(Message::SubmitProfilePin)
            .padding(8),
        text(if wrong {
            "That's not its PIN (forgotten: an admin can reset it, in Settings → Admin mode)"
        } else {
            "Its PIN (four digits)"
        })
        .size(11)
        .style(style::muted),
    ]
    .spacing(4)
    .padding([4, 10])
    .into()
}

fn add_form<'a>(name: &str, number: &str, pin: &str, error: Option<&str>) -> Element<'a, Message> {
    let mut c = column![
        text("A new profile").font(style::BOLD),
        text_input("Name", name)
            .id(profile_field_id())
            .on_input(Message::AddProfileName)
            .on_submit(Message::SubmitAddProfile)
            .padding(8),
        text_input("Employee number (Xtime finds their ROs with it)", number)
            .on_input(Message::AddProfileNumber)
            .on_submit(Message::SubmitAddProfile)
            .padding(8),
        text_input("PIN (optional, four digits)", pin)
            .secure(true)
            .on_input(Message::AddProfilePin)
            .on_submit(Message::SubmitAddProfile)
            .padding(8),
    ]
    .spacing(6);
    if let Some(e) = error {
        c = c.push(text(e.to_string()).size(12));
    }
    c.push(
        row![
            text("Its own hours, ROs, sign-ins and settings.")
                .size(11)
                .style(style::muted)
                .width(Length::Fill),
            button(text("Cancel"))
                .padding([5, 10])
                .style(style::ghost)
                .on_press(Message::CancelProfileAsk),
            button(text("Add").font(style::BOLD))
                .padding([5, 14])
                .style(button::primary)
                .on_press(Message::SubmitAddProfile),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .into()
}

/// A new profile's kind: Main Shop or Quick Lane (as the sync question's card: two big choices)
pub fn kind_card(app: &App) -> Element<'_, Message> {
    let p = app.profiles.active();
    let choice = |k: Kind, about: &'static str| {
        button(
            column![text(k.label()).size(16).font(style::BOLD), text(about).size(13).style(style::muted)]
                .spacing(4)
                .width(Length::Fill),
        )
        .padding([14, 16])
        .width(Length::Fill)
        .style(style::list_item(false))
        .on_press(Message::SetProfileKind(k))
    };
    container(
        column![
            row![badge(p, 40.0), text(format!("Welcome, {}", p.name)).size(20).font(style::BOLD)]
                .spacing(12)
                .align_y(Alignment::Center),
            text("Which part of the shop is this profile for? It changes what the app shows first. (It can be changed in Settings.)")
                .size(13),
            choice(
                Kind::Main,
                "Diagnostics and repair: the Service Manual, Diagnostics with the VCI, wiring diagrams, labor times.",
            ),
            choice(
                Kind::Quick,
                "Quick service and oil changes: each vehicle opens on its maintenance specs (oil, capacities, \
                 torques, tires); no Diagnostics or wiring.",
            ),
        ]
        .spacing(14),
    )
    .padding(24)
    .width(520)
    .style(style::floating_card)
    .into()
}

/// Settings' Profile part: its name, its kind, its PIN, the others (to take off the list)
pub fn settings_part(app: &App) -> Element<'_, Message> {
    let p = app.profiles.active();
    let kind = |k: Kind| {
        button(text(k.label()).size(13))
            .padding([5, 12])
            .style(style::toggle(p.kind == Some(k)))
            .on_press(Message::SetProfileKind(k))
    };
    let pin_row = row![
        text_input(if p.pin.is_some() { "New PIN" } else { "Four digits" }, &app.pin_draft)
            .secure(true)
            .on_input(Message::SettingsPin)
            .on_submit(Message::SaveSettingsPin)
            .padding(6)
            .width(140),
        button(text(if p.pin.is_some() { "Change PIN" } else { "Set PIN" }))
            .padding([5, 10])
            .style(style::ghost)
            .on_press_maybe(crate::profiles::valid_pin(&app.pin_draft).then_some(Message::SaveSettingsPin)),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let mut pin = column![pin_row].spacing(4);
    if p.pin.is_some() {
        pin = pin.push(
            row![
                text("Switching to this profile asks for its PIN.").size(12).style(style::muted),
                horizontal_space(),
                button(text("Take the PIN off").size(12))
                    .padding([3, 8])
                    .style(style::ghost)
                    .on_press(Message::ClearSettingsPin),
            ]
            .align_y(Alignment::Center),
        );
    }
    let mut c = column![
        text("Profile").font(style::BOLD),
        text_input("Name", &app.profile_name)
            .on_input(Message::ProfileName)
            .on_submit(Message::SaveSettings)
            .padding(8),
        row![kind(Kind::Main), kind(Kind::Quick)].spacing(4),
        pin,
    ]
    .spacing(8);
    let others: Vec<&Profile> = app.profiles.list.iter().filter(|o| o.id != p.id).collect();
    if !others.is_empty() {
        let mut list = Column::new().spacing(2);
        for o in others {
            list = list.push(
                row![
                    badge(o, 22.0),
                    text(o.name.as_str()).size(13).width(Length::Fill),
                    button(text("Take off the list").size(12))
                        .padding([3, 8])
                        .style(style::ghost)
                        .on_press(Message::RemoveProfile(o.id.clone())),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        c = c.push(
            text("Other profiles on this computer (taking one off keeps its files)")
                .size(12)
                .style(style::muted),
        );
        c = c.push(list);
    }
    c.into()
}

/// Admin mode (from Settings): the Feedback password, then every profile on this computer (its PIN reset, it
/// deleted with its files)
pub fn admin_card(app: &App) -> Element<'_, Message> {
    let a = &app.admin;
    let head = row![
        text("Admin mode").size(20).font(style::BOLD),
        horizontal_space(),
        button(text(if a.unlocked { "Lock" } else { "Close" }))
            .padding([5, 12])
            .style(style::ghost)
            .on_press(if a.unlocked { Message::AdminLock } else { Message::CloseAdmin }),
    ]
    .align_y(Alignment::Center);
    let body: Element<'_, Message> = if !a.unlocked {
        let mut c = column![
            text("The Feedback password (the one for everyone's reports). It's checked with the feedback server.").size(13),
            row![
                text_input("Password", &a.key)
                    .id(admin_field_id())
                    .secure(true)
                    .on_input(Message::AdminKey)
                    .on_submit(Message::AdminUnlock)
                    .padding(8)
                    .width(Length::Fill),
                button(text(if a.checking { "Checking…" } else { "Unlock" }).font(style::BOLD))
                    .padding([7, 16])
                    .style(button::primary)
                    .on_press_maybe((!a.checking && !a.key.trim().is_empty()).then_some(Message::AdminUnlock)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(10);
        if let Some(e) = &a.error {
            c = c.push(text(format!("Couldn't unlock: {e}")).size(13));
        }
        c.into()
    } else {
        let mut list = Column::new().spacing(4);
        for p in &app.profiles.list {
            let here = p.id == app.profiles.active;
            let first = p.id == crate::profiles::FIRST;
            let mut about = vec![p.kind.map_or("Not set up yet", Kind::label).to_string()];
            if p.pin.is_some() {
                about.push("PIN".into());
            }
            if here {
                about.push("in use".into());
            }
            if first {
                about.push("the app's own folder".into());
            }
            let asked = a.confirm_delete.as_deref() == Some(p.id.as_str());
            let delete = button(text(if asked { "Really delete?" } else { "Delete" }).size(12))
                .padding([4, 10])
                .style(if asked { button::danger } else { style::ghost })
                .on_press_maybe((!here && !first).then(|| Message::AdminDelete(p.id.clone())));
            list = list.push(
                container(
                    row![
                        badge(p, 30.0),
                        column![
                            text(p.name.as_str()).font(style::BOLD),
                            text(about.join("  ·  ")).size(11).style(style::muted)
                        ]
                        .spacing(1)
                        .width(Length::Fill),
                        button(text("Reset PIN").size(12))
                            .padding([4, 10])
                            .style(style::ghost)
                            .on_press_maybe(p.pin.is_some().then(|| Message::AdminResetPin(p.id.clone()))),
                        delete,
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .padding([6, 10])
                .style(style::panel),
            );
        }
        let mut c = column![
            text(
                "Every profile on this computer. Deleting one deletes its folder too: its hours, ROs, settings and \
                  sign-ins on this computer (anything synced stays on the sync server)."
            )
            .size(13),
            list,
        ]
        .spacing(10);
        if let Some(n) = &a.note {
            c = c.push(text(n.as_str()).size(13).font(style::BOLD));
        }
        c.into()
    };
    container(column![head, body].spacing(16))
        .padding(24)
        .width(560)
        .style(style::floating_card)
        .into()
}
