//! Reading Xtime's JSON loosely, as the JavaScript version does: field names in any case, values as JavaScript would
//! turn them into text.

use regex::Regex;
use serde_json::{Map, Value};

/// A field by name, in any case ("RONumber", "roNumber"…), like the JS `get(obj, name)`
pub fn get<'a>(v: &'a Value, name: &str) -> Option<&'a Value> {
    v.as_object()?.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v)
}

/// JavaScript's `String(x)` for what Xtime sends; missing → ""
pub fn text(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(other) => other.to_string(),
    }
}

/// Like JS `String(x)` where `null` and `undefined` count too (used to compare IDs: a missing ID matches nothing real)
pub fn js_string(v: Option<&Value>) -> String {
    match v {
        None => "undefined".into(),
        Some(Value::Null) => "null".into(),
        other => text(other),
    }
}

/// true, 1, "true", "y", "yes", "1"
pub fn flag(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() == Some(1.0),
        Some(Value::String(s)) => matches!(s.to_ascii_lowercase().as_str(), "true" | "y" | "yes" | "1"),
        _ => false,
    }
}

/// The first field whose name matches one of the patterns (tried in order) and holds a plain value (not empty, not an
/// object or list), like the dashboard's `pick(obj, ...patterns)`
pub fn pick(obj: Option<&Value>, patterns: &[&Regex]) -> Option<Value> {
    let map = obj?.as_object()?;
    for re in patterns {
        let hit = map
            .iter()
            .find(|(k, v)| re.is_match(k) && !v.is_null() && v.as_str() != Some("") && !v.is_object() && !v.is_array());
        if let Some((_, v)) = hit {
            return Some(v.clone());
        }
    }
    None
}

/// The first field whose name matches and holds an object (e.g. "Customer", "ServiceAdvisor")
pub fn sub<'a>(obj: &'a Value, re: &Regex) -> Option<&'a Value> {
    obj.as_object()?
        .iter()
        .find(|(k, v)| re.is_match(k) && v.is_object())
        .map(|(_, v)| v)
}

/// "000456" → "456": JavaScript's `.trim().replace(/^0+(?=\d)/, '')` (zeros go while a digit follows them)
pub fn no_leading_zeros(s: &str) -> String {
    let t = s.trim();
    let zeros = t.len() - t.trim_start_matches('0').len();
    let rest = &t[zeros..];
    if zeros == 0 {
        t.to_string()
    } else if rest.starts_with(|c: char| c.is_ascii_digit()) {
        rest.to_string()
    } else {
        t[zeros - 1..].to_string() // "000" → "0", "00A" → "0A"
    }
}

/// Field names (no values) of an object, nested objects as `Name{a,b}`: sent to the log to map Xtime's fields
pub fn field_names(obj: &Map<String, Value>) -> Vec<String> {
    obj.iter()
        .map(|(k, v)| match v.as_object() {
            Some(m) => format!("{k}{{{}}}", m.keys().cloned().collect::<Vec<_>>().join(",")),
            None => k.clone(),
        })
        .collect()
}

/// A pattern compiled once, where it's written: `rx!("(?i)^promise")`
macro_rules! rx {
    ($s:literal) => {{
        static R: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new($s).expect("pattern"));
        &*R
    }};
}
pub(crate) use rx;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_fields_loosely() {
        let v = json!({ "roNumber": 820367, "Active": "yes", "Tag": null });
        assert_eq!(text(get(&v, "RONumber")), "820367");
        assert!(flag(get(&v, "active")));
        assert_eq!(js_string(get(&v, "Tag")), "null");
        assert_eq!(js_string(get(&v, "Nope")), "undefined");
    }

    #[test]
    fn strips_leading_zeros_like_js() {
        assert_eq!(no_leading_zeros("000456"), "456");
        assert_eq!(no_leading_zeros("0"), "0");
        assert_eq!(no_leading_zeros("000"), "0");
        assert_eq!(no_leading_zeros(" 12 "), "12");
        assert_eq!(no_leading_zeros("0A1"), "0A1");
        assert_eq!(no_leading_zeros("00A"), "0A");
    }

    #[test]
    fn picks_the_first_plain_match_in_order() {
        let v = json!({ "PromiseTime": "", "PromisedDate": "2026-11-14", "Promise": { "x": 1 } });
        assert_eq!(pick(Some(&v), &[rx!("(?i)promise")]), Some(json!("2026-11-14")));
    }
}