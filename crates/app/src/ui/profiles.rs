//! Profiles (crate::profiles) on screen: the switcher at the top right (whose profile, by their initials), its menu
//! (everyone's, a PIN when one has it, adding one), the question of a new profile's kind (Main Shop or Quick Lane),
//! and Settings' Profiles tab (yours to change; everyone's, to switch to; admin mode's Reset PIN and Delete).

use iced::widget::{Column, Space, button, column, container, horizontal_space, mouse_area, row, text, text_input};
use iced::{Alignment, Color, Element, Length};

use super::{anim, style};
use crate::app::{App, Message, ProfileAsk, profile_field_id};
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
            "That's not its PIN (forgotten: an admin can reset it, in Settings › About Workbench › Admin mode)"
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

/// When something happened, in words: "today at 3:41 PM", "yesterday at 9:02 AM", "Mon at 8:15 AM", "Nov 3"
fn when(ms: i64) -> String {
    use chrono::{Datelike, Local, TimeZone};
    let Some(t) = Local.timestamp_millis_opt(ms).single() else {
        return String::new();
    };
    let now = Local::now();
    let clock = t.format("%-I:%M %p").to_string();
    match now.date_naive().signed_duration_since(t.date_naive()).num_days() {
        0 => format!("today at {clock}"),
        1 => format!("yesterday at {clock}"),
        2..=6 => format!("{} at {clock}", t.format("%a")),
        _ if t.year() == now.year() => t.format("%b %-d").to_string(),
        _ => t.format("%b %-d, %Y").to_string(),
    }
}

