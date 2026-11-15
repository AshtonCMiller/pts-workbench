//! The app's settings, saved as `settings.json` in the app's own folder:
//! Windows `%APPDATA%\PTS Workbench`, Linux `$XDG_CONFIG_HOME/pts-workbench` (or `~/.config/pts-workbench`).
//! The store ID and dealer code are asked for on first setup; neither is ever in the code.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Your employee number (Xtime finds your technician ID from it)
    pub employee_number: String,
    /// Your Xtime store ID
    pub store_id: String,
    /// The dealership's P&A code (Ford)
    pub dealer_code: String,
    /// Your Xtime technician ID: looked up from the employee number once Xtime is connected, or typed in
    pub tech_id: String,
    /// Your name, sent with feedback (later also from Xtime)
    pub name: String,
    pub theme: ThemeChoice,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    /// Follows Windows' own light or dark setting (and changes with it)
    #[default]
    Auto,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::Auto, ThemeChoice::Light, ThemeChoice::Dark];

    /// Light or Dark: Auto is whichever the system uses now
    pub fn resolve(self, system_dark: bool) -> ThemeChoice {
        match self {
            ThemeChoice::Auto if system_dark => ThemeChoice::Dark,
            ThemeChoice::Auto => ThemeChoice::Light,
            other => other,
        }
    }
}

impl std::fmt::Display for ThemeChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ThemeChoice::Auto => "Auto",
            ThemeChoice::Light => "Light",
            ThemeChoice::Dark => "Dark",
        })
    }
}

impl Settings {
    /// What first setup asks for before the app opens
    pub fn needs_setup(&self) -> bool {
        self.store_id.trim().is_empty() || self.dealer_code.trim().is_empty()
    }

    /// Settings as saved last time; defaults when there are none (or they can't be read)
    pub fn load() -> Self {
        std::fs::read_to_string(file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Saves them (whole or not at all)
    pub fn save(&self) -> std::io::Result<()> {
        let f = file();
        if let Some(dir) = f.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = f.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, f)
    }

    /// Tidied before saving: no stray spaces; the dealer code in capitals
    pub fn cleaned(mut self) -> Self {
        for s in [&mut self.employee_number, &mut self.store_id, &mut self.tech_id, &mut self.name] {
            *s = s.trim().to_string();
        }
        self.dealer_code = self.dealer_code.trim().to_uppercase();
        self
    }
}

/// The app's own folder (shared with the launcher, which keeps downloaded versions there)
pub fn app_dir() -> PathBuf {
    launcher::app_dir()
}

fn file() -> PathBuf {
    app_dir().join("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_needs_store_and_dealer_code() {
        let mut s = Settings::default();
        assert!(s.needs_setup());
        s.store_id = "1234".into();
        s.dealer_code = " f1234 ".into();
        assert!(!s.needs_setup());
        assert_eq!(s.cleaned().dealer_code, "F1234");
    }

    #[test]
    fn old_files_still_load() {
        // a field added later just takes its default
        let s: Settings = serde_json::from_str(r#"{ "store_id": "1" }"#).unwrap();
        assert_eq!(s.store_id, "1");
        assert_eq!(s.theme, ThemeChoice::Auto);
        // a saved choice stays
        let s: Settings = serde_json::from_str(r#"{ "theme": "Light" }"#).unwrap();
        assert_eq!(s.theme, ThemeChoice::Light);
    }

    #[test]
    fn auto_follows_the_system() {
        assert_eq!(ThemeChoice::Auto.resolve(true), ThemeChoice::Dark);
        assert_eq!(ThemeChoice::Auto.resolve(false), ThemeChoice::Light);
        assert_eq!(ThemeChoice::Light.resolve(true), ThemeChoice::Light);
        assert_eq!(ThemeChoice::Dark.resolve(false), ThemeChoice::Dark);
    }
}
