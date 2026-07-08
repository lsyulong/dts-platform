/// Parse a timestamp string from SQLite (TEXT column) into DateTime<Utc>.
///
/// SQLite stores timestamps as TEXT via `datetime('now')` which produces
/// the format `YYYY-MM-DD HH:MM:SS` (UTC). This function tries RFC 3339
/// first, then falls back to the SQLite format.
///
/// On parse failure, logs a warning and returns the current time as a
/// graceful degradation strategy (avoids panics in production).
pub fn parse_timestamp(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .map(|dt| dt.and_utc())
        })
        .unwrap_or_else(|_| {
            log::warn!("无法解析时间戳 '{s}'，使用当前时间作为降级值");
            chrono::Utc::now()
        })
}
