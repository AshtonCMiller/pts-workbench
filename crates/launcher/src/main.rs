//! The launcher: the program people start. Shows a small loading window straight away, picks the app version to run
//! (see lib.rs), runs it, and closes the loading window once the app's window is up. Starts the app again for
//! "Restart to update". Anything given to the launcher is passed on to the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // no console window on Windows

mod splash;

use std::io::{BufRead, BufReader};
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

use launcher::{Choice, ENV_VERSION, READY_LINE, RESTART_CODE, choose, has_started, set_aside, updates_dir};
use splash::Splash;

/// The loading window closes by itself after this, whatever the app's doing
const SPLASH_AT_MOST: Duration = Duration::from_secs(30);

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let here = match std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        Some(d) => d,
        None => return ExitCode::FAILURE,
    };
    let updates = updates_dir();

    loop {
        let splash = Splash::show("PTS Workbench", "Loading the workbench…");
        let choice = choose(&here, &updates);
        let child = Command::new(choice.exe())
            .args(&args)
            .env(ENV_VERSION, choice.version())
            .stdout(Stdio::piped())
            .spawn();
        let status = match child {
            Ok(mut child) => {
                // the app says when its window is up: the loading window goes then (or after a while regardless)
                if let Some(out) = child.stdout.take() {
                    let s = splash.clone();
                    std::thread::spawn(move || {
                        for line in BufReader::new(out).lines().map_while(Result::ok) {
                            if line.trim() == READY_LINE {
                                s.close();
                            }
                        }
                    });
                }
                let s = splash.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(SPLASH_AT_MOST);
                    s.close();
                });
                child.wait()
            }
            Err(e) => Err(e),
        };
        splash.close();
        match (&choice, status) {
            // "Restart to update": pick again (the new version, if one was downloaded)
            (_, Ok(s)) if s.code() == Some(RESTART_CODE) => continue,
            // a downloaded version that didn't start, or failed before its window was up: set it aside, use the installed one
            (Choice::Downloaded { version, .. }, Err(e)) => set_aside(&updates, version, &e.to_string()),
            (Choice::Downloaded { version, .. }, Ok(s)) if !s.success() && !has_started(&updates, version) => {
                set_aside(&updates, version, &format!("exited with {s} before its window was up"));
            }
            // otherwise the app is done: so is the launcher
            (_, Ok(s)) => return ExitCode::from(s.code().unwrap_or(0).clamp(0, 255) as u8),
            (Choice::Installed { .. }, Err(_)) => return ExitCode::FAILURE,
        }
    }
}
