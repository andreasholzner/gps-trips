//! The climbs marked in the elevation profile (US-81): a light background
//! below the elevation line over each climb's stretch. Which samples that
//! covers is decided here, where `cargo test` reaches it; the chart only
//! fills under what it is given (ADR-0025).

use trip_archive_types::Climb;

/// The elevation at each of the chart's samples that lies on a climb, and
/// `None` everywhere else — the series the chart fills down from; `None`
/// for a trip without climbs, which then draws no such series at all.
pub fn climb_shading(
    distance_km: &[f64],
    elevation_m: &[f64],
    climbs: &[Climb],
) -> Option<Vec<Option<f64>>> {
    if climbs.is_empty() {
        return None;
    }
    let shading = distance_km
        .iter()
        .zip(elevation_m)
        .map(|(km, elevation)| {
            let metres = km * 1000.0;
            climbs
                .iter()
                .any(|climb| (climb.start_m..=climb.end_m).contains(&metres))
                .then_some(*elevation)
        })
        .collect();
    Some(shading)
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn a_climb(start_m: f64, end_m: f64) -> Climb {
        Climb {
            start_m,
            end_m,
            gain_m: 100.0,
            moving_secs: None,
        }
    }

    #[test]
    fn us81_only_the_samples_on_a_climb_are_shaded() {
        let distance_km = [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0];
        let elevation_m = [10.0, 20.0, 30.0, 40.0, 30.0, 50.0, 60.0];
        let climbs = [a_climb(500.0, 1500.0), a_climb(2500.0, 3000.0)];

        let shading = climb_shading(&distance_km, &elevation_m, &climbs).unwrap();

        assert_eq!(
            shading,
            [
                None,
                Some(20.0),
                Some(30.0),
                Some(40.0),
                None,
                Some(50.0),
                Some(60.0)
            ]
        );
    }

    #[test]
    fn us81_a_trip_without_climbs_has_no_shading_series() {
        assert_eq!(climb_shading(&[0.0, 1.0], &[10.0, 20.0], &[]), None);
    }
}
