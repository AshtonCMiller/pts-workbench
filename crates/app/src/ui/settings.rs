//! First setup (the store ID and dealer code, before anything else) and Settings.
//!
//! Settings is laid out as Chrome's: its tabs down the left (the highlight sliding between them), the tab's page on the
//! right (springing in when it changes). Profiles (ui/profiles.rs `settings_tab`), Appearance, Linked accounts, About
//! Workbench (the version, updates and their channel, what's new, admin mode).

use iced::Task;
use iced::widget::{
    Column, button, checkbox, column, container, horizontal_space, pick_list, row, text, text_input, toggler, vertical_space,
};
// scrolling that glides (crates/ui-kit)
use iced::{Alignment, Color, Element, Length};
use ui_kit::scroll::scrollable;

use super::{anim, pill, style};
use crate::app::{App, Field, Message, SettingsTab, admin_field_id};
use crate::settings::{Graphics, ThemeChoice};
use crate::signin::Site;
use crate::updates::{ANY_BETA, Channel, STABLE, State};

/// Each field's name, for moving to it
fn field_id(f: Field) -> text_input::Id {
    text_input::Id::new(match f {
        Field::StoreId => "store-id",
        Field::DealerCode => "dealer-code",
        Field::EmployeeNumber => "employee-number",
    })
}

/// Puts the cursor in the form's first field (when first setup opens)
pub fn focus_first_field() -> Task<Message> {
    text_input::focus(field_id(Field::StoreId))
}

pub(super) fn field<'a>(label: &'a str, help: &'a str, value: &'a str, f: Field) -> Element<'a, Message> {
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

/// First setup's fields: the dealership's, and whose profile it is
fn fields(app: &App) -> Element<'_, Message> {
    column![dealership(app), employee(app), from_xtime(app)].spacing(14).into()
}

/// The dealership's store ID and dealer code (the same for everyone here)
pub(super) fn dealership(app: &App) -> Element<'_, Message> {
    let d = &app.draft;
    column![
        field("Store ID", "Your dealership's Xtime store ID.", &d.store_id, Field::StoreId),
        field("Dealer code", "Your dealership's Ford P&A code.", &d.dealer_code, Field::DealerCode),
    ]
    .spacing(14)
    .into()
}

pub(super) fn employee(app: &App) -> Element<'_, Message> {
    field(
        "Employee number",
        "Your Xtime employee number: your name and ROs are found with it.",
        &app.draft.employee_number,
        Field::EmployeeNumber,
    )
}

/// Who Xtime says you are (found from your employee number; not typed in)
pub(super) fn from_xtime(app: &App) -> Element<'_, Message> {
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
pub(super) fn sync(app: &App) -> Element<'_, Message> {
    let on = !app.settings.sync_off;
    column![
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

/// A part of a tab's page: its heading, and what's in it on a panel
pub(super) fn section<'a>(title: &'a str, about: Option<&'a str>, body: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let mut c = column![text(title).size(15).font(style::BOLD)].spacing(4);
    if let Some(a) = about {
        c = c.push(text(a).size(12).style(style::muted));
    }
    c.push(container(body).padding(14).width(Length::Fill).style(style::panel))
        .spacing(8)
        .into()
}

// ---------- Appearance ----------

