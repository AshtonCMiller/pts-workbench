//! The app's screens. `view` puts them together: the top bar, then (with the sidebar and chat on either side) your
//! Xtime dashboard or the Workbench. First setup and Settings fill the window instead.
//!
//!   topbar.rs     Xtime / Workbench / Diagnostics, Chat, Settings (My ROs' button is beside it: sidebar.rs)
//!   bottombar.rs  the shift clock, Xtime's state and the latest message, Feedback
//!   sidebar.rs    open vehicles and your ROs
//!   notes.rs      Notifications: the list, and the pop-up for a new one
//!   workbench.rs  one vehicle: its tabs and the Service Manual's tabs
//!   settings.rs   first setup and Settings
//!   feedback.rs   Feedback: send a bug or an idea, your reports, everyone's (the admin)
//!   print_window.rs  the app's Print window: a preview of every page, the printer, copies
//!   profiles.rs   the profile switcher (top right), a new profile's kind, Settings' Profile part
//!
//! The colours, styles, animations and pills are in crates/ui-kit (`ui::style`, `ui::anim` and `ui::pill` here); the
//! Xtime View, its inspection form, the Add / Edit Service window, the dashboard and My ROs in crates/xtime-ui; the
//! hours report in crates/hours-ui.

// the shared look and motion (crates/ui-kit), under their usual names here
pub use ui_kit::{anim, pill, style};

mod bottombar;
mod feedback;
pub mod lines;
mod notes;
mod phone;
mod print_window;
mod profiles;
mod settings;
mod sidebar;
mod topbar;
mod whats_new;
mod workbench;

pub use settings::focus_first_field;

use iced::widget::{button, column, container, row, text};
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
                Mode::Xtime => anim::page(xtime_page(app), phase).into(),
                Mode::Workbench => anim::page(workbench::view(app), phase).into(),
                Mode::Diagnostics => anim::page(diag_ui::view(&app.diag, app.diag_ro()).map(Message::Vci), phase).into(),
            };
            // My ROs and Chat slide in and out of their sides (My ROs' button right beside it, moving with it)
            row![
                anim::side(sidebar::view(app), app.sidebar, anim::Edge::Left, sidebar::WIDTH),
                sidebar::handle(app),
                main,
                anim::side(chat(app), app.chat, anim::Edge::Right, CHAT_WIDTH),
                anim::side(notes::panel(app), app.notes_open, anim::Edge::Right, notes::WIDTH),
            ]
            .height(Length::Fill)
            .into()
        }
    };
    let page = container(column![topbar::view(app), body, bottombar::view(app)])
        .style(style::page)
        .width(Length::Fill)
        .height(Length::Fill);
    // a new notification, popped up in the corner for a few seconds
    let toast = notes::toast(app).unwrap_or_else(nothing);
    // the profile switcher's menu, dropped from the top right
    let menu = if app.profile_menu && app.overlay.is_none() {
        profiles::menu(app)
    } else {
        nothing()
    };
    // Settings, Feedback, Print, Hours or Service: a card over everything, the page frosted behind it
    let over: Element<'_, Message> = match app.overlay {
        Some((which, phase)) => {
            let (card, close) = match which {
                Overlay::Settings => (settings::card(app), Message::CloseSettings),
                Overlay::Feedback => (feedback::card(app), Message::CloseFeedback),
                Overlay::Print => (print_window::card(app), Message::ClosePrint),
                Overlay::WhatsNew => (whats_new::card(app), Message::CloseWhatsNew),
                // (answered, not put away)
                Overlay::ProfileKind => (profiles::kind_card(app), Message::Ignore),
                Overlay::Phone => (phone::card(app), Message::ClosePhone),
                Overlay::Hours => (
                    hours_ui::card(
                        &app.hours_view,
                        &app.hours,
                        app.pay.as_ref(),
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
            anim::modal(card, phase, close).into()
        }
        None => nothing(),
    };
    // (always the same layers, the page always the first: iced keeps what each widget remembers, such as how far each
    // page is scrolled, only while it stays in the same place among them, so nothing popping up moves the page)
    iced::widget::stack![page, toast, menu, over].into()
}

/// An empty layer
fn nothing<'a>() -> Element<'a, Message> {
    iced::widget::Space::new(0, 0).into()
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
        signed_out: !app.signed_in(crate::signin::Site::Xtime),
        now: app.now,
    };
    container(xtime_ui::chat::panel(&app.chat_panel, shown).map(Message::Chat))
        .width(CHAT_WIDTH)
        .height(Length::Fill)
        .into()
}

/// The Xtime page: the app's dashboard of your ROs (crates/xtime-ui), or Xtime's own dashboard (the website, inside
/// the app), switched at the top
fn xtime_page(app: &App) -> Element<'_, Message> {
    let site = app.settings.xtime_site;
    let pick = row![
        button(text("My ROs").size(12))
            .padding([4, 12])
            .style(style::toggle(!site))
            .on_press(Message::XtimeSite(false)),
        button(text("Xtime's dashboard").size(12))
            .padding([4, 12])
            .style(style::toggle(site))
            .on_press(Message::XtimeSite(true)),
    ]
    .spacing(4);
    let mut bar = iced::widget::Row::new()
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .push(pick)
        .push(iced::widget::horizontal_space());
    if site {
        bar = bar.push(
            button(text("Reload").size(12))
                .padding([4, 10])
                .style(style::ghost)
                .on_press(Message::ReloadPane),
        );
    }
    let body: Element<'_, Message> = if site {
        workbench::site_room(app, "xtime-dashboard".into())
    } else {
        xtime_ui::my_ros::dashboard(&app.my_ros, &app.search).map(Message::from)
    };
    column![container(bar).padding([6, 12]).width(Length::Fill), body]
        .height(Length::Fill)
        .into()
}
