//! The rates the screens show over one trip or many — the average speed
//! (US-80) and the climbing rate (US-81): always a total
//! over a total, so figures added up over several trips give their overall
//! rate and never an average of averages. Pure, so they are unit-tested on
//! the host (ADR-0012).

/// The average speed in km/h of `metres` covered in `secs` of moving; `None`
/// without moving time, where there is no speed to speak of.
pub fn average_kmh(metres: f64, secs: f64) -> Option<f64> {
    (secs > 0.0).then(|| metres / secs * 3.6)
}

/// The climbing rate in m/h of `gain_m` climbed in `secs` of moving on the
/// climbs (US-81); `None` without moving time — a trip with no climb.
pub fn climbing_rate(gain_m: f64, secs: f64) -> Option<f64> {
    (secs > 0.0).then(|| gain_m / secs * 3600.0)
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us80_the_average_speed_is_distance_over_time_in_kmh() {
        assert_eq!(average_kmh(12_000.0, 3600.0), Some(12.0));
        assert_eq!(average_kmh(0.0, 600.0), Some(0.0));
    }

    #[test]
    fn us80_no_moving_time_gives_no_speed() {
        assert_eq!(average_kmh(100.0, 0.0), None);
    }

    #[test]
    fn us81_the_climbing_rate_is_height_over_time_in_metres_per_hour() {
        assert_eq!(climbing_rate(300.0, 1800.0), Some(600.0));
        assert_eq!(climbing_rate(0.0, 0.0), None);
    }
}
