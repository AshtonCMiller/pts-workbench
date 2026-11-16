//! The app's screens. `view` puts them together: the top bar, then (with the sidebar and chat on either side) your
//! Xtime dashboard or the Workbench. First setup and Settings fill the window instead.
//!
//!   topbar.rs     Xtime / Workbench, Chat, My ROs, Settings
//!   bottombar.rs  the shift clock, Xtime's state and the latest message, Feedback
//!   sidebar.rs    open vehicles and your ROs
//!   workbench.rs  one vehicle: its tabs and the Service Manual's tabs
//!   settings.rs   first setup and Settings
//!   feedback.rs   Feedback: send a bug or an idea, your reports, everyone's (the admin)
//!   print_window.rs  the app's Print window: a preview of every page, the printer, copies
//!
//! The colours, styles, animations and pills are in crates/ui-kit (`ui::style`, `ui::anim` and `ui::pill` here); the
//! Xtime View, its inspection form, the Add / Edit Service window, the dashboard and My ROs in crates/xtime-ui; the
//! hours report in crates/hours-ui.

// the shared look and motion (crates/ui-kit), under their usual names here
pub use ui_kit::{anim, pill, style};

mod bottombar;
mod feedback;
pub mod lines;
mod print_window;
mod settings;
mod sidebar;
mod topbar;
mod whats_new;
mod workbench;

pub use settings::focus_first_field;

use iced::widget::{column, container, row, text};
use iced::{Element, Length};

use crate::app::{App, Message, Mode, Overlay, Screen};

pub fn view(app: &App) -> Element<'_, Message> {
    // the page shown (while one leaves, still the old one), sliding and fading in its transition
    let (screen, mode, _) = &app.outer.shown;
    let phase = app.outer.phase;
    let body: Element<'_, Message> = match screen {
        Screen::Setup => return anim::page(settings::setup(app), phase).into(),
        Screen::Main => {
            let main: Element<'_, Message> = match mode {
                // your ROs as Xtime's dashboard has them (crates/xtime-ui)
                Mode::Xtime => anim::page(xtime_ui::my_ros::dashboard(&app.my_ros, &app.search).map(Message::from), phase).into(),
                Mode::Workbench => anim::page(workbench::view(app), phase).into(),
            };
            // My ROs and Chat slide in and out of their sides
            row![
                anim::side(sidebar::view(app), app.sidebar, anim::Edge::Left, sidebar::WIDTH),
                main,
                anim::side(chat(app), app.chat, anim::Edge::Right, CHAT_WIDTH),
            ]
            .height(Length::Fill)
            .into()
        }
    };
    let base = container(column![topbar::view(app), body, bottombar::view(app)])
        .style(style::page)
        .width(Length::Fill)
        .height(Length::Fill);
    // Settings, Feedback, Print, Hours or Service: a card over everything, the page frosted behind it
    match app.overlay {
        Some((which, phase)) => {
            let (card, close) = match which {
                Overlay::Settings => (settings::card(app), Message::CloseSettings),
                Overlay::Feedback => (feedback::card(app), Message::CloseFeedback),
                Overlay::Print => (print_window::card(app), Message::ClosePrint),
                Overlay::WhatsNew => (whats_new::card(app), Message::CloseWhatsNew),
                Overlay::Hours => (
                    hours_ui::card(
                        &app.hours_view,
                        &app.hours,
                        app.now * 1000,
                        xtime::ro::SPIFF_OVER,
                        app.print_job.is_none(),
                    )
                    .map(Message::Hr),
                    Message::Hr(hours_ui::HoursMsg::Close),
                ),
                Overlay::Service => (
                    match &app.service {
                        Some(ed) => xtime_ui::service::card(ed).map(Message::Svc),
                        None => text("").into(),
                    },
                    Message::Svc(xtime_ui::service::SvcMsg::Close),
                ),
            };
            iced::widget::stack![base, anim::modal(card, phase, close)].into()
        }
        None => base.into(),
    }
}

const CHAT_WIDTH: f32 = 340.0;

/// The Dealership chat panel (crates/xtime-ui): with a vehicle showing, its RO can narrow the list
fn chat(app: &App) -> Element<'_, Message> {
    let ro = app
        .workspace()
        .filter(|_| app.mode == crate::app::Mode::Workbench)
        .map(|w| w.ro.ro.as_str());
    let shown = xtime_ui::chat::ChatShown {
        ro,
        my_ros: app.my_ros.iter().map(|r| r.ro.clone()).collect(),
        sample: app.sample_data,
        now: app.now,
    };
    container(xtime_ui::chat::panel(&app.chat_panel, shown).map(Message::Chat))
        .width(CHAT_WIDTH)
        .height(Length::Fill)
        .into()
}
