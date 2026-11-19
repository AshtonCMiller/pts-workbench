//! Profiles: people sharing a computer (the Quick Lane's, say) each with their own of everything: hours, spiffs,
//! ROs, tech number, settings, Xtime and PTS sign-ins, saved logins.
//!
//! Each profile is a folder: the first one's is the app's own folder (where everything was before profiles, so
//! nothing moves), the others' `profiles\<id>\`. [`dir`] is the one in use: everything the app keeps goes there
//! (crate::settings::app_dir). What's the computer's, not a person's (updates, the launcher), stays in the app's own
//! folder. The list is `profiles.json` there.
//!
//! A profile can have a PIN (four digits): switching to it asks for it. It's kept as a SHA-256 of the profile's ID
//! and the PIN, never the PIN itself. (It keeps coworkers out of each other's hours, not a determined snoop: the
//! files are in the same Windows account.)
//!
//! Each profile's kind ([`Kind`]): Main Shop (diagnostics and repair) or Quick Lane (quick service: no Diagnostics or
//! wiring, each vehicle opening on its maintenance specs). Asked once, when it's first used.

use std::path::PathBuf;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The first profile: the app's own folder
pub const FIRST: &str = "default";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Diagnostics and repair
    Main,
    /// Quick service and oil changes
    Quick,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Main => "Main Shop",
            Kind::Quick => "Quick Lane",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// Not asked yet: none
    #[serde(default)]
    pub kind: Option<Kind>,
    /// Its PIN's hash (none: no PIN)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
}

