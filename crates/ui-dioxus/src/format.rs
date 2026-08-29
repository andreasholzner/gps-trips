//! Pure display formatting for the screens. Kept free of any Dioxus or
//! browser types so it runs under a plain `cargo test` on the host
//! (ADR-0012: pure logic lives in plain modules).

/// Metres as kilometres, the unit the list shows.
pub fn km(metres: f64) -> String {
    format!("{:.2} km", metres / 1000.0)
}

/// Metres of ascent/descent — whole metres, no decimals.
pub fn metres(value: Option<f64>) -> String {
    value.map_or_else(dash, |m| format!("{m:.0} m"))
}

/// Seconds as `hh:mm:ss`.
pub fn duration(secs: Option<i64>) -> String {
    secs.map_or_else(dash, |secs| {
        let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
        format!("{h:02}:{m:02}:{s:02}")
    })
}

/// The date part of a stored RFC-3339 timestamp (ADR-0009). Anything that
/// isn't shaped like one is shown as-is rather than hidden.
pub fn date(timestamp: Option<&str>) -> String {
    match timestamp {
        None => dash(),
        Some(ts) => ts.split('T').next().unwrap_or(ts).to_string(),
    }
}

fn dash() -> String {
    "—".to_string()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_are_shown_in_kilometres() {
        assert_eq!(km(12_345.0), "12.35 km");
        assert_eq!(km(0.0), "0.00 km");
    }

    #[test]
    fn missing_values_render_as_a_dash() {
        assert_eq!(metres(None), "—");
        assert_eq!(duration(None), "—");
        assert_eq!(date(None), "—");
    }

    #[test]
    fn durations_are_zero_padded_hours_minutes_seconds() {
        assert_eq!(duration(Some(3_725)), "01:02:05");
        assert_eq!(duration(Some(0)), "00:00:00");
        // Over a day: hours keep counting up rather than wrapping.
        assert_eq!(duration(Some(90_000)), "25:00:00");
    }

    #[test]
    fn a_timestamp_is_shown_as_its_date() {
        assert_eq!(date(Some("2026-07-11T09:30:00Z")), "2026-07-11");
    }

    #[test]
    fn an_unexpected_timestamp_is_shown_unchanged() {
        assert_eq!(date(Some("sometime")), "sometime");
    }
}
