//! `pts-signin`: a sign-in window. Opens a site's own sign-in in a web view, and once you're back on the site,
//! prints its cookies as one line of JSON and closes:
//!
//!   pts-signin --title <window title> --start <address> --done-host <host> --domains <a,b> --data-dir <folder>
//!              [--then <address> --then-from <host>] [--done-cookie]
//!              [--hidden --timeout <seconds>] [--settle <seconds>] [--wait-cookie <name>]
//!
//! Done once the window is back on `--done-host` (and still there a moment later). For sites that land you somewhere
//! else after signing in: `--then-from` — when a page on that host (or under it) loads, go on to `--then` (`{DOMAIN}`
//! there: that page's host without its first part). `--done-cookie` — also done once `--done-host` has set a new
//! cookie (for a site shown inside another page, so the window itself never goes there).
//!
//! To set a site up rather than sign in (Ford PTS: opening a vehicle in PTS once, so its pages have what they need):
//! `--hidden` — the window stays out of sight unless a sign-in page comes up (then it shows, to sign in), and gives up
//! after `--timeout` seconds while it's still hidden. `--settle` — done only once a page on `--done-host` (other than
//! its /transport hand-over) has been loaded this long. `--wait-cookie` — done only once the site has set a cookie
//! by this name.
//!
//! The hosts and paths it went through (never the rest of the address, nor any cookie) are written to
//! `signin-log.txt` in `--data-dir`, to see where a sign-in got stuck.
//!
//! Prints `{"ok":true,"cookies":[…]}` (each like `workbench_net::SiteCookie`), or `{"ok":false,"cancelled":true}` when
//! the window is closed first (exit code 1). Only cookies of the `--domains` sites are printed, and only to the app
//! that started it (its standard output); nothing is written here except the web view's own profile in `--data-dir`,
//! which keeps "remember me" and saved passwords between sign-ins, as a browser does.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // no console window on Windows

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::{Icon, WindowBuilder};
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
    then: Option<String>,
    then_from: Option<String>,
    done_cookie: bool,
    hidden: bool,
    timeout: Option<Duration>,
    settle: Option<Duration>,
    wait_cookie: Option<String>,
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
        then: get("--then"),
        then_from: get("--then-from").map(|h| h.trim_start_matches('.').to_ascii_lowercase()),
        done_cookie: list.iter().any(|a| a == "--done-cookie"),
        hidden: list.iter().any(|a| a == "--hidden"),
        timeout: get("--timeout").and_then(|s| s.parse().ok()).map(Duration::from_secs),
        settle: get("--settle").and_then(|s| s.parse().ok()).map(Duration::from_secs),
        wait_cookie: get("--wait-cookie"),
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

/// The path of an address, in small letters (without ? or #)
fn path_of(url: &str) -> String {
    url.split_once("://")
        .map(|(_, r)| r)
        .unwrap_or(url)
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .split_once('/')
        .map(|(_, p)| format!("/{}", p.to_ascii_lowercase()))
        .unwrap_or_else(|| "/".into())
}

/// On a sign-in page (or nowhere yet)
fn on_signin_page(url: &str) -> bool {
    let host = host_of(url);
    let path = path_of(url);
    host.is_empty()
        || !url.contains("://")
        || url.starts_with("about:")
        || host.split('.').any(|l| l == "signin")
        || ["login", "signin", "sign-in", "logon", "account/log"]
            .iter()
            .any(|w| path.contains(w))
}

/// A page on `from` (or under it) that isn't a sign-in page, nor the page `--then` opens (login.<domain>)
fn go_on_from(url: &str, from: &str) -> bool {
    let host = host_of(url);
    (host == from || host.ends_with(&format!(".{from}"))) && !host.starts_with("login.") && !on_signin_page(url)
}

/// `--then` for the page at `url`: `{DOMAIN}` is its host without the first part
fn then_address(then: &str, url: &str) -> String {
    let host = host_of(url);
    let domain = host.split_once('.').map(|(_, d)| d).unwrap_or(&host);
    then.replace("{DOMAIN}", domain)
}

