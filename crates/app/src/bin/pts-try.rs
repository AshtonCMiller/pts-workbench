//! A command line to try the Xtime crate against the real site, using sign-in cookies saved to a file
//! (see `workbench_net::Http::save_cookies`). The app itself is `src/main.rs`.
//!
//!   pts-try --cookies <file> employee <employee number> <store id>
//!   pts-try --cookies <file> dashboard <technician id> [<store id>]
//!
//! The cookie file holds sign-ins: keep it in your own profile folder.

use std::path::PathBuf;
use std::process::ExitCode;

use workbench_net::Http;
use xtime::{Xtime, paths};

/// What the app tells Xtime and Ford it is (the same as a desktop browser)
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

const USAGE: &str = "usage:
  pts-try --cookies <file> employee <employee number> <store id>
  pts-try --cookies <file> dashboard <technician id> [<store id>]";

#[tokio::main]
async fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let cookies = match args.iter().position(|a| a == "--cookies") {
        Some(i) if i + 1 < args.len() => {
            let file = PathBuf::from(args.remove(i + 1));
            args.remove(i);
            Some(file)
        }
        _ => None,
    };
    let (Some(cookies), Some(command)) = (cookies, args.first().cloned()) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };

    let http = match Http::with_saved_cookies(USER_AGENT, &cookies) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let xtime = Xtime::new(http.clone(), paths::DATA_HOST).expect("Xtime's address");

    let result = match (command.as_str(), &args[1..]) {
        ("employee", [number, store]) => xtime
            .employee(number, store, paths::EMPLOYEES)
            .await
            .map(|e| serde_json::to_string_pretty(&e)),
        ("dashboard", [tech, rest @ ..]) => {
            let staff = rest
                .first()
                .map(|store| paths::EMPLOYEES.replace("{STORE}", store))
                .unwrap_or_default();
            xtime
                .dashboard(tech, paths::DASHBOARD, &staff)
                .await
                .map(|d| serde_json::to_string_pretty(&d))
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    // Xtime may have refreshed its cookies: keep them
    if let Err(e) = http.save_cookies(&cookies) {
        eprintln!("couldn't save the sign-ins: {e}");
    }
    match result {
        Ok(Ok(json)) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Ok(Err(e)) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}