impl Profile {
    pub fn initials(&self) -> String {
        let s: String = self
            .name
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .take(2)
            .collect::<String>()
            .to_uppercase();
        if s.is_empty() { "?".into() } else { s }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Profiles {
    pub active: String,
    pub list: Vec<Profile>,
}

/// The folder of the profile in use (set at the start, and on switching)
static ACTIVE: RwLock<Option<PathBuf>> = RwLock::new(None);
/// Its ID (for its saved logins)
static ACTIVE_ID: RwLock<String> = RwLock::new(String::new());

fn base() -> PathBuf {
    launcher::app_dir()
}

fn file() -> PathBuf {
    base().join("profiles.json")
}

/// The folder of the profile in use
pub fn dir() -> PathBuf {
    ACTIVE.read().ok().and_then(|d| d.clone()).unwrap_or_else(base)
}

/// Whose saved logins (crates/signin/src/logins.rs): none for the first profile (its logins as they were)
pub fn scope() -> String {
    let id = ACTIVE_ID.read().map(|s| s.clone()).unwrap_or_default();
    if id == FIRST { String::new() } else { id }
}

pub fn dir_of(id: &str) -> PathBuf {
    if id == FIRST || id.is_empty() {
        base()
    } else {
        base().join("profiles").join(id)
    }
}

fn hash(id: &str, pin: &str) -> String {
    Sha256::digest(format!("{id}:{pin}").as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A PIN as it's kept: four digits (else none)
pub fn valid_pin(pin: &str) -> bool {
    pin.len() == 4 && pin.bytes().all(|b| b.is_ascii_digit())
}

impl Profiles {
    /// The list (the first time: one profile, the app's folder, named for whoever's there)
    pub fn load() -> Self {
        let mut p: Profiles = std::fs::read_to_string(file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        if p.list.is_empty() {
            // (named from the settings already here)
            let name = std::fs::read_to_string(base().join("settings.json"))
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|v| v["name"].as_str().map(str::to_string))
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| "Profile 1".into());
            p.list.push(Profile {
                id: FIRST.into(),
                name,
                kind: None,
                pin: None,
            });
        }
        if !p.list.iter().any(|x| x.id == p.active) {
            p.active = p.list[0].id.clone();
        }
        p
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(base());
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(file(), t);
        }
    }

    pub fn active(&self) -> &Profile {
        self.list.iter().find(|p| p.id == self.active).unwrap_or(&self.list[0])
    }

    pub fn active_mut(&mut self) -> &mut Profile {
        let i = self.list.iter().position(|p| p.id == self.active).unwrap_or(0);
        &mut self.list[i]
    }

    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.list.iter().find(|p| p.id == id)
    }

    /// Makes this the profile in use (everything read and kept from then on is its)
    pub fn activate(&mut self, id: &str) {
        if self.get(id).is_none() {
            return;
        }
        self.active = id.to_string();
        self.save();
        let d = dir_of(id);
        let _ = std::fs::create_dir_all(&d);
        if let Ok(mut a) = ACTIVE.write() {
            *a = Some(d);
        }
        if let Ok(mut a) = ACTIVE_ID.write() {
            *a = id.to_string();
        }
    }

    /// The one in use when the app starts
    pub fn start() -> Self {
        let mut p = Self::load();
        let id = p.active.clone();
        p.activate(&id);
        p
    }

    /// A new profile: its folder starts with the dealership's own settings (store ID, dealer code: the same for
    /// everyone here) and the person's employee number. Its ID.
    pub fn add(&mut self, name: &str, employee_number: &str, pin: Option<&str>, from: &crate::settings::Settings) -> String {
        let id = format!("p{:x}", crate::timecard::now_ms());
        let d = dir_of(&id);
        let _ = std::fs::create_dir_all(&d);
        let settings = crate::settings::Settings {
            store_id: from.store_id.clone(),
            dealer_code: from.dealer_code.clone(),
            name: name.trim().to_string(),
            employee_number: employee_number.trim().to_string(),
            theme: from.theme,
            graphics: from.graphics,
            ..Default::default()
        };
        if let Ok(t) = serde_json::to_string_pretty(&settings) {
            let _ = std::fs::write(d.join("settings.json"), t);
        }
        // (what's new already seen here: not shown again)
        let _ = std::fs::copy(base().join("whats-new.json"), d.join("whats-new.json"));
        self.list.push(Profile {
            id: id.clone(),
            name: name.trim().to_string(),
            kind: None,
            pin: pin.filter(|p| valid_pin(p)).map(|p| hash(&id, p)),
        });
        self.save();
        id
    }

    /// Takes a profile off the list (not the one in use). Its folder's kept (its hours, in case): it's only no
    /// longer offered.
    pub fn remove(&mut self, id: &str) {
        if id != self.active {
            self.list.retain(|p| p.id != id);
            self.save();
        }
    }

    /// Deletes a profile and its folder (the admin: crate::app's admin mode). Not the one in use, and not the first
    /// (its folder is the app's own).
    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        if id == self.active {
            return Err("It's the profile in use: switch to another first".into());
        }
        if id == FIRST {
            return Err("The first profile's folder is the app's own: it can't be deleted (its PIN can be reset)".into());
        }
        if self.get(id).is_none() {
            return Err("It's not on the list".into());
        }
        let d = dir_of(id);
        if d.exists() {
            std::fs::remove_dir_all(&d).map_err(|e| format!("Its folder couldn't be deleted ({e})"))?;
        }
        self.list.retain(|p| p.id != id);
        self.save();
        Ok(())
    }

    pub fn set_pin(&mut self, id: &str, pin: Option<&str>) {
        let hashed = pin.filter(|p| valid_pin(p)).map(|p| hash(id, p));
        if let Some(p) = self.list.iter_mut().find(|p| p.id == id) {
            p.pin = hashed;
        }
        self.save();
    }

    /// Whether this PIN opens it (no PIN set: anything does)
    pub fn pin_ok(&self, id: &str, pin: &str) -> bool {
        match self.get(id).and_then(|p| p.pin.as_deref()) {
            None => true,
            Some(h) => hash(id, pin) == h,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_and_initials() {
        let mut p = Profiles {
            active: FIRST.into(),
            list: vec![Profile {
                id: FIRST.into(),
                name: "Ashton Miller".into(),
                kind: Some(Kind::Main),
                pin: None,
            }],
        };
        assert_eq!(p.active().initials(), "AM");
        assert!(p.pin_ok(FIRST, ""));
        p.list[0].pin = Some(hash(FIRST, "1234"));
        assert!(p.pin_ok(FIRST, "1234"));
        assert!(!p.pin_ok(FIRST, "4321"));
        assert!(valid_pin("0042") && !valid_pin("12a4") && !valid_pin("123"));
        // the first profile is the app's own folder
        assert_eq!(dir_of(FIRST), base());
        assert_eq!(dir_of("p1"), base().join("profiles").join("p1"));
    }
}
