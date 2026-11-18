//! The pages on Windows: WebView2 web views (wry), children of the app's window, kept on the window's own thread
//! (a web view can only be used there: they're kept in that thread's own list, never handed around).

use std::cell::RefCell;
use std::collections::HashMap;

use iced::Rectangle;
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::raw_window_handle::HasWindowHandle;
use wry::{PageLoadEvent, WebContext, WebView, WebViewBuilder};

use crate::Page;
use crate::logins;

thread_local! {
    /// The pages open, by the app's name for them
    static PAGES: RefCell<HashMap<String, WebView>> = RefCell::new(HashMap::new());
    /// Their profile (lives as long as they do)
    static CONTEXT: RefCell<Option<WebContext>> = const { RefCell::new(None) };
    /// What's been typed on a sign-in page in them (saved once the sign-in works)
    static TYPED: RefCell<logins::Typed> = RefCell::new(logins::Typed::default());
}

fn rect(r: Rectangle) -> wry::Rect {
    wry::Rect {
        position: LogicalPosition::new(f64::from(r.x), f64::from(r.y)).into(),
        size: LogicalSize::new(f64::from(r.width.max(1.0)), f64::from(r.height.max(1.0))).into(),
    }
}

pub fn show(parent: &impl HasWindowHandle, page: &Page, at: Rectangle) -> Result<(), String> {
    PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        for (id, v) in pages.iter() {
            if *id != page.id {
                let _ = v.set_visible(false);
            }
        }
        if let Some(v) = pages.get(&page.id) {
            v.set_bounds(rect(at)).map_err(|e| e.to_string())?;
            return v.set_visible(true).map_err(|e| e.to_string());
        }
        let view = CONTEXT.with(|c| {
            let mut c = c.borrow_mut();
            let context = c.get_or_insert_with(|| WebContext::new(Some(page.data_dir.clone())));
            let (load_id, fields_id) = (page.id.clone(), page.id.clone());
            WebViewBuilder::with_web_context(context)
                .with_bounds(rect(at))
                .with_initialization_script(logins::CAPTURE)
                .with_ipc_handler(move |req| fields(&fields_id, req.body()))
                .with_on_page_load_handler(move |event, url| {
                    if let PageLoadEvent::Finished = event {
                        loaded(&load_id, &url);
                    }
                })
                .build_as_child(parent)
                .map_err(|e| format!("Couldn't open the page here ({e})"))
        })?;
        // signed in before it opens: the app's sign-in cookies
        set_cookies(&view, &page.cookies);
        view.load_url(&page.url).map_err(|e| e.to_string())?;
        pages.insert(page.id.clone(), view);
        Ok(())
    })
}

pub fn hide_all() {
    PAGES.with(|pages| {
        if let Ok(pages) = pages.try_borrow() {
            for v in pages.values() {
                let _ = v.set_visible(false);
            }
        }
    });
}

pub fn close(ids: &[String]) {
    PAGES.with(|pages| {
        if let Ok(mut pages) = pages.try_borrow_mut() {
            for id in ids {
                // (dropped: its web view closes)
                pages.remove(id);
            }
        }
    });
}

pub fn reload(id: &str) {
    PAGES.with(|pages| {
        if let Ok(pages) = pages.try_borrow() {
            if let Some(v) = pages.get(id) {
                let _ = v.evaluate_script("location.reload()");
            }
        }
    });
}

fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    host.rsplit('@')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// A page's sign-in fields, as it sent them (the site: where the page really is)
fn fields(id: &str, body: &str) {
    #[derive(serde::Deserialize)]
    struct Sent {
        fields: logins::Login,
    }
    let Ok(sent) = serde_json::from_str::<Sent>(body) else { return };
    let here = PAGES.with(|p| p.try_borrow().ok().and_then(|p| p.get(id).and_then(|v| v.url().ok())));
    let Some(here) = here.map(|u| host_of(&u)) else { return };
    TYPED.with(|t| {
        if let Ok(mut t) = t.try_borrow_mut() {
            t.fields(&here, &sent.fields.user, &sent.fields.pass);
        }
    });
}

/// A page loaded: off the sign-in page a password was typed on (it worked), the login's saved; a sign-in page with a
/// saved login, filled in
fn loaded(id: &str, url: &str) {
    let host = host_of(url);
    TYPED.with(|t| {
        if let Ok(mut t) = t.try_borrow_mut() {
            if t.typed_on().is_some_and(|h| h != host) {
                if let Some((h, login)) = t.take() {
                    logins::save(&h, &login);
                }
            }
        }
    });
    if let Some(saved) = logins::get(&host) {
        let script = logins::fill_script(&saved);
        PAGES.with(|p| {
            if let Ok(p) = p.try_borrow() {
                if let Some(v) = p.get(id) {
                    let _ = v.evaluate_script(&script);
                }
            }
        });
    }
}

/// Hands the web view the app's sign-in cookies (WebView2's cookie manager: wry has no way to yet)
fn set_cookies(view: &WebView, cookies: &[workbench_net::SiteCookie]) {
    use ::windows::Win32::Foundation::BOOL;
    use ::windows::core::{HSTRING, Interface};
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_2;
    use wry::WebViewExtWindows;
    if cookies.is_empty() {
        return;
    }
    // SAFETY: COM calls on the window's own thread, on a web view that's open
    unsafe {
        let Ok(core) = view.controller().CoreWebView2() else { return };
        let Ok(core) = core.cast::<ICoreWebView2_2>() else { return };
        let Ok(manager) = core.CookieManager() else { return };
        for c in cookies {
            let path = if c.path.is_empty() { "/" } else { c.path.as_str() };
            let Ok(cookie) = manager.CreateCookie(
                &HSTRING::from(c.name.as_str()),
                &HSTRING::from(c.value.as_str()),
                &HSTRING::from(c.domain.as_str()),
                &HSTRING::from(path),
            ) else {
                continue;
            };
            let _ = cookie.SetIsSecure(BOOL::from(c.secure));
            let _ = cookie.SetIsHttpOnly(BOOL::from(c.http_only));
            if let Some(t) = c.expires {
                let _ = cookie.SetExpires(t as f64);
            }
            let _ = manager.AddOrUpdateCookie(&cookie);
        }
    }
}
