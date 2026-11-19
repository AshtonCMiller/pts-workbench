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
    /// Whose saved logins (the app's profile)
    static SCOPE: RefCell<String> = const { RefCell::new(String::new()) };
}

fn rect(r: Rectangle) -> wry::Rect {
    wry::Rect {
        position: LogicalPosition::new(f64::from(r.x), f64::from(r.y)).into(),
        size: LogicalSize::new(f64::from(r.width.max(1.0)), f64::from(r.height.max(1.0))).into(),
    }
}

pub fn show(parent: &impl HasWindowHandle, page: &Page, at: Rectangle) -> Result<(), String> {
    SCOPE.with(|s| *s.borrow_mut() = page.scope.clone());
    PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        for (id, v) in pages.iter() {
            if *id != page.id {
                hide(v);
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

/// Hides a page, giving the keyboard back to the app first (a hidden web view keeps it otherwise: nothing could be
/// typed in the app's own fields)
fn hide(v: &WebView) {
    let _ = v.focus_parent();
    let _ = v.set_visible(false);
}

pub fn hide_all() {
    PAGES.with(|pages| {
        if let Ok(pages) = pages.try_borrow() {
            for v in pages.values() {
                hide(v);
            }
        }
    });
}

/// A picture of a page (a PNG, WebView2's CapturePreview), then the page hidden; the picture sent when it's taken
/// (none: it couldn't be)
pub fn picture_and_hide(id: &str, done: iced::futures::channel::oneshot::Sender<Option<Vec<u8>>>) {
    use ::windows::Win32::Foundation::HGLOBAL;
    use ::windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
    use ::windows::Win32::System::Com::{IStream, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET};
    use webview2_com::CapturePreviewCompletedHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG;
    use wry::WebViewExtWindows;

    let started = PAGES.with(|pages| {
        let Ok(pages) = pages.try_borrow() else { return None };
        let v = pages.get(id)?;
        // SAFETY: COM calls on the window's own thread, on a web view that's open
        unsafe {
            let core = v.controller().CoreWebView2().ok()?;
            let stream: IStream = CreateStreamOnHGlobal(HGLOBAL::default(), ::windows::Win32::Foundation::BOOL::from(true)).ok()?;
            let kept = stream.clone();
            let page = id.to_string();
            let done = std::cell::RefCell::new(Some(done));
            let handler = CapturePreviewCompletedHandler::create(Box::new(move |result| {
                // the picture read out of the stream, then the page hidden
                let png = result.ok().and_then(|_| {
                    kept.Seek(0, STREAM_SEEK_SET, None).ok()?;
                    let mut stat = STATSTG::default();
                    kept.Stat(&mut stat, STATFLAG_NONAME).ok()?;
                    let mut buf = vec![0u8; stat.cbSize as usize];
                    let mut read = 0u32;
                    kept.Read(buf.as_mut_ptr().cast(), buf.len() as u32, Some(&mut read)).ok().ok()?;
                    buf.truncate(read as usize);
                    Some(buf)
                });
                hide_one(&page);
                if let Some(d) = done.borrow_mut().take() {
                    let _ = d.send(png);
                }
                Ok(())
            }));
            core.CapturePreview(COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG, &stream, &handler)
                .ok()?;
            Some(())
        }
    });
    // (couldn't start: hidden straight away, no picture)
    if started.is_none() {
        hide_one(id);
    }
}

fn hide_one(id: &str) {
    PAGES.with(|pages| {
        if let Ok(pages) = pages.try_borrow() {
            if let Some(v) = pages.get(id) {
                hide(v);
            }
        }
    });
}

/// The keyboard back to the app (clicked outside the page showing: its fields are typed in again)
pub fn focus_app() {
    PAGES.with(|pages| {
        if let Ok(pages) = pages.try_borrow() {
            for v in pages.values() {
                let _ = v.focus_parent();
            }
        }
    });
}

pub fn close(ids: &[String]) {
    PAGES.with(|pages| {
        if let Ok(mut pages) = pages.try_borrow_mut() {
            for id in ids {
                // (dropped: its web view closes; the keyboard the app's first)
                if let Some(v) = pages.get(id) {
                    let _ = v.focus_parent();
                }
                pages.remove(id);
            }
        }
    });
}

/// Every page closed, and their profile let go of (another profile's pages open with its own)
pub fn close_all() {
    PAGES.with(|pages| {
        if let Ok(mut pages) = pages.try_borrow_mut() {
            for v in pages.values() {
                let _ = v.focus_parent();
            }
            pages.clear();
        }
    });
    CONTEXT.with(|c| {
        if let Ok(mut c) = c.try_borrow_mut() {
            *c = None;
        }
    });
    TYPED.with(|t| {
        if let Ok(mut t) = t.try_borrow_mut() {
            *t = logins::Typed::default();
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

fn scope() -> String {
    SCOPE.with(|s| s.borrow().clone())
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
                    logins::save(&scope(), &h, &login);
                }
            }
        }
    });
    if let Some(saved) = logins::get(&scope(), &host) {
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
