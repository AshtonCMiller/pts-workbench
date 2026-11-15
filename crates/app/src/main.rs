//! PTS Workbench: the app. Started by the launcher (crates/launcher), which picks the newest working version.
//! Everything the app draws is iced; web views (wry) come later for sign-ins and Ford's documents.
//!
//!   app.rs        state, messages, what each message changes
//!   clock.rs      the time, for what the app shows
//!   timecard.rs   your hours (crates/hours) in the app: the file, bringing over the old app's, an RO's lines
//!   settings.rs   saved settings (first setup asks for the store ID and dealer code)
//!   signin.rs     signing in to Xtime and Ford PTS (the sign-in window, crates/signin)
//!   system_theme.rs  whether Windows is set to dark (the Auto appearance)
//!   workspace.rs  open vehicles and their tabs
//!   print/        printing inside the app (the spiff sheet): Windows' Print window, then drawn on the printer
//!   ros.rs        your ROs from Xtime (loaded every minute while signed in)
//!   reports.rs    the Feedback screen: sending bug reports and ideas, their replies (the feedback crate asks)
//!   sample.rs     sample ROs until Xtime is connected
//!   ui/           the screens: top bar, sidebar, dashboard, workbench, settings; style.rs the colors
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // no console window on Windows

mod app;
mod clock;
mod print;
mod reports;
mod ros;
mod sample;
mod settings;
mod signin;
mod system_theme;
mod timecard;
mod ui;
mod workspace;

fn main() -> iced::Result {
    iced::application(app::App::title, app::App::update, app::App::view)
        .theme(app::App::theme)
        .subscription(app::App::subscription)
        .window_size((1400.0, 900.0))
        .run_with(app::App::new)
}
