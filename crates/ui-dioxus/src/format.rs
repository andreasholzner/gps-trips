//! Pure display formatting for the screens. Kept free of any Dioxus or
//! browser types so it runs under a plain `cargo test` on the host
//! (ADR-0012: pure logic lives in plain modules).

use time::{format_description::well_known::Rfc3339, Date, OffsetDateTime, UtcOffset};
use trip_archive_types::{KomootPrivacy, TripKind};

/// Metres as kilometres, the unit the list shows.
pub fn km(metres: f64) -> String {
    format!("{:.2} km", metres / 1000.0)
}

/// Metres of ascent/descent — whole metres, no decimals.
pub fn metres(value: Option<f64>) -> String {
    value.map_or_else(dash, |m| format!("{m:.0} m"))
}

/// A speed in km/h, to a tenth (US-79).
pub fn speed(kmh: Option<f64>) -> String {
    kmh.map_or_else(dash, |kmh| format!("{kmh:.1} km/h"))
}

/// A climbing rate in whole metres an hour (US-81).
pub fn climbing_rate(metres_per_hour: Option<f64>) -> String {
    metres_per_hour.map_or_else(dash, |rate| format!("{rate:.0} m/h"))
}

/// An incline as a whole percentage, signed so uphill and downhill read
/// apart at a glance (US-79): `+7 %`, `−4 %`, and level ground as `0 %`.
pub fn incline(percent: Option<f64>) -> String {
    percent.map_or_else(dash, |percent| match percent.round() {
        p if p > 0.0 => format!("+{p:.0} %"),
        p if p < 0.0 => format!("−{:.0} %", -p),
        _ => "0 %".to_string(),
    })
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

/// A string shown as it is, or a dash when there is none — the readout's
/// already-rendered time of a point the GPX gave none (US-62).
pub fn or_dash(value: Option<&str>) -> String {
    value.map_or_else(dash, str::to_string)
}

/// A user agent as the owner reads it (US-70): the device, then the
/// browser — "iPhone · Safari". Each is looked for in the order that tells
/// look-alikes apart: an iPhone also claims to be "like Mac OS X", Android
/// is a Linux, Edge and Chrome both say "Safari". One that names neither is
/// shown as it came.
pub fn device(user_agent: &str) -> String {
    let has = |needle: &str| user_agent.contains(needle);
    let system = [
        ("iPhone", "iPhone"),
        ("iPad", "iPad"),
        ("Android", "Android"),
        ("CrOS", "ChromeOS"),
        ("Windows", "Windows"),
        ("Macintosh", "Mac"),
        ("Linux", "Linux"),
    ]
    .into_iter()
    .find(|(needle, _)| has(needle))
    .map(|(_, name)| name);
    let browser = [
        ("Edg", "Edge"),
        ("OPR/", "Opera"),
        ("Firefox/", "Firefox"),
        ("FxiOS", "Firefox"),
        ("CriOS", "Chrome"),
        ("Chrome/", "Chrome"),
        ("Safari/", "Safari"),
    ]
    .into_iter()
    .find(|(needle, _)| has(needle))
    .map(|(_, name)| name);
    match (system, browser) {
        (Some(system), Some(browser)) => format!("{system} · {browser}"),
        (Some(one), None) | (None, Some(one)) => one.to_string(),
        (None, None) => user_agent.to_string(),
    }
}

/// A linked Komoot tour's privacy (US-35), or a dash for a trip that never
/// came from Komoot — and for a linked one whose privacy no sync has read
/// yet. A privacy Komoot reported that the archive couldn't map shows as
/// "Unknown": displayed, never pushed back (ADR-0021).
pub fn privacy(privacy: Option<KomootPrivacy>) -> String {
    privacy.map_or_else(dash, |p| p.label().to_string())
}

/// How many trips the list shows, and how many the tab holds in all (US-61)
/// — "247 of 312 recorded trips", so a narrowed list says what it narrowed
/// *from*. An unnarrowed list drops the "of": there is nothing to compare it
/// against, and "312 of 312" only reads as noise.
///
/// The noun agrees with the number it follows, which is the total whenever
/// both are shown.
pub fn trip_counts(shown: usize, total: usize, kind: TripKind) -> String {
    let noun = if total == 1 { "trip" } else { "trips" };
    let kind = kind.as_str();
    if shown == total {
        format!("{total} {kind} {noun}")
    } else {
        format!("{shown} of {total} {kind} {noun}")
    }
}

/// Which rows of the list a page shows (US-63), counted from one the way
/// the owner counts them.
pub fn page_place(rows: std::ops::Range<usize>) -> String {
    format!("showing {}–{}", rows.start + 1, rows.end)
}

/// A stored RFC-3339 instant (ADR-0009), or `None` for anything else — an
/// empty string is how the track says a point has no time.
pub fn instant(timestamp: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(timestamp, &Rfc3339).ok()
}

/// An instant as the clock read where it happened (US-62): `14:32 (+02:00)`,
/// or with its date, `11 Jul 14:32 (+02:00)`. Never a bare time — one with no
/// zone on it is worse than one in the wrong zone — so an instant whose
/// offset is not known is shown as what it is, `14:32 UTC`.
pub fn clock(instant: OffsetDateTime, offset: Option<UtcOffset>, with_date: bool) -> String {
    let local = instant.to_offset(offset.unwrap_or(UtcOffset::UTC));
    let time = format!("{:02}:{:02}", local.hour(), local.minute());
    let time = match offset {
        Some(offset) => format!("{time} ({})", utc_offset(offset)),
        None => format!("{time} UTC"),
    };
    if with_date {
        format!("{} {} {time}", local.day(), short_month(local.date()))
    } else {
        time
    }
}

/// A stored `YYYY-MM-DD` date as the owner reads one, `11. Jul. 2026` (US-62).
/// Anything else is shown as it is rather than hidden.
pub fn day(date: &str) -> String {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    match Date::parse(date, &format) {
        Ok(d) => format!("{}. {}. {}", d.day(), short_month(d), d.year()),
        Err(_) => date.to_string(),
    }
}

fn short_month(date: Date) -> String {
    date.month().to_string().chars().take(3).collect()
}

/// `+02:00`, `-04:00`, `+05:30` — the sign always shown.
fn utc_offset(offset: UtcOffset) -> String {
    let (h, m, _) = offset.as_hms();
    let sign = if offset.is_negative() { '-' } else { '+' };
    format!("{sign}{:02}:{:02}", h.unsigned_abs(), m.unsigned_abs())
}

fn dash() -> String {
    "—".to_string()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us70_a_user_agent_is_named_by_its_device_and_browser() {
        for (agent, device) in [
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 \
                 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
                "iPhone · Safari",
            ),
            (
                "Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/140.0.0.0 Mobile Safari/537.36",
                "Android · Chrome",
            ),
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0",
                "Windows · Edge",
            ),
            (
                "Mozilla/5.0 (X11; Linux x86_64; rv:143.0) Gecko/20100101 Firefox/143.0",
                "Linux · Firefox",
            ),
            (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
                 (KHTML, like Gecko) Version/18.0 Safari/605.1.15",
                "Mac · Safari",
            ),
            ("curl/8.9.1", "curl/8.9.1"),
        ] {
            assert_eq!(super::device(agent), device, "{agent}");
        }
    }

    #[test]
    fn us79_a_speed_is_shown_in_kilometres_an_hour() {
        assert_eq!(speed(Some(12.345)), "12.3 km/h");
        assert_eq!(speed(Some(0.0)), "0.0 km/h");
        assert_eq!(speed(None), "—");
    }

    #[test]
    fn us81_a_climbing_rate_is_shown_in_whole_metres_an_hour() {
        assert_eq!(climbing_rate(Some(612.6)), "613 m/h");
        assert_eq!(climbing_rate(None), "—");
    }

    #[test]
    fn us79_an_incline_is_a_signed_whole_percentage() {
        assert_eq!(incline(Some(7.4)), "+7 %");
        assert_eq!(incline(Some(-4.6)), "−5 %");
        // Neither sign on level ground, however it was approached.
        assert_eq!(incline(Some(0.0)), "0 %");
        assert_eq!(incline(Some(-0.3)), "0 %");
        assert_eq!(incline(None), "—");
    }

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
    fn a_privacy_is_shown_by_its_label_and_an_absent_one_as_a_dash() {
        assert_eq!(privacy(Some(KomootPrivacy::Public)), "Public");
        assert_eq!(privacy(Some(KomootPrivacy::Private)), "Private");
        // Shown, not hidden: the archive says what it couldn't map.
        assert_eq!(privacy(Some(KomootPrivacy::Unknown)), "Unknown");
        // Never came from Komoot, or no sync has read its privacy yet.
        assert_eq!(privacy(None), "—");
    }

    #[test]
    fn a_stored_value_is_shown_unchanged_and_an_absent_one_as_a_dash() {
        assert_eq!(
            or_dash(Some("2026-07-11T09:30:00Z")),
            "2026-07-11T09:30:00Z"
        );
        assert_eq!(or_dash(Some("Europe/Oslo")), "Europe/Oslo");
        assert_eq!(or_dash(None), "—");
    }

    // US-61: the line above the table. The second number appears only when
    // there is something to have narrowed from.
    #[test]
    fn an_unnarrowed_list_reports_one_number() {
        assert_eq!(
            trip_counts(312, 312, TripKind::Recorded),
            "312 recorded trips"
        );
        assert_eq!(trip_counts(7, 7, TripKind::Planned), "7 planned trips");
    }

    #[test]
    fn a_narrowed_list_says_what_it_narrowed_from() {
        assert_eq!(
            trip_counts(247, 312, TripKind::Recorded),
            "247 of 312 recorded trips"
        );
        // Nothing matched, but the tab is not empty — which is exactly the
        // case this line exists for.
        assert_eq!(
            trip_counts(0, 312, TripKind::Recorded),
            "0 of 312 recorded trips"
        );
    }

    #[test]
    fn the_owners_place_is_the_rows_shown_counted_from_one() {
        // US-63: beside US-61's counts, where the page is in the list.
        assert_eq!(page_place(50..100), "showing 51–100");
        assert_eq!(page_place(200..247), "showing 201–247");
    }

    #[test]
    fn the_noun_agrees_with_the_number_beside_it() {
        assert_eq!(trip_counts(1, 1, TripKind::Recorded), "1 recorded trip");
        // "1 of 312" — the noun follows the total, not the shown count.
        assert_eq!(
            trip_counts(1, 312, TripKind::Recorded),
            "1 of 312 recorded trips"
        );
    }

    // ── US-62: times in the zone they were read in ───────────────────────

    use time::macros::{datetime, offset};

    #[test]
    fn a_time_is_read_in_its_own_offset_and_says_which() {
        assert_eq!(
            clock(datetime!(2026-07-11 12:32 UTC), Some(offset!(+2)), false),
            "14:32 (+02:00)"
        );
        assert_eq!(
            clock(datetime!(2026-07-11 12:32 UTC), Some(offset!(-4)), false),
            "08:32 (-04:00)"
        );
        assert_eq!(
            clock(datetime!(2026-07-11 12:02 UTC), Some(offset!(+5:30)), false),
            "17:32 (+05:30)"
        );
    }

    #[test]
    fn a_time_with_its_date_is_dated_where_it_was_read() {
        // 23:30 UTC on the 10th is already the 11th at +02:00.
        assert_eq!(
            clock(datetime!(2026-07-10 23:30 UTC), Some(offset!(+2)), true),
            "11 Jul 01:30 (+02:00)"
        );
    }

    #[test]
    fn a_time_whose_offset_is_unknown_says_it_is_utc() {
        assert_eq!(
            clock(datetime!(2026-07-11 12:32 UTC), None, false),
            "12:32 UTC"
        );
        assert_eq!(
            clock(datetime!(2026-07-11 12:32 UTC), None, true),
            "11 Jul 12:32 UTC"
        );
    }

    #[test]
    fn a_stored_instant_is_read_and_an_empty_one_is_none() {
        assert_eq!(
            instant("2026-07-11T12:32:00Z"),
            Some(datetime!(2026-07-11 12:32 UTC))
        );
        assert_eq!(instant(""), None);
    }

    #[test]
    fn a_date_is_shown_as_the_owner_reads_one() {
        assert_eq!(day("2026-07-11"), "11. Jul. 2026");
        assert_eq!(day("sometime"), "sometime");
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
