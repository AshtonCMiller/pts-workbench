//! Saved logins (as the Electron app's logins.js; also used by crates/webpane, the pages inside the app's window): the username and password typed on a sign-in page, kept once the
//! sign-in worked, and filled in on that site's sign-in page the next time.
//!
//! Kept by the computer's own keyring (Windows Credential Manager; the Secret Service on Linux), one entry per site
//! (by the sign-in page's host), never in a file of the app's and never written to its logs. Filled in only on a page
//! of the same host (the address the web view is really on, not what a page says it is), and never sent anywhere
//! else; "Sign in" is still yours to press.
//!
//! The web view's script ([`CAPTURE`]) tells the window what's in the sign-in fields when you press a button or
//! Enter. Sign-ins in two steps (the username, then the password on the next page) are put together.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const SERVICE: &str = "PTS Workbench sign-in";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Login {
    pub user: String,
    pub pass: String,
}

fn entry(host: &str) -> Option<keyring::Entry> {
    keyring::Entry::new(SERVICE, host).ok()
}

/// The login saved for a sign-in page's host
pub fn get(host: &str) -> Option<Login> {
    let text = entry(host)?.get_password().ok()?;
    serde_json::from_str::<Login>(&text)
        .ok()
        .filter(|l| !l.pass.is_empty() || !l.user.is_empty())
}

/// Saves it (when it's new or changed). Whether it was saved.
pub fn save(host: &str, login: &Login) -> bool {
    if host.is_empty() || login.pass.is_empty() || get(host).as_ref() == Some(login) {
        return false;
    }
    let Ok(text) = serde_json::to_string(login) else { return false };
    entry(host).is_some_and(|e| e.set_password(&text).is_ok())
}

/// What's been typed on this sign-in, so far
#[derive(Default)]
pub struct Typed {
    /// (host, username, when): the first step of a two-step sign-in
    user: Option<(String, String, Instant)>,
    /// (host, the login): a password typed
    login: Option<(String, Login)>,
}

impl Typed {
    /// What the page's fields had (`host`: where the web view is)
    pub fn fields(&mut self, host: &str, user: &str, pass: &str) {
        let user = user.trim();
        if pass.is_empty() {
            if !user.is_empty() {
                self.user = Some((host.to_string(), user.to_string(), Instant::now()));
            }
            return;
        }
        // the password page of a two-step sign-in: its username from the step before (on the same site, lately)
        let user = if user.is_empty() {
            self.user
                .as_ref()
                .filter(|(h, _, at)| same_site(h, host) && at.elapsed() < Duration::from_secs(600))
                .map(|(_, u, _)| u.clone())
                .unwrap_or_default()
        } else {
            user.to_string()
        };
        self.login = Some((
            host.to_string(),
            Login {
                user,
                pass: pass.to_string(),
            },
        ));
    }

    /// The login typed, to save once the sign-in's worked: (host, login)
    pub fn take(&mut self) -> Option<(String, Login)> {
        self.login.take()
    }

    /// The host a password was typed on (not saved yet; crates/webpane asks)
    #[allow(dead_code)]
    pub fn typed_on(&self) -> Option<&str> {
        self.login.as_ref().map(|(h, _)| h.as_str())
    }
}

/// The same site: one host's the other's, or both under the same parent ("login.cox.com", "signin.cox.com")
fn same_site(a: &str, b: &str) -> bool {
    a == b || parent(a) == parent(b)
}

fn parent(h: &str) -> Vec<&str> {
    h.split('.').rev().take(2).collect()
}

/// Run in every page: tells the window what's in its sign-in fields when a button's pressed or Enter is (names of
/// nothing else), and gives the fill-in script the fields
pub const CAPTURE: &str = r#"(() => {
  if (window.__pts) return;
  const shown = (e) => e && !e.disabled && e.type !== 'hidden' && (e.offsetWidth > 0 || e.offsetHeight > 0);
  const inputs = () => Array.from(document.querySelectorAll('input'));
  const user = () => inputs().find((i) => shown(i) && (i.type === 'email' || ((i.type === 'text' || i.type === '') &&
    /user|login|email|ident|name|account/i.test([i.name, i.id, i.autocomplete, i.placeholder].join(' ')))));
  const pass = () => inputs().find((i) => shown(i) && i.type === 'password');
  window.__pts = { user, pass };
  const tell = () => {
    const u = user(), p = pass();
    const msg = { user: u ? u.value : '', pass: p ? p.value : '' };
    if (!msg.user && !msg.pass) return;
    try { window.ipc.postMessage(JSON.stringify({ fields: msg })); } catch (_) {}
  };
  document.addEventListener('submit', tell, true);
  document.addEventListener('click', (e) => {
    if (e.target && e.target.closest && e.target.closest('button, input[type=submit], input[type=button], [role=button], a')) tell();
  }, true);
  document.addEventListener('keydown', (e) => { if (e.key === 'Enter') tell(); }, true);
})();"#;

/// Fills the sign-in fields with a saved login (each one only while it's empty), for a few seconds as the page draws
/// them (a page that shows its fields late, or one step at a time)
pub fn fill_script(l: &Login) -> String {
    let user = serde_json::to_string(&l.user).unwrap_or_else(|_| "\"\"".into());
    let pass = serde_json::to_string(&l.pass).unwrap_or_else(|_| "\"\"".into());
    format!(
        r#"(() => {{
  const u = {user}, p = {pass};
  const set = (el, v) => {{
    if (!el || !v || el.value) return;
    const s = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
    s.call(el, v);
    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
  }};
  let n = 0;
  const t = setInterval(() => {{
    if (window.__pts) {{ set(window.__pts.user(), u); set(window.__pts.pass(), p); }}
    if (++n > 40) clearInterval(t);
  }}, 250);
}})();"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_steps_put_together() {
        let mut t = Typed::default();
        t.fields("signin.cox.com", "jdoe", "");
        t.fields("signin.cox.com", "", "hunter2");
        assert_eq!(
            t.take(),
            Some((
                "signin.cox.com".into(),
                Login {
                    user: "jdoe".into(),
                    pass: "hunter2".into()
                }
            ))
        );
        assert!(t.take().is_none());
        // another site's username isn't used
        t.fields("login.other.com", "someone", "");
        t.fields("signin.cox.com", "", "pw");
        assert_eq!(t.take().unwrap().1.user, "");
    }

    #[test]
    fn fill_script_escapes() {
        let s = fill_script(&Login {
            user: "a\"b".into(),
            pass: "x'</script>".into(),
        });
        assert!(s.contains(r#"const u = "a\"b""#));
    }
}
