//! The trip list's heat map (US-63): one translucent mark per listed trip,
//! so that where trips pile up the marks overlap and darken — a heat map by
//! superposition rather than by plugin.
//!
//! Which marks are drawn and how heavy each is gets decided here, where
//! `cargo test` reaches it; the region map's script only draws what it is
//! handed (ADR-0025).

use serde::Serialize;
use trip_archive_types::TripSummary;

/// How opaque a lone trip's mark is, and the floor no mark goes below. The
/// floor keeps every mark visible on its own — zoomed in, where marks no
/// longer overlap, each one is drawn at exactly this — and the ceiling keeps
/// a single mark from hiding the map under it.
const MAX_OPACITY: f64 = 0.6;
const MIN_OPACITY: f64 = 0.25;

/// What the region map draws: every mark at the same opacity.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HeatMarks {
    /// `[lat, lon]`, the order Leaflet takes.
    pub points: Vec<[f64; 2]>,
    pub opacity: f64,
}

/// One mark per trip, at the centre of its stored bounding box. The centre
/// rather than the box: a long tour's box spans hundreds of kilometres and
/// says little about where the tour was, while its centre answers "roughly
/// where". A trip with no box — a track without points — gets no mark.
///
/// The opacity falls as `1/√n` with the number of marks, so a few trips
/// stay clearly visible while hundreds do not merge into one solid blob.
pub fn marks(trips: &[TripSummary]) -> HeatMarks {
    let points: Vec<[f64; 2]> = trips
        .iter()
        .filter_map(
            |trip| match (trip.min_lat, trip.min_lon, trip.max_lat, trip.max_lon) {
                (Some(min_lat), Some(min_lon), Some(max_lat), Some(max_lon)) => {
                    Some([(min_lat + max_lat) / 2.0, (min_lon + max_lon) / 2.0])
                }
                _ => None,
            },
        )
        .collect();
    let opacity = opacity_for(points.len());
    HeatMarks { points, opacity }
}

fn opacity_for(count: usize) -> f64 {
    let count = count.max(1) as f64;
    (MAX_OPACITY / count.sqrt()).clamp(MIN_OPACITY, MAX_OPACITY)
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use trip_archive_types::{ActivityType, TripKind};

    fn a_trip(bbox: Option<[f64; 4]>) -> TripSummary {
        let [min_lat, min_lon, max_lat, max_lon] = match bbox {
            Some(corners) => corners.map(Some),
            None => [None; 4],
        };
        TripSummary {
            id: 1,
            name: "Trip".to_string(),
            activity_type: ActivityType::Hiking,
            start_time: None,
            distance_m: 1_000.0,
            ascent_m: None,
            duration_secs: None,
            trip_kind: TripKind::Recorded,
            privacy_status: None,
            min_lat,
            min_lon,
            max_lat,
            max_lon,
        }
    }

    #[test]
    fn a_trip_is_marked_at_the_centre_of_its_bounding_box() {
        let marks = marks(&[a_trip(Some([59.0, 10.0, 61.0, 12.0]))]);

        assert_eq!(marks.points, vec![[60.0, 11.0]]);
    }

    #[test]
    fn a_trip_without_a_bounding_box_gets_no_mark() {
        let marks = marks(&[a_trip(None), a_trip(Some([47.0, 11.0, 47.5, 11.5]))]);

        assert_eq!(marks.points, vec![[47.25, 11.25]]);
    }

    #[test]
    fn an_empty_list_draws_nothing() {
        assert!(marks(&[]).points.is_empty());
    }

    #[test]
    fn a_lone_trip_is_drawn_at_full_weight() {
        let marks = marks(&[a_trip(Some([59.0, 10.0, 61.0, 12.0]))]);

        assert_eq!(marks.opacity, MAX_OPACITY);
    }

    #[test]
    fn marks_lighten_as_more_trips_match() {
        // `1/√n`: four times the trips, half the weight each.
        assert_eq!(opacity_for(4), MAX_OPACITY / 2.0);
        assert!(opacity_for(40) < opacity_for(4));
    }

    #[test]
    fn a_huge_list_stays_visible() {
        // A lone mark in a big archive must still read as a mark once zoomed
        // in, where nothing overlaps it.
        assert_eq!(opacity_for(100_000), 0.25);
    }
}