/// Light or dark (Auto: as Windows is), and how the window's drawn. Both are kept the moment they're picked.
fn appearance(app: &App) -> Element<'_, Message> {
    let theme = app.draft.theme;
    let themes = anim::tabs(
        ThemeChoice::ALL.iter().position(|t| *t == theme).unwrap_or(0),
        ThemeChoice::ALL
            .iter()
            .map(|&t| {
                button(text(t.to_string()).size(13))
                    .padding([6, 18])
                    .style(style::tab(t == theme))
                    .on_press(Message::SetTheme(t))
                    .into()
            })
            .collect(),
    )
    .spacing(2.0);
    let theme_part = column![
        themes,
        text(match theme {
            ThemeChoice::Auto => "Follows Windows: light or dark as it is, changing when it does.",
            ThemeChoice::Light => "Always light.",
            ThemeChoice::Dark => "Always dark.",
        })
        .size(12)
        .style(style::muted),
    ]
    .spacing(8);

    let graphics = app.draft.graphics;
    let choice = |g: Graphics, title: &'static str, about: &'static str| {
        button(
            column![text(title).font(style::BOLD), text(about).size(12).style(style::muted)]
                .spacing(2)
                .width(Length::Fill),
        )
        .padding([10, 12])
        .width(Length::Fill)
        .style(style::list_item(graphics == g))
        .on_press(Message::SetGraphics(g))
    };
    let mut graphics_part = column![
        choice(
            Graphics::Standard,
            "Standard",
            "Draws with the graphics card: smooth, and easy on the computer."
        ),
        choice(
            Graphics::Compatible,
            "Compatible",
            "Draws without the graphics card: a little slower, but never flickers. Pick it if the whole window flickers."
        ),
    ]
    .spacing(6);
    if graphics != app.started_graphics {
        graphics_part = graphics_part.push(
            text("Changes the next time Workbench starts: close it and open it again.")
                .size(12)
                .font(style::BOLD),
        );
    }
    column![
        section("Theme", None, theme_part),
        section("Graphics", Some("How the window is drawn."), graphics_part),
    ]
    .spacing(22)
    .into()
}

// ---------- Linked accounts ----------

