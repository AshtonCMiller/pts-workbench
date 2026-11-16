//! The bottom bar: the shift clock and your hours, what's going on (Xtime's state, the latest message), Feedback, and
//! sync's state in the corner (a light that pulses while syncing; hover for more, click for Settings).
//! Keeps the top bar for getting around (Xtime / Workbench, Chat, My ROs, Settings).

use iced::widget::{button, container, horizontal_space, row, text, tooltip};
use iced::{Alignment, Color, Element, Length};

use super::{anim, pill, style};
use crate::app::{App, Message};
use crate::ros::{self, LoadError};
use crate::signin::Site;
use crate::syncing::Status;

pub fn view(app: &App) -> Element<'_, Message> {
    // the shift clock
    let clock: Element<'_, Message> = match crate::timecard::on_the_clock(&app.hours, app.now * 1000) {
        Some(on) => row![
            pill("Clocked in", [0x15, 0x80, 0x3d]),
            text(on).size(13),
            button(text("Clock out").size(13))
                .padding([3, 10])
                .style(style::ghost)
                .on_press(Message::ToggleClock),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
        None => row![
            pill("Clocked out", [0x5d, 0x66, 0x70]),
            button(text("Clock in").size(13))
                .padding([3, 10])
                .style(style::ghost)
                .on_press(Message::ToggleClock),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
    };

    // your hours: what's flagged this pay period, and the report
    let flagged = app.hours.report(&chrono::Local, app.now * 1000, 0, xtime::ro::SPIFF_OVER).total;
    let hours = button(text(format!("Hours · {flagged:.1} flagged")).size(13))
        .padding([3, 10])
        .style(style::ghost)
        .on_press(Message::OpenHours);

    let mut bar = row![clock, hours, divider()].spacing(12).align_y(Alignment::Center);

    // Xtime's state: signing in, signed out, loading, a problem, or when your ROs last loaded
    let sign_in = |label: &'static str| {
        row![
            pill(label, [0xb4, 0x23, 0x18]),
            button(text("Sign in").size(13).font(style::BOLD))
                .padding([3, 10])
                .style(button::primary)
                .on_press_maybe(app.signing_in.is_none().then_some(Message::SignIn(Site::Xtime))),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
    };
    if app.signing_in == Some(Site::Xtime) {
        bar = bar.push(text("Signing in to Xtime…").size(13).style(style::muted));
    } else if !app.signed_in(Site::Xtime) {
        bar = bar.push(sign_in("Xtime: not signed in"));
    } else {
        match &app.ros_error {
            Some(LoadError::SignedOut) => bar = bar.push(sign_in("Xtime: signed out")),
            Some(e @ (LoadError::NoEmployee | LoadError::NotFound(_))) => {
                bar = bar.push(
                    row![
                        pill(e.text(), [0xd9, 0x77, 0x06]),
                        button(text("Settings").size(13))
                            .padding([3, 10])
                            .style(style::ghost)
                            .on_press(Message::OpenSettings),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
            }
            Some(e @ LoadError::Problem(_)) => {
                bar = bar.push(
                    row![
                        pill(e.text(), [0xb4, 0x23, 0x18]),
                        button(text("Try again").size(13))
                            .padding([3, 10])
                            .style(style::ghost)
                            .on_press_maybe((!app.ros_loading).then_some(Message::LoadRos)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
            }
            None if app.ros_loading && app.sample_data => bar = bar.push(text("Loading your ROs from Xtime…").size(13).style(style::muted)),
            None => {
                if let Some(at) = app.ros_loaded_at {
                    bar = bar.push(
                        text(format!("Xtime · updated {}", ros::ago(app.now - at)))
                            .size(13)
                            .style(style::muted),
                    );
                }
            }
        }
        if app.sample_data && !app.ros_loading {
            bar = bar.push(pill("Sample data", [0xd9, 0x77, 0x06]));
        }
    }

    // the latest message ("Settings saved", a sign-in that failed…)
    if let Some(n) = &app.notice {
        bar = bar.push(
            row![
                text(n.as_str()).size(13),
                button(text("×"))
                    .padding([0, 6])
                    .style(button::text)
                    .on_press(Message::DismissNotice)
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
    }

    bar = bar.push(horizontal_space()).push(
        button(
            text(if app.is_admin() && app.reports.new_count > 0 {
                format!("Feedback  {}", app.reports.new_count)
            } else {
                "Feedback".to_string()
            })
            .size(13),
        )
        .padding([3, 10])
        .style(style::ghost)
        .on_press(Message::OpenFeedback),
    );
    bar = bar.push(sync(app));

    container(bar.padding([0, 12]))
        .height(36)
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .style(style::panel)
        .into()
}

/// Sync's light: green synced, blue (pulsing) syncing, amber a problem, gray off
fn sync(app: &App) -> Element<'_, Message> {
    let rgb = |r, g, b| Color::from_rgb8(r, g, b);
    let (color, label) = match (&app.sync.status, app.sync.running) {
        (Status::Off, _) => (rgb(0x9a, 0xa3, 0xad), "Sync off"),
        (_, true) | (Status::Syncing, _) => (rgb(0x3b, 0x82, 0xf6), "Syncing"),
        (Status::Synced(_), _) => (rgb(0x16, 0xa3, 0x4a), "Synced"),
        (Status::Failed(_), _) => (rgb(0xd9, 0x77, 0x06), "Sync problem"),
    };
    let light = button(
        row![
            anim::dot(color, app.sync.running || app.sync.status == Status::Syncing),
            text(label).size(13)
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .padding([3, 8])
    .style(style::ghost)
    .on_press(Message::OpenSettings);
    let tip = container(text(app.sync.describe(crate::timecard::now_ms())).size(12))
        .padding([4, 8])
        .style(style::floating_card);
    tooltip(light, tip, tooltip::Position::Top).gap(6.0).into()
}

/// A thin upright line between parts of the bar
fn divider<'a>() -> Element<'a, Message> {
    container(text(""))
        .width(1)
        .height(18)
        .style(|theme: &iced::Theme| container::Style {
            background: Some(style::colors(theme).line.into()),
            ..Default::default()
        })
        .into()
}
