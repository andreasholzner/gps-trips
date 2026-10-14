//! The trip list's heat map (US-63): one translucent mark per listed trip,
//! so that where trips pile up the marks overlap and darken — a heat map by
//! superposition rather than by plugin. Each mark is in its trip's activity
//! color (US-75).
//!
//! Which marks are drawn and how heavy each is gets decided here, where
//! `cargo test` reaches it; the region map's script only draws what it is
//! handed (ADR-0025).

use serde::Serialize;
use trip_archive_types::ActivityType;

use crate::activity_color;
use crate::map_items::MapItem;

/// How opaque a lone trip's mark is, and the floor no mark goes below. The
/// floor keeps every mark visible on its own — zoomed in, where marks no
/// longer overlap, each one is drawn at exactly this — and the ceiling keeps
/// a single mark from hiding the map under it.
const MAX_OPACITY: f64 = 0.6;
const MIN_OPACITY: f64 = 0.25;

/// What the region map draws: every mark at the same opacity.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HeatMarks {
    pub marks: Vec<HeatMark>,
    pub opacity: f64,
    /// `[lat, lon]` points "Fit to items" fits the map to: the corners of
    /// every marked item's bounding box — not its mark, so a lone item is
    /// shown whole rather than zoomed in on — and the list's waypoints
    /// (US-87).
    pub fit: Vec<[f64; 2]>,
}

/// One trip's mark.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HeatMark {
    /// `[lat, lon]`, the order Leaflet takes.
    pub at: [f64; 2],
    /// Kept for the legend; the script only needs the color.
    #[serde(skip)]
    pub activity: ActivityType,
    pub color: &'static str,
}

impl HeatMarks {
    /// The activities of the trips that got a mark, for the legend.
    pub fn activities(&self) -> Vec<ActivityType> {
        self.marks.iter().map(|mark| mark.activity).collect()
    }

    /// The same marks, fitted to `points` as well.
    pub fn also_fitting(mut self, points: &[[f64; 2]]) -> Self {
        self.fit.extend_from_slice(points);
        self
    }

    /// Every point the map is fitted to: the items' corners, then the
    /// others.
    pub fn fit_points(&self) -> Vec<[f64; 2]> {
        self.fit.clone()
    }
}

/// One mark per trip or collection track, at the centre of its stored
/// bounding box. The centre
/// rather than the box: a long tour's box spans hundreds of kilometres and
/// says little about where the tour was, while its centre answers "roughly
/// where". A trip with no box — a track without points — gets no mark.
///
/// The opacity falls as `1/√n` with the number of marks, so a few trips
/// stay clearly visible while hundreds do not merge into one solid blob.
pub fn marks(items: &[MapItem]) -> HeatMarks {
    let boxes: Vec<_> = items
        .iter()
        .filter_map(|item| Some((item, item.bbox?)))
        .collect();
    let marks: Vec<HeatMark> = boxes
        .iter()
        .map(|&(item, bbox)| HeatMark {
            at: [
                (bbox.min_lat + bbox.max_lat) / 2.0,
                (bbox.min_lon + bbox.max_lon) / 2.0,
            ],
            activity: item.activity_type,
            color: activity_color::color(item.activity_type),
        })
        .collect();
    let fit = boxes
        .iter()
        .flat_map(|(_, bbox)| [[bbox.min_lat, bbox.min_lon], [bbox.max_lat, bbox.max_lon]])
        .collect();
    let opacity = opacity_for(marks.len());
    HeatMarks {
        marks,
        opacity,
        fit,
    }
}

fn opacity_for(count: usize) -> f64 {
    let count = count.max(1) as f64;
    (MAX_OPACITY / count.sqrt()).clamp(MIN_OPACITY, MAX_OPACITY)
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use trip_archive_types::{ActivityType, TripKind, TripSummary};

    fn positions(marks: &HeatMarks) -> Vec<[f64; 2]> {
        marks.marks.iter().map(|mark| mark.at).collect()
    }

    fn a_trip(bbox: Option<[f64; 4]>) -> MapItem {
        let [min_lat, min_lon, max_lat, max_lon] = match bbox {
            Some(corners) => corners.map(Some),
            None => [None; 4],
        };
        MapItem::from(&TripSummary {
            id: 1,
            name: "Trip".to_string(),
            activity_type: ActivityType::Hiking,
            start_time: None,
            start_date: None,
            distance_m: 1_000.0,
            ascent_m: None,
            duration_secs: None,
            trip_kind: TripKind::Recorded,
            privacy_status: None,
            min_lat,
            min_lon,
            max_lat,
            max_lon,
            has_recorded_trip: false,
        })
    }

    #[test]
    fn a_trip_is_marked_at_the_centre_of_its_bounding_box() {
        let marks = marks(&[a_trip(Some([59.0, 10.0, 61.0, 12.0]))]);

        assert_eq!(positions(&marks), vec![[60.0, 11.0]]);
    }

    #[test]
    fn a_mark_is_in_its_trips_activity_color() {
        // US-75.
        let mut ride = a_trip(Some([47.0, 11.0, 47.5, 11.5]));
        ride.activity_type = ActivityType::Cycling;

        let marks = marks(&[a_trip(Some([59.0, 10.0, 61.0, 12.0])), ride]);

        let colors: Vec<_> = marks.marks.iter().map(|mark| mark.color).collect();
        assert_eq!(colors, ["#b2182b", "#1f4e9c"]);
    }

    #[test]
    fn the_activities_shown_are_those_that_got_a_mark() {
        let mut boxless = a_trip(None);
        boxless.activity_type = ActivityType::Kayaking;

        let marks = marks(&[a_trip(Some([59.0, 10.0, 61.0, 12.0])), boxless]);

        assert_eq!(marks.activities(), vec![ActivityType::Hiking]);
    }

    #[test]
    fn a_trip_without_a_bounding_box_gets_no_mark() {
        let marks = marks(&[a_trip(None), a_trip(Some([47.0, 11.0, 47.5, 11.5]))]);

        assert_eq!(positions(&marks), vec![[47.25, 11.25]]);
    }

    #[test]
    fn an_empty_list_draws_nothing() {
        assert!(marks(&[]).marks.is_empty());
    }

    #[test]
    fn the_map_is_fitted_to_every_marked_items_bounding_box() {
        // Its corners, not its mark: a lone item's mark is a single point,
        // which the map would zoom right in on rather than show the item.
        let marks = marks(&[a_trip(Some([59.0, 10.0, 61.0, 12.0])), a_trip(None)]);

        assert_eq!(marks.fit_points(), vec![[59.0, 10.0], [61.0, 12.0]]);
    }

    #[test]
    fn us87_the_map_is_fitted_to_what_the_list_adds_too() {
        let marks = marks(&[a_trip(Some([59.0, 10.0, 61.0, 12.0]))])
            .also_fitting(&[[46.49, 9.17], [46.58, 9.84]]);

        assert_eq!(
            marks.fit_points(),
            vec![[59.0, 10.0], [61.0, 12.0], [46.49, 9.17], [46.58, 9.84]]
        );
        assert_eq!(marks.marks.len(), 1, "nothing more is marked");
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
