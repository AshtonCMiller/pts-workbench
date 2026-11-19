//! "What's new" (the Electron app's, the same way): `whats-new.md` (beside this crate's Cargo.toml) lists the changes
//! in plain words, newest first, and is built into the app. Shown as a card:
//!   • once, after an update installed: the items this computer hasn't been shown yet
//!   • when an update is downloaded and waiting for a restart: what it brings (its release carries its notes)
//!   • any time, from Settings
//! Which items were shown is kept in `whats-new.json` in the app's folder: the Electron app's file and its way of
//! naming items, so nothing it showed is shown again.
//!
//! The file: "## heading" starts an update (a version or a date), "- item" lines under it (an item carries on to the
//! next line when that line is indented; **bold** and `code` work), and a "# Bug Fixes" / "# Improvements" line (or
//! "###") within an update starts a category for the items after it. A "#" line before the first "##" is the file's
//! own title; `<!-- … -->` is left out.

use std::collections::HashSet;
use std::path::PathBuf;

use sha1::{Digest, Sha1};

/// This version's notes (and the ones before it)
pub const NOTES: &str = include_str!("../whats-new.md");

/// One update's changes
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub head: String,
    /// (category, text): the category is "" when there's none
    pub items: Vec<(String, String)>,
}

/// What the card shows: why ("installed", or a version waiting for a restart), and the changes
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub ready: Option<String>,
    /// Shown from Settings: everything recent, seen or not
    pub all: bool,
    pub groups: Vec<Group>,
}

/// Reads the notes (as the Electron app's `parseWhatsNew`)
pub fn parse(md: &str) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    let mut cat = String::new();
    let mut in_item = false;
    let mut in_comment = false;
    for raw in md.lines() {
        let line = raw.trim_end();
        if in_comment {
            if line.contains("-->") {
                in_comment = false;
            }
            continue;
        }
        if line.trim_start().starts_with("<!--") {
            in_item = false;
            if !line.contains("-->") {
                in_comment = true;
            }
            continue;
        }
        let hashes = line.chars().take_while(|c| *c == '#').count();
        let after = &line[hashes..];
        let heading = hashes > 0 && after.starts_with(char::is_whitespace) && !after.trim().is_empty();
        if heading && hashes == 2 {
            groups.push(Group {
                head: after.trim().to_string(),
                items: Vec::new(),
            });
            cat.clear();
            in_item = false;
        } else if heading && (hashes == 1 || hashes >= 3) {
            // a category (before any "##": the file's title)
            in_item = false;
            if !groups.is_empty() {
                cat = after.trim().to_string();
            }
        } else if let Some(text) = line
            .trim_start()
            .strip_prefix(['-', '*'])
            .filter(|t| t.starts_with(char::is_whitespace) && !t.trim().is_empty())
        {
            if groups.is_empty() {
                groups.push(Group {
                    head: String::new(),
                    items: Vec::new(),
                });
            }
            let g = groups.last_mut().expect("a group");
            g.items.push((cat.clone(), text.trim().to_string()));
            in_item = true;
        } else if in_item && line.starts_with(char::is_whitespace) && !line.trim().is_empty() {
            // an item carried on to the next line
            if let Some((_, t)) = groups.last_mut().and_then(|g| g.items.last_mut()) {
                t.push(' ');
                t.push_str(line.trim());
            }
        } else if line.trim().is_empty() {
            in_item = false;
        }
    }
    for g in &mut groups {
        for (_, t) in &mut g.items {
            if t.chars().count() > 600 {
                *t = t.chars().take(600).collect();
            }
        }
    }
    groups.retain(|g| !g.items.is_empty());
    groups
}

/// An item's name in `whats-new.json` (the Electron app's: SHA-1 of its words in lowercase, 12 hex digits)
pub fn key(text: &str) -> String {
    let h = Sha1::digest(text.trim().to_lowercase().as_bytes());
    h.iter().map(|b| format!("{b:02x}")).collect::<String>()[..12].to_string()
}

fn seen_file() -> PathBuf {
    crate::settings::app_dir().join("whats-new.json")
}

/// The items shown on this computer (none when nothing was ever kept)
fn seen() -> Option<HashSet<String>> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(seen_file()).ok()?).ok()?;
    Some(
        v.get("seen")?
            .as_array()?
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
    )
}

