//! The rates the screens show over one trip or many (US-80): always a total
//! over a total, so figures added up over several trips give their overall
//! rate and never an average of averages. Pure, so they are unit-tested on
//! the host (ADR-0012).

/// The average speed in km/h of `metres` covered in `secs` of moving; `None`
/// without moving time, where there is no speed to speak of.
pub fn average_kmh(metres: f64, secs: f64) -> Option<f64> {
    (secs > 0.0).then(|| metres / secs * 3.6)
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
}
