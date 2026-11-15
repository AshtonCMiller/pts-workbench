//! The app's screens. `view` puts them together: the top bar, then (with the sidebar and chat on either side) your
//! Xtime dashboard or the Workbench. First setup and Settings fill the window instead.
//!
//!   topbar.rs     Xtime / Workbench, Chat, My ROs, Settings
//!   bottombar.rs  the shift clock, Xtime's state and the latest message, Feedback
//!   sidebar.rs    open vehicles and your ROs
//!   dashboard.rs  your Xtime dashboard (a table of your ROs, with search)
//!   workbench.rs  one vehicle: its tabs and the Service Manual's tabs
//!   xtime_view.rs the Xtime View tab: an RO as Xtime has it, its lines, marking them done
//!   settings.rs   first setup and Settings
//!   feedback.rs   Feedback: send a bug or an idea, your reports, everyone's (the admin)
//!   hours_report.rs  your hours: one pay period's flags, confirmed, clocked, efficiency, spiffs
//!   print_window.rs  the app's Print window: a preview of every page, the printer, copies
//!   anim.rs       motion: tabs whose highlight slides, pages that slide and fade
//!   style.rs      colors and shared styles

pub mod anim;
pub mod style;

mod bottombar;
mod dashboard;
mod feedback;
pub mod hours_report;
mod print_window;
mod settings;
mod sidebar;
mod topbar;
mod workbench;
mod xtime_view;

pub use settings::focus_first_field;

use iced::widget::{column, container, row, text};
use iced::{Color, Element, Length};

use crate::app::{App, Message, Mode, Overlay, Screen};

pub fn view(app: &App) -> Element<'_, Message> {
    // the page shown (while one leaves, still the old one), sliding and fading in its transition
    let (screen, mode, _) = &app.outer.shown;
    let phase = app.outer.phase;
    let body: Element<'_, Message> = match screen {
        Screen::Setup => return anim::page(settings::setup(app), phase).into(),
        Screen::Main => {
            let main: Element<'_, Message> = match mode {
                Mode::Xtime => anim::page(dashboard::view(app), phase).into(),
                Mode::Workbench => anim::page(workbench::view(app), phase).into(),
            };
            // My ROs and Chat slide in and out of their sides
            row![
                anim::side(sidebar::view(app), app.sidebar, anim::Edge::Left, sidebar::WIDTH),
                main,
                anim::side(chat(), app.chat, anim::Edge::Right, CHAT_WIDTH),
            ]
            .height(Length::Fill)
            .into()
        }
    };
    let base = container(column![topbar::view(app), body, bottombar::view(app)])
        .style(style::page)
        .width(Length::Fill)
        .height(Length::Fill);
    // Settings, Feedback, Print or Hours: a card over everything, the page frosted behind it
    match app.overlay {
        Some((which, phase)) => {
            let (card, close) = match which {
                Overlay::Settings => (settings::card(app), Message::CloseSettings),
                Overlay::Feedback => (feedback::card(app), Message::CloseFeedback),
                Overlay::Print => (print_window::card(app), Message::ClosePrint),
                Overlay::Hours => (hours_report::card(app), Message::CloseHours),
            };
            iced::widget::stack![base, anim::modal(card, phase, close)].into()
        }
        None => base.into(),
    }
}

const CHAT_WIDTH: f32 = 340.0;

/// The Dealership chat panel (comes with the Xtime crate's chat)
fn chat<'a>() -> Element<'a, Message> {
    container(
        column![
            text("Dealership chat").size(16).font(style::BOLD),
            text("Your Xtime conversations show here once Xtime is connected.")
                .size(13)
                .style(style::muted),
        ]
        .spacing(8),
    )
    .padding(14)
    .width(CHAT_WIDTH)
    .height(Length::Fill)
    .style(style::panel)
    .into()
}

/// A small coloured label: an RO's mode, a waiting flag
pub fn pill<'a>(label: impl iced::widget::text::IntoFragment<'a>, rgb: [u8; 3]) -> Element<'a, Message> {
    container(text(label).size(11))
        .padding([1, 8])
        .style(style::pill(Color::from_rgb8(rgb[0], rgb[1], rgb[2])))
        .into()
}
