//! `pts-signin`: a sign-in window. Opens a site's own sign-in in a web view, and once you're back on the site,
//! prints its cookies as one line of JSON and closes:
//!
//!   pts-signin --title <window title> --start <address> --done-host <host> --domains <a,b> --data-dir <folder>
//!
//! Prints `{"ok":true,"cookies":[…]}` (each like `workbench_net::SiteCookie`), or `{"ok":false,"cancelled":true}` when
//! the window is closed first (exit code 1). Only cookies of the `--domains` sites are printed, and only to the app
//! that started it (its standard output); nothing is written here except the web view's own profile in `--data-dir`,
//! which keeps "remember me" and saved passwords between sign-ins, as a browser does.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // no console window on Windows

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::{PageLoadEvent, WebContext, WebViewBuilder};

/// Must match `workbench_net::SiteCookie` (the app reads these)
#[derive(Serialize)]
struct SiteCookie {
    name: String,
    value: String,
    domain: String,
    path: String,
    secure: bool,
    http_only: bool,
    expires: Option<i64>,
}

#[derive(Serialize)]
struct Answer {
    ok: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    cancelled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    cookies: Vec<SiteCookie>,
}

fn answer(a: &Answer) {
    println!("{}", serde_json::to_string(a).unwrap_or_default());
}

struct Args {
    title: String,
    start: String,
    done_host: String,
    domains: Vec<String>,
    data_dir: PathBuf,
}

fn args() -> Option<Args> {
    let list: Vec<String> = std::env::args().skip(1).collect();
    let get = |name: &str| list.iter().position(|a| a == name).and_then(|i| list.get(i + 1)).cloned();
    Some(Args {
        title: get("--title").unwrap_or_else(|| "Sign in".into()),
        start: get("--start")?,
        done_host: get("--done-host")?.to_ascii_lowercase(),
        domains: get("--domains")?
            .split(',')
            .map(|d| d.trim().trim_start_matches('.').to_ascii_lowercase())
            .filter(|d| !d.is_empty())
            .collect(),
        data_dir: PathBuf::from(get("--data-dir")?),
    })
}

/// The host of an address, in small letters
fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    rest.split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Back on the site (not on one of its sign-in pages)
fn signed_in(url: &str, done_host: &str) -> bool {
    let path = url
        .split_once("://")
        .map(|(_, r)| r)
        .unwrap_or(url)
        .split_once('/')
        .map(|(_, p)| p.to_ascii_lowercase())
        .unwrap_or_default();
    host_of(url) == done_host
        && !["login", "signin", "sign-in", "logon", "account/log"]
            .iter()
            .any(|w| path.contains(w))
}

fn wanted(domain: &str, domains: &[String]) -> bool {
    let d = domain.trim_start_matches('.').to_ascii_lowercase();
    domains.iter().any(|w| d == *w || d.ends_with(&format!(".{w}")))
}

enum UserEvent {
    /// A page finished loading
    Loaded(String),
    /// A moment after landing on the site: still there? (Sites can load first and send you to sign in after.)
    Check,
}

fn main() {
    let Some(a) = args() else {
        answer(&Answer {
            ok: false,
            cancelled: false,
            error: Some("missing arguments".into()),
            cookies: vec![],
        });
        std::process::exit(2);
    };

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let window = match WindowBuilder::new()
        .with_title(&a.title)
        .with_inner_size(LogicalSize::new(560.0, 760.0))
        .build(&event_loop)
    {
        Ok(w) => w,
        Err(e) => {
            answer(&Answer {
                ok: false,
                cancelled: false,
                error: Some(format!("couldn't open the window: {e}")),
                cookies: vec![],
            });
            std::process::exit(2);
        }
    };

    let proxy = event_loop.create_proxy();
    let mut context = WebContext::new(Some(a.data_dir.clone()));
    let builder = WebViewBuilder::with_web_context(&mut context)
        .with_url(&a.start)
        .with_on_page_load_handler(move |event, url| {
            if let PageLoadEvent::Finished = event {
                let _ = proxy.send_event(UserEvent::Loaded(url));
            }
        });
    // Linux: the web view goes in the window's GTK box (works on Wayland and X11); elsewhere in the window itself
    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(window.default_vbox().expect("the window's GTK box"))
    };
    #[cfg(not(target_os = "linux"))]
    let webview = builder.build(&window);
    let webview = match webview {
        Ok(w) => w,
        Err(e) => {
            answer(&Answer {
                ok: false,
                cancelled: false,
                error: Some(format!("couldn't open the web view: {e}")),
                cookies: vec![],
            });
            std::process::exit(2);
        }
    };

    let check_proxy = event_loop.create_proxy();
    let mut waiting = false;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        let _keep = (&window, &context); // the window and the web view's profile live as long as the web view
        match event {
            Event::UserEvent(UserEvent::Loaded(url)) if !waiting && signed_in(&url, &a.done_host) => {
                waiting = true;
                let p = check_proxy.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(2000));
                    let _ = p.send_event(UserEvent::Check);
                });
            }
            Event::UserEvent(UserEvent::Check) => {
                waiting = false;
                let here = webview.url().unwrap_or_default();
                if !signed_in(&here, &a.done_host) {
                    return; // sent on to sign in after all: wait for the next page
                }
                match webview.cookies() {
                    Ok(list) => {
                        let cookies = list
                            .iter()
                            .filter_map(|c| {
                                let domain = c.domain().map(str::to_string).unwrap_or_else(|| a.done_host.clone());
                                wanted(&domain, &a.domains).then(|| SiteCookie {
                                    name: c.name().to_string(),
                                    value: c.value().to_string(),
                                    domain,
                                    path: c.path().unwrap_or("/").to_string(),
                                    secure: c.secure().unwrap_or(false),
                                    http_only: c.http_only().unwrap_or(false),
                                    expires: c.expires_datetime().map(|t| t.unix_timestamp()),
                                })
                            })
                            .collect();
                        answer(&Answer {
                            ok: true,
                            cancelled: false,
                            error: None,
                            cookies,
                        });
                        *control_flow = ControlFlow::Exit;
                    }
                    Err(e) => {
                        answer(&Answer {
                            ok: false,
                            cancelled: false,
                            error: Some(format!("couldn't read the sign-in: {e}")),
                            cookies: vec![],
                        });
                        *control_flow = ControlFlow::ExitWithCode(2);
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                answer(&Answer {
                    ok: false,
                    cancelled: true,
                    error: None,
                    cookies: vec![],
                });
                *control_flow = ControlFlow::ExitWithCode(1);
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_when_youre_back_on_the_site() {
        let host = "www.inspect.app.coxautoinc.com";
        assert!(signed_in("https://www.inspect.app.coxautoinc.com/", host));
        assert!(signed_in("https://WWW.inspect.app.coxautoinc.com:443/Dashboard?x=1", host));
        assert!(!signed_in(
            "https://xtime.signin.coxautoinc.com/?return=https://www.inspect.app.coxautoinc.com/",
            host
        ));
        assert!(!signed_in("https://www.inspect.app.coxautoinc.com/Account/Login", host));
        assert!(!signed_in("about:blank", host));
    }

    #[test]
    fn takes_only_the_sites_cookies() {
        let d = vec!["coxautoinc.com".to_string(), "xtime.com".to_string()];
        assert!(wanted(".coxautoinc.com", &d));
        assert!(wanted("www.inspect.app.coxautoinc.com", &d));
        assert!(wanted("chatgateway.xtime.com", &d));
        assert!(!wanted("notcoxautoinc.com", &d));
        assert!(!wanted(".google.com", &d));
    }
}
