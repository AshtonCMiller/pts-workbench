//! PTS Workbench: the app. Started by the launcher (crates/launcher), which picks the newest working version.
//! Everything the app draws is iced; web views (wry) come later for sign-ins and Ford's documents.
//!
//!   app.rs        state, messages, what each message changes
//!   clock.rs      the time, for what the app shows
//!   syncing.rs    sync between your computers (crates/sync): when, what came in, ROs opened / closed elsewhere
//!   updates.rs    the app updating itself from GitHub releases (Beta updates too), for the launcher to start
//!   timecard.rs   your hours (crates/hours) in the app: the file, bringing over the old app's, an RO's lines
//!   settings.rs   saved settings (first setup asks for the store ID and dealer code)
//!   signin.rs     signing in to Xtime and Ford PTS (the sign-in window, crates/signin)
//!   system_theme.rs  whether Windows is set to dark (the Auto appearance)
//!   workspace.rs  open vehicles and their tabs
//!   whats_new.rs  "What's new" (whats-new.md): after an update, when one's waiting, from Settings
//!   vehicles.rs   the vehicles open, kept between starts (the Electron app's workspaces.json) and for sync
//!   print/        printing inside the app (the spiff sheet): Windows' Print window, then drawn on the printer
//!   ros.rs        your ROs from Xtime (loaded every minute while signed in)
//!   reports.rs    the Feedback screen: sending bug reports and ideas, their replies (the feedback crate asks)
//!   sample.rs     sample ROs until Xtime is connected
//!   ui/           the screens: top bar, sidebar, dashboard, workbench, settings; style.rs the colors
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // no console window on Windows

mod app;
mod clock;
mod diag;
mod notes;
mod print;
mod reports;
mod ros;
mod sample;
mod settings;
mod signin;
mod syncing;
mod system_theme;
mod timecard;
mod ui;
mod updates;
mod vehicles;
mod whats_new;
mod workspace;

fn main() -> iced::Result {
    let ran = iced::application(app::App::title, app::App::update, app::App::view)
        .theme(app::App::theme)
        .subscription(app::App::subscription)
        .window(iced::window::Settings {
            size: iced::Size::new(1400.0, 900.0),
            // the title bar and taskbar (assets/icon.svg drawn at 256 px)
            icon: iced::window::icon::from_file_data(include_bytes!("../../../assets/icon-256.png"), None).ok(),
            ..Default::default()
        })
        .run_with(app::App::new);
    // "Restart to update": the launcher starts the new version when the app exits with this
    match updates::exit_code() {
        0 => ran,
        code => std::process::exit(code),
    }
}