/// A cookie of `host` itself (`www.` aside), not of a wider domain
fn cookie_of(domain: &str, host: &str) -> bool {
    let d = domain.trim_start_matches('.').to_ascii_lowercase();
    d == host || format!("www.{d}") == host
}

fn wanted(domain: &str, domains: &[String]) -> bool {
    let d = domain.trim_start_matches('.').to_ascii_lowercase();
    domains.iter().any(|w| d == *w || d.ends_with(&format!(".{w}")))
}

enum UserEvent {
    /// A page finished loading
    Loaded(String),
    /// Every 1.5 s: signed in yet? (Done only when it's so twice in a row: sites can load first and send you to sign
    /// in after.)
    Check,
}

/// The `--done-host` cookies there are now, as name=value
fn done_host_cookies(webview: &wry::WebView, host: &str) -> HashSet<String> {
    webview
        .cookies()
        .map(|list| {
            list.iter()
                .filter(|c| c.domain().is_some_and(|d| cookie_of(d, host)))
                .map(|c| format!("{}={}", c.name(), c.value()))
                .collect()
        })
        .unwrap_or_default()
}

/// The sign-in's path, for `signin-log.txt` (hosts and paths only)
struct Log(Option<std::fs::File>);

impl Log {
    fn open(dir: &std::path::Path) -> Log {
        let _ = std::fs::create_dir_all(dir);
        Log(std::fs::File::create(dir.join("signin-log.txt")).ok())
    }
    fn line(&mut self, what: &str) {
        use std::io::Write;
        if let Some(f) = &mut self.0 {
            let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let _ = writeln!(f, "{secs} {what}");
        }
    }
    fn page(&mut self, what: &str, url: &str) {
        self.line(&format!("{what} {}{}", host_of(url), path_of(url)));
    }
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
        .with_visible(!a.hidden)
        // the app's icon (assets/icon.svg at 64 px, as plain RGBA so nothing has to read a PNG)
        .with_window_icon(Icon::from_rgba(include_bytes!("../../../assets/icon-64.rgba").to_vec(), 64, 64).ok())
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
    let builder = WebViewBuilder::with_web_context(&mut context).with_on_page_load_handler(move |event, url| {
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

    // What Inspect had before signing in (an old sign-in kept in the profile): only a new cookie means signed in
    let before = if a.done_cookie {
        done_host_cookies(&webview, &a.done_host)
    } else {
        HashSet::new()
    };
    let mut log = Log::open(&a.data_dir);
    log.page("start", &a.start);
    if let Err(e) = webview.load_url(&a.start) {
        log.line(&format!("couldn't open the start page: {e}"));
    }

    let check_proxy = event_loop.create_proxy();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(1500));
            if check_proxy.send_event(UserEvent::Check).is_err() {
                break; // the window has closed
            }
        }
    });
    let mut ready_checks = 0;
    let mut went_on: Option<Instant> = None;
    let started = Instant::now();
    let mut hidden = a.hidden;
    // when a page on the site (not its /transport hand-over) last finished loading
    let mut site_loaded: Option<Instant> = None;
    let mut waiting_said = false;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        let _keep = (&window, &context); // the window and the web view's profile live as long as the web view
        match event {
            Event::UserEvent(UserEvent::Loaded(url)) => {
                log.page("loaded", &url);
                if host_of(&url) == a.done_host && !path_of(&url).starts_with("/transport") {
                    site_loaded = Some(Instant::now());
                }
                // hidden, and sent off the site (to its sign-in) or to a sign-in page: show the window, to sign in
                if hidden && !url.starts_with("about:") && (host_of(&url) != a.done_host || on_signin_page(&url)) {
                    hidden = false;
                    log.page("showing the window to sign in at", &url);
                    window.set_visible(true);
                    window.set_focus();
                }
                // Landed somewhere else after signing in (Xtime: Scheduling): go on (not more than every 20 s)
                let go_on = match (&a.then, &a.then_from) {
                    (Some(then), Some(from)) if go_on_from(&url, from) => Some(then_address(then, &url)),
                    _ => None,
                };
                if let Some(next) = go_on.filter(|_| went_on.is_none_or(|t| t.elapsed() > Duration::from_secs(20))) {
                    went_on = Some(Instant::now());
                    log.page("going on to", &next);
                    let _ = webview.load_url(&next);
                }
            }
            Event::UserEvent(UserEvent::Check) => {
                let here = webview.url().unwrap_or_default();
                // still hidden past its time: give up
                if hidden && a.timeout.is_some_and(|t| started.elapsed() > t) {
                    log.page("timed out at", &here);
                    answer(&Answer {
                        ok: false,
                        cancelled: false,
                        error: Some("PTS didn't finish opening the vehicle".into()),
                        cookies: vec![],
                    });
                    *control_flow = ControlFlow::ExitWithCode(2);
                    return;
                }
                let settled = a.settle.is_none_or(|s| site_loaded.is_some_and(|t| t.elapsed() >= s));
                let has_cookie = a.wait_cookie.as_ref().is_none_or(|name| {
                    webview.cookies().is_ok_and(|list| {
                        list.iter()
                            .any(|c| c.name() == name && c.domain().is_some_and(|d| wanted(d, &a.domains)))
                    })
                });
                let ready = (settled && has_cookie && signed_in(&here, &a.done_host))
                    || (a.done_cookie
                        && !on_signin_page(&here)
                        && done_host_cookies(&webview, &a.done_host).difference(&before).next().is_some());
                // on the site, but its cookie isn't there yet: say so once (names only)
                if settled && !has_cookie && signed_in(&here, &a.done_host) && !waiting_said {
                    waiting_said = true;
                    log.line(&format!("waiting for the {} cookie", a.wait_cookie.as_deref().unwrap_or("")));
                }
                ready_checks = if ready { ready_checks + 1 } else { 0 };
                if ready_checks < 2 {
                    return;
                }
                log.page("signed in at", &here);
                match webview.cookies() {
                    Ok(list) => {
                        let cookies: Vec<SiteCookie> = list
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
                        log.line(&format!(
                            "handing over {} cookies: {}",
                            cookies.len(),
                            cookies.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(",")
                        ));
                        answer(&Answer {
                            ok: true,
                            cancelled: false,
                            error: None,
                            cookies,
                        });
                        *control_flow = ControlFlow::Exit;
                    }
                    Err(e) => {
                        log.line(&format!("couldn't read the cookies: {e}"));
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
                log.page("closed at", &webview.url().unwrap_or_default());
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
    fn goes_on_from_scheduling_to_inspect() {
        let from = "xtime.app.coxautoinc.com";
        let sched = "https://x1.xtime.app.coxautoinc.com/panama/scheduling?a=b";
        assert!(go_on_from(sched, from));
        assert!(!go_on_from(
            "https://login.xtime.app.coxautoinc.com/?pt=svp&view=svp&StoreID=1",
            from
        ));
        assert!(!go_on_from("https://xtime.signin.coxautoinc.com/?x", from));
        assert!(!go_on_from("https://www.inspect.app.coxautoinc.com/", from));
        assert_eq!(
            then_address("https://login.{DOMAIN}/?pt=svp&view=svp&StoreID=3590", sched),
            "https://login.xtime.app.coxautoinc.com/?pt=svp&view=svp&StoreID=3590"
        );
    }

    #[test]
    fn knows_sign_in_pages() {
        assert!(on_signin_page("https://xtime.signin.coxautoinc.com/?return=x"));
        assert!(on_signin_page("about:blank"));
        assert!(on_signin_page(""));
        assert!(on_signin_page("https://www.inspect.app.coxautoinc.com/Account/Login?r=1"));
        assert!(!on_signin_page("https://login.xtime.app.coxautoinc.com/?pt=svp&view=svp&StoreID=1"));
        assert!(!on_signin_page("https://x1.xtime.app.coxautoinc.com/panama/"));
    }

    #[test]
    fn inspects_own_cookies_only() {
        let host = "www.inspect.app.coxautoinc.com";
        assert!(cookie_of("www.inspect.app.coxautoinc.com", host));
        assert!(cookie_of(".inspect.app.coxautoinc.com", host));
        assert!(!cookie_of(".coxautoinc.com", host));
        assert!(!cookie_of(".app.coxautoinc.com", host));
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