/// Settings' Profiles tab: yours (its name, kind, PIN, employee number), the dealership's details, sync, and everyone
/// on this computer (whether they've a PIN, when they were last used; switching to them; adding one; with admin mode
/// on, their PIN reset or them deleted)
pub fn settings_tab(app: &App) -> Element<'_, Message> {
    use super::settings::{dealership, employee, from_xtime, section, sync};
    let me = app.profiles.active();

    // yours
    let kinds = anim::tabs(
        match me.kind {
            Some(Kind::Quick) => 1,
            _ => 0,
        },
        [Kind::Main, Kind::Quick]
            .into_iter()
            .map(|k| {
                button(text(k.label()).size(13))
                    .padding([6, 14])
                    .style(style::tab(me.kind == Some(k)))
                    .on_press(Message::SetProfileKind(k))
                    .into()
            })
            .collect(),
    )
    .spacing(2.0);
    let mut pin = column![
        row![
            text_input(if me.pin.is_some() { "New PIN" } else { "Four digits" }, &app.pin_draft)
                .secure(true)
                .on_input(Message::SettingsPin)
                .on_submit(Message::SaveSettingsPin)
                .padding(6)
                .width(140),
            button(text(if me.pin.is_some() { "Change PIN" } else { "Set PIN" }))
                .padding([5, 10])
                .style(style::ghost)
                .on_press_maybe(crate::profiles::valid_pin(&app.pin_draft).then_some(Message::SaveSettingsPin)),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
    ]
    .spacing(4);
    pin = pin.push(if me.pin.is_some() {
        Element::from(
            row![
                text("Switching to this profile asks for its PIN.").size(12).style(style::muted),
                horizontal_space(),
                button(text("Take the PIN off").size(12))
                    .padding([3, 8])
                    .style(style::ghost)
                    .on_press(Message::ClearSettingsPin),
            ]
            .align_y(Alignment::Center),
        )
    } else {
        text("No PIN: anyone at this computer can switch to it.")
            .size(12)
            .style(style::muted)
            .into()
    });
    let you = column![
        row![
            badge(me, 52.0),
            column![
                text_input("Name", &app.profile_name)
                    .on_input(Message::ProfileName)
                    .on_submit(Message::SaveSettings)
                    .padding(8),
                text(format!(
                    "{} · {}",
                    me.kind.map_or("Not set up yet", Kind::label),
                    if me.pin.is_some() { "PIN-locked" } else { "No PIN" }
                ))
                .size(12)
                .style(style::muted),
            ]
            .spacing(4)
            .width(Length::Fill),
        ]
        .spacing(14)
        .align_y(Alignment::Center),
        column![text("Part of the shop").font(style::BOLD), kinds].spacing(6),
        column![text("PIN").font(style::BOLD), pin].spacing(6),
        employee(app),
        from_xtime(app),
    ]
    .spacing(16);

    // everyone's
    let admin = app.admin.unlocked;
    let mut list = Column::new().spacing(6);
    for p in &app.profiles.list {
        let here = p.id == app.profiles.active;
        let first = p.id == crate::profiles::FIRST;
        let mut facts = vec![
            p.kind.map_or("Not set up yet", Kind::label).to_string(),
            if p.pin.is_some() { "PIN-locked" } else { "No PIN" }.to_string(),
        ];
        match (here, p.last_used) {
            (true, _) => facts.push("in use now".into()),
            (false, Some(t)) => facts.push(format!("last used {}", when(t))),
            (false, None) => {}
        }
        if let Some(t) = p.added() {
            facts.push(format!("added {}", when(t)));
        }
        let mut actions = row![].spacing(6).align_y(Alignment::Center);
        if here {
            actions = actions.push(super::pill("In use", [0x1a, 0x5f, 0xb4]));
        } else {
            actions = actions.push(
                button(text("Switch").size(12))
                    .padding([4, 12])
                    .style(button::primary)
                    .on_press(Message::PickProfile(p.id.clone())),
            );
        }
        if admin {
            if p.pin.is_some() {
                actions = actions.push(
                    button(text("Reset PIN").size(12))
                        .padding([4, 10])
                        .style(style::ghost)
                        .on_press(Message::AdminResetPin(p.id.clone())),
                );
            }
            let asked = app.admin.confirm_delete.as_deref() == Some(p.id.as_str());
            actions = actions.push(
                button(text(if asked { "Really delete?" } else { "Delete" }).size(12))
                    .padding([4, 10])
                    .style(if asked { button::danger } else { style::ghost })
                    .on_press_maybe((!here && !first).then(|| Message::AdminDelete(p.id.clone()))),
            );
        } else if !here {
            actions = actions.push(
                button(text("Take off the list").size(12))
                    .padding([4, 10])
                    .style(style::ghost)
                    .on_press(Message::RemoveProfile(p.id.clone())),
            );
        }
        let mut item = column![
            row![
                badge(p, 36.0),
                column![
                    text(p.name.as_str()).font(style::BOLD),
                    text(facts.join("  ·  ")).size(11).style(style::muted)
                ]
                .spacing(2)
                .width(Length::Fill),
                actions,
            ]
            .spacing(12)
            .align_y(Alignment::Center)
        ]
        .spacing(6);
        // its PIN, asked for right under it
        if let Some(ProfileAsk::Pin { id, typed, wrong }) = &app.profile_ask {
            if *id == p.id {
                item = item.push(pin_field(typed, *wrong));
            }
        }
        list = list.push(container(item).padding([8, 10]).width(Length::Fill).style(style::card));
    }
    let add: Element<'_, Message> = match &app.profile_ask {
        Some(ProfileAsk::Add { name, number, pin, error }) => container(add_form(name, number, pin, error.as_deref()))
            .padding(10)
            .style(style::card)
            .into(),
        _ => button(text("+ Add a profile"))
            .padding([8, 12])
            .style(style::ghost)
            .on_press(Message::AddProfileStart)
            .into(),
    };
    let mut everyone = column![list, add].spacing(10);
    if let Some(n) = &app.admin.note {
        everyone = everyone.push(text(n.as_str()).size(13).font(style::BOLD));
    }
    everyone = everyone.push(
        text(if admin {
            "Deleting a profile deletes its folder too: its hours, ROs, settings and sign-ins on this computer (anything \
             synced stays on the sync server)."
        } else {
            "Each has its own hours, ROs, sign-ins and settings. Taking one off the list keeps its files."
        })
        .size(12)
        .style(style::muted),
    );

    column![
        section("Your profile", None, you),
        section(
            "Dealership",
            Some("The same for everyone at this dealership: new profiles start with them."),
            dealership(app),
        ),
        section("Sync", None, sync(app)),
        section("Everyone on this computer", None, everyone),
    ]
    .spacing(22)
    .into()
}