/// Xtime and Ford PTS: signed in or not, and signing in (again)
fn accounts(app: &App) -> Element<'_, Message> {
    let mut list = Column::new().spacing(10);
    for site in Site::ALL {
        let (state, color, again) = if app.signing_in == Some(site) {
            ("Signing in…", [0x1a, 0x5f, 0xb4], "Sign in")
        } else if app.signed_in(site) {
            ("Signed in", [0x15, 0x80, 0x3d], "Sign in again")
        } else {
            ("Not signed in", [0x6b, 0x72, 0x80], "Sign in")
        };
        let about = match site {
            Site::Xtime => "Your ROs, Xtime's dashboard and each RO's Xtime View.",
            _ => "The Service Manual, OASIS, bulletins, wiring and Diagnostics.",
        };
        list = list.push(
            container(
                row![
                    column![
                        row![text(site.name()).size(15).font(style::BOLD), pill(state, color)]
                            .spacing(8)
                            .align_y(Alignment::Center),
                        text(about).size(12).style(style::muted),
                    ]
                    .spacing(4)
                    .width(Length::Fill),
                    button(text(again).size(13).wrapping(text::Wrapping::None))
                        .padding([6, 14])
                        .style(if app.signed_in(site) { style::ghost } else { button::primary })
                        .on_press_maybe(app.signing_in.is_none().then_some(Message::SignIn(site))),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding(14)
            .width(Length::Fill)
            .style(style::panel),
        );
    }
    column![
        text("Workbench signs in to these for you, with this profile's own sign-ins.")
            .size(13)
            .style(style::muted),
        list,
        text("Signing in opens Xtime's or Ford's own sign-in page in a small window. Passwords stay with Windows' own password store, never in Workbench's files.")
            .size(12)
            .style(style::muted),
    ]
    .spacing(14)
    .into()
}

// ---------- About Workbench ----------

/// One choice in the update channel list
#[derive(Debug, Clone, PartialEq, Eq)]
struct ChannelChoice {
    id: String,
    label: String,
}

impl std::fmt::Display for ChannelChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

/// Stable, the newest beta of any, then each public beta channel (newest first); the one picked even when it's no
/// longer offered
fn channel_choices(app: &App) -> Vec<ChannelChoice> {
    let mut out = vec![
        ChannelChoice {
            id: STABLE.into(),
            label: "Stable (recommended)".into(),
        },
        ChannelChoice {
            id: ANY_BETA.into(),
            label: "Newest beta (any channel)".into(),
        },
    ];
    let listed: &[Channel] = match &app.channels {
        Some(Ok(list)) => list,
        _ => &[],
    };
    for c in listed {
        out.push(ChannelChoice {
            id: c.id.clone(),
            label: format!("{} beta · v{}", c.name, c.version),
        });
    }
    let mine = &app.updates.channel;
    if !out.iter().any(|c| &c.id == mine) {
        out.push(ChannelChoice {
            id: mine.clone(),
            label: format!("{mine} beta (no longer offered)"),
        });
    }
    out
}

fn about(app: &App) -> Element<'_, Message> {
    let u = &app.updates;

    // the version and how updates are going
    let mut actions = row![
        button(text("Check now").size(13))
            .padding([6, 14])
            .style(style::ghost)
            .on_press_maybe((u.on() && !u.busy()).then_some(Message::CheckUpdates)),
        button(text("What's new").size(13))
            .padding([6, 14])
            .style(style::ghost)
            .on_press(Message::ShowWhatsNew),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if matches!(u.state, State::Ready(_)) {
        actions = actions.push(
            button(text("Restart to update").size(13).font(style::BOLD))
                .padding([6, 14])
                .style(button::primary)
                .on_press(Message::RestartToUpdate),
        );
    }
    let (dot, busy) = match &u.state {
        State::Ready(_) => (Color::from_rgb8(0x1a, 0x5f, 0xb4), false),
        State::Checking | State::Downloading(_) => (Color::from_rgb8(0x1a, 0x5f, 0xb4), true),
        State::Failed(_) => (Color::from_rgb8(0xd6, 0x45, 0x45), false),
        State::Off(_) => (Color::from_rgb8(0x9c, 0xa3, 0xaf), false),
        _ => (Color::from_rgb8(0x16, 0xa3, 0x4a), false),
    };
    let version = column![
        row![
            text("Workbench").size(24).font(style::BOLD),
            text(format!("v{}", u.running)).size(15).style(style::muted),
        ]
        .spacing(10)
        .align_y(Alignment::End),
        row![anim::dot(dot, busy), text(u.describe()).size(13)]
            .spacing(8)
            .align_y(Alignment::Center),
        actions,
    ]
    .spacing(10);

    // which updates
    let choices = channel_choices(app);
    let picked = choices.iter().find(|c| c.id == u.channel).cloned();
    let mut channel = column![
        pick_list(choices, picked, |c: ChannelChoice| Message::SetChannel(c.id))
            .padding(8)
            .width(Length::Fill),
        text(if u.channel == STABLE {
            "Tested versions only. Best for everyday work."
        } else if u.channel == ANY_BETA {
            "The newest beta from any channel: new features first, less tested."
        } else {
            "This channel's betas: try a change before it's out for everyone. When a regular version newer than the beta comes out, Workbench goes back to Stable."
        })
        .size(12)
        .style(style::muted),
    ]
    .spacing(8);
    match &app.channels {
        None => channel = channel.push(text("Getting the beta channels…").size(12).style(style::muted)),
        Some(Err(e)) => channel = channel.push(text(format!("Couldn't get the beta channels: {e}")).size(12)),
        Some(Ok(_)) => {}
    }
    if !u.on() {
        channel = channel.push(
            text("Updates are off here, so this changes nothing yet.")
                .size(12)
                .style(style::muted),
        );
    }

    column![
        container(version).padding(16).width(Length::Fill).style(style::panel),
        section(
            "Update channel",
            Some("Which versions this computer updates to (the same for every profile here)."),
            channel,
        ),
        section(
            "Admin mode",
            Some("For whoever looks after Workbench here: resetting a forgotten PIN, deleting a profile."),
            admin(app),
        ),
    ]
    .spacing(22)
    .into()
}

/// Admin mode's switch: turned on, it asks for the Feedback password right here; while it's on, Profiles has each
/// profile's Reset PIN and Delete
fn admin(app: &App) -> Element<'_, Message> {
    let a = &app.admin;
    let mut c = column![
        row![
            text(if a.unlocked {
                "On: Profiles can reset a PIN or delete a profile"
            } else {
                "Off"
            })
            .size(13)
            .width(Length::Fill),
            toggler(a.unlocked || a.asking).on_toggle(move |on| if on {
                Message::AdminAsk
            } else if a.unlocked {
                Message::AdminLock
            } else {
                Message::AdminCancel
            }),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    ]
    .spacing(10);
    if a.asking && !a.unlocked {
        c = c.push(
            row![
                text_input("The Feedback password", &a.key)
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
                button(text("Cancel"))
                    .padding([7, 12])
                    .style(style::ghost)
                    .on_press(Message::AdminCancel),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
        c = c.push(
            text("It's checked with the feedback server. Admin mode stays on until it's switched off, the profile's switched or Workbench closes.")
                .size(12)
                .style(style::muted),
        );
        if let Some(e) = &a.error {
            c = c.push(text(format!("Couldn't unlock: {e}")).size(13));
        }
    }
    c.into()
}

// ---------- the card ----------

/// The first screen on a new computer
pub fn setup(app: &App) -> Element<'_, Message> {
    let card = container(
        column![
            text("Welcome to Workbench").size(22).font(style::BOLD),
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

/// Settings, as a card over everything (from the top bar): its tabs down the left, the tab's page on the right
pub fn card(app: &App) -> Element<'_, Message> {
    let tab = app.settings_tab;

    // the tabs, the highlight sliding up and down between them
    let nav = column![
        text("Settings").size(20).font(style::BOLD),
        anim::tabs(
            SettingsTab::ALL.iter().position(|t| *t == tab).unwrap_or(0),
            SettingsTab::ALL
                .iter()
                .map(|&t| {
                    button(text(t.label()).size(14))
                        .padding([9, 14])
                        .width(Length::Fill)
                        .style(style::tab(t == tab))
                        .on_press(Message::SettingsTab(t))
                        .into()
                })
                .collect(),
        )
        .vertical()
        .spacing(2.0)
        .radius(8.0),
        vertical_space(),
        text(format!("Workbench v{}", app.updates.running)).size(11).style(style::muted),
    ]
    .spacing(18)
    .padding(20)
    .width(220)
    .height(Length::Fill);
    let nav = container(nav).height(Length::Fill).style(|t: &iced::Theme| container::Style {
        background: Some(style::colors(t).hover.into()),
        ..Default::default()
    });

    // the tab's page, springing in when the tab changes
    let page: Element<'_, Message> = match tab {
        SettingsTab::Profiles => super::profiles::settings_tab(app),
        SettingsTab::Appearance => appearance(app),
        SettingsTab::Accounts => accounts(app),
        SettingsTab::About => about(app),
    };
    let head = row![
        text(tab.label()).size(20).font(style::BOLD),
        horizontal_space(),
        button(text("Close"))
            .padding([5, 12])
            .style(style::ghost)
            .on_press(Message::CloseSettings),
    ]
    .align_y(Alignment::Center);
    let body = scrollable(container(page).padding(iced::Padding {
        top: 0.0,
        right: 14.0,
        bottom: 8.0,
        left: 0.0,
    }))
    .height(Length::Fill)
    .remember(format!("settings:{tab:?}"));
    // (Profiles has fields kept with Save; the other tabs keep what's picked straight away)
    let foot: Element<'_, Message> = if tab == SettingsTab::Profiles {
        row![
            notice(app),
            horizontal_space(),
            button(text("Save").font(style::BOLD))
                .padding([8, 20])
                .style(button::primary)
                .on_press(Message::SaveSettings),
        ]
        .align_y(Alignment::Center)
        .into()
    } else {
        notice(app)
    };
    let right = column![head, anim::page(body, anim::Phase::Entering(app.settings_since)), foot]
        .spacing(16)
        .padding(24)
        .width(Length::Fill)
        .height(Length::Fill);

    container(row![nav, right].height(Length::Fill))
        .width(Length::Fill)
        .max_width(920)
        .height(Length::Fill)
        .max_height(660)
        .style(style::floating_card)
        .into()
}
