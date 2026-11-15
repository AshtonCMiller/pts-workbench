//! The time, for what the app shows (the shift clock itself is in your hours: crates/hours, timecard.rs).

/// Seconds since 1970
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