/// Marks these items as shown on this computer
pub fn mark_seen(groups: &[Group]) {
    let file = seen_file();
    // (in the order they were kept, the newest last: the file keeps the last 2000)
    let mut list: Vec<String> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("seen")?.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|x| x.as_str().map(str::to_string))
        .collect();
    for g in groups {
        for (_, t) in &g.items {
            let k = key(t);
            if !list.contains(&k) {
                list.push(k);
            }
        }
    }
    let start = list.len().saturating_sub(2000);
    if let Some(d) = file.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(&file, serde_json::json!({ "seen": &list[start..] }).to_string());
}

/// The items not shown on this computer yet (nothing kept yet: only the newest update's, so a first install isn't
/// flooded)
pub fn unseen(groups: Vec<Group>) -> Vec<Group> {
    unseen_with(groups, seen().as_ref())
}

fn unseen_with(groups: Vec<Group>, seen: Option<&HashSet<String>>) -> Vec<Group> {
    let Some(seen) = seen else {
        return groups.into_iter().take(1).collect();
    };
    groups
        .into_iter()
        .map(|g| Group {
            items: g.items.into_iter().filter(|(_, t)| !seen.contains(&key(t))).collect(),
            ..g
        })
        .filter(|g| !g.items.is_empty())
        .collect()
}

/// After starting: what this version brings that hasn't been shown here
pub fn on_start() -> Option<Shown> {
    let groups = unseen(parse(NOTES));
    (!groups.is_empty()).then_some(Shown {
        ready: None,
        all: false,
        groups,
    })
}

/// From Settings: the last few updates, seen or not
pub fn recent() -> Shown {
    Shown {
        ready: None,
        all: true,
        groups: parse(NOTES).into_iter().take(4).collect(),
    }
}

/// A downloaded update's notes (from its release): what's new to this computer
pub fn for_update(version: &str, notes: &str) -> Option<Shown> {
    let groups = unseen(parse(notes));
    (!groups.is_empty()).then(|| Shown {
        ready: Some(version.to_string()),
        all: false,
        groups,
    })
}

/// An item's words, with its **bold** and `code` parts: (words, bold, code)
pub fn spans(text: &str) -> Vec<(String, bool, bool)> {
    let mut out: Vec<(String, bool, bool)> = Vec::new();
    let (mut bold, mut code) = (false, false);
    let mut cur = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        if !code && rest.starts_with("**") {
            if !cur.is_empty() {
                out.push((std::mem::take(&mut cur), bold, code));
            }
            bold = !bold;
            rest = &rest[2..];
        } else if rest.starts_with('`') {
            if !cur.is_empty() {
                out.push((std::mem::take(&mut cur), bold, code));
            }
            code = !code;
            rest = &rest[1..];
        } else {
            let c = rest.chars().next().expect("not empty");
            cur.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    if !cur.is_empty() {
        out.push((cur, bold, code));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MD: &str = "# What's new in PTS Workbench\n\n<!--\n  how to write it\n-->\n\n## v0.5.1\n\n# Improvements\n- **Lines:** in the sidebar,\n  with their hours.\n- Two\n\n### Bug Fixes\n* Fixed `x`\n\n## v0.5.0\n- The first\n";

    #[test]
    fn reads_the_notes() {
        let g = parse(MD);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].head, "v0.5.1");
        assert_eq!(
            g[0].items,
            vec![
                (
                    "Improvements".to_string(),
                    "**Lines:** in the sidebar, with their hours.".to_string()
                ),
                ("Improvements".to_string(), "Two".to_string()),
                ("Bug Fixes".to_string(), "Fixed `x`".to_string()),
            ]
        );
        assert_eq!(g[1].items, vec![(String::new(), "The first".to_string())]);
        // and the notes built in read
        assert!(!parse(NOTES).is_empty());
    }

    #[test]
    fn names_items_as_the_electron_app_does() {
        // crypto.createHash('sha1').update('hello').digest('hex').slice(0, 12)
        assert_eq!(key("  Hello "), "aaf4c61ddcc5");
    }

    #[test]
    fn shows_only_what_wasnt_shown() {
        let g = parse(MD);
        assert_eq!(unseen_with(g.clone(), None).len(), 1); // nothing kept: the newest only
        let seen: HashSet<String> = [key("Two"), key("The first")].into_iter().collect();
        let u = unseen_with(g, Some(&seen));
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].items.len(), 2);
    }

    #[test]
    fn bold_and_code() {
        assert_eq!(
            spans("**Lines:** see `x` now"),
            vec![
                ("Lines:".to_string(), true, false),
                (" see ".to_string(), false, false),
                ("x".to_string(), false, true),
                (" now".to_string(), false, false),
            ]
        );
    }
}
