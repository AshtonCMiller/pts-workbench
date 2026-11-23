//! Predictive Pay in the app (the math: crates/hours `pay`; the look: crates/hours-ui): its setup is `pay.json` in
//! the profile's folder, and syncs to your other computers and your phone, encrypted, in a document of its own
//! (crates/sync `Keys::pay`), written whole as `{ format, pay }`. Whichever changed it last wins (`at`).

use std::path::PathBuf;

use hours::pay::PaySetup;
use serde_json::{Value, json};

/// How often your other computers' and phone's changes are looked for (seconds; and whenever Hours opens)
pub const EVERY: i64 = 5 * 60;

fn file() -> PathBuf {
    crate::settings::app_dir().join("pay.json")
}

/// As it was saved here (none: not set up)
pub fn load() -> Option<PaySetup> {
    std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok())
}

pub fn save(p: &PaySetup) -> std::io::Result<()> {
    let f = file();
    if let Some(dir) = f.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = f.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(p).map_err(std::io::Error::other)?)?;
    std::fs::rename(tmp, f)
}

/// The synced document
pub fn doc(p: &PaySetup) -> Value {
    json!({ "format": 1, "pay": p })
}

/// The setup in a synced document, if it's newer than this computer's
pub fn newer(doc: &Value, here: Option<&PaySetup>) -> Option<PaySetup> {
    let got: PaySetup = serde_json::from_value(doc.get("pay")?.clone()).ok()?;
    (got.at > here.map_or(0, |h| h.at)).then_some(got)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_wins() {
        let mut a = PaySetup {
            at: 10,
            retirement: 5.0,
            ..PaySetup::default()
        };
        let d = doc(&a);
        assert_eq!(newer(&d, None).map(|p| p.retirement), Some(5.0));
        assert!(newer(&d, Some(&a)).is_none());
        a.at = 20;
        assert!(newer(&d, Some(&a)).is_none());
        assert!(newer(&json!({ "format": 1 }), None).is_none());
    }
}
