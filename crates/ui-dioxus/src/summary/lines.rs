//! The tag summary's map (US-78): every trip under the chosen tags as a
//! line, as a share's overview map draws them (US-53). Which line is which
//! color is decided here, where `cargo test` reaches it; the script only
//! draws what it is handed (ADR-0025).

use trip_archive_types::{TagSummaries, TripTrack};

use crate::activity_color;
use crate::interop::OverviewLine;
use crate::stats::year_colors::SLOTS;
use crate::track;

/// The color of the `index`th chosen tag, with several chosen: the running
/// chart's categorical palette, in the order the tags were chosen, starting
/// over past its end.
pub fn tag_color(index: usize) -> &'static str {
    SLOTS[index % SLOTS.len()].0
}

/// Each trip's color, in the order of `data.trips`: with one tag, a shade of
/// its activity's color in date order, as a share's trips get (US-72); with
/// several, the color of the first chosen tag it is under. The map's lines
/// and the trip list's rows both take theirs from here.
pub fn trip_colors(data: &TagSummaries) -> Vec<&'static str> {
    if data.tags.len() == 1 {
        return activity_color::in_list_order(data.trips.iter().map(|trip| trip.activity_type));
    }
    data.trips
        .iter()
        .map(|trip| {
            let first = data
                .tags
                .iter()
                .position(|tag| tag.trip_ids.contains(&trip.id))
                .unwrap_or(0);
            tag_color(first)
        })
        .collect()
}

/// One line per trip whose track was read, each drawn once, in its
/// [`trip_colors`] color. The colors are taken over every trip, so one whose
/// track could not be read shifts no other.
pub fn lines(data: &TagSummaries, tracks: &[TripTrack]) -> Vec<OverviewLine> {
    data.trips
        .iter()
        .zip(trip_colors(data))
        .filter_map(|(trip, color)| {
            let track = tracks.iter().find(|track| track.id == trip.id)?;
            Some(OverviewLine {
                id: trip.id,
                name: trip.name.clone(),
                color,
                points: track::lat_lon(&track.coordinates),
            })
        })
        .collect()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use trip_archive_types::{ActivityType, StatsTrip, TagTrips};

    use ActivityType::{Cycling, Hiking};

    fn trip(id: i64, activity: ActivityType) -> StatsTrip {
        StatsTrip {
            id,
            name: format!("Trip {id}"),
            activity_type: activity,
            start_date: "2024-07-01".to_string(),
            end_date: "2024-07-01".to_string(),
            distance_m: 1000.0,
            ascent_m: None,
            descent_m: None,
            moving_secs: None,
            moving_distance_m: None,
            climb_gain_m: None,
            climb_secs: None,
        }
    }

    fn tag(name: &str, trip_ids: &[i64]) -> TagTrips {
        TagTrips {
            name: name.to_string(),
            trip_ids: trip_ids.to_vec(),
            undated: 0,
        }
    }

    fn track(id: i64) -> TripTrack {
        TripTrack {
            id,
            coordinates: vec![[10.0, 60.0], [10.1, 60.1]],
        }
    }

    fn colors(lines: &[OverviewLine]) -> Vec<(i64, &'static str)> {
        lines.iter().map(|line| (line.id, line.color)).collect()
    }

    #[test]
    fn us78_with_one_tag_each_trip_gets_a_shade_of_its_activity_in_date_order() {
        let data = TagSummaries {
            tags: vec![tag("alps", &[1, 2, 3])],
            trips: vec![trip(1, Hiking), trip(2, Cycling), trip(3, Hiking)],
        };

        let lines = lines(&data, &[track(1), track(2), track(3)]);

        let hiking = activity_color::shades(Hiking);
        let cycling = activity_color::shades(Cycling);
        assert_eq!(
            colors(&lines),
            [(1, hiking[0]), (2, cycling[0]), (3, hiking[1])]
        );
        assert_eq!(lines[0].name, "Trip 1");
        assert_eq!(lines[0].points[0], [60.0, 10.0]);
    }

    #[test]
    fn us78_with_several_tags_a_trip_is_drawn_once_in_its_first_chosen_tags_color() {
        let data = TagSummaries {
            tags: vec![tag("norway", &[2, 3]), tag("alps", &[1, 2])],
            trips: vec![trip(1, Hiking), trip(2, Cycling), trip(3, Hiking)],
        };

        let lines = lines(&data, &[track(1), track(2), track(3)]);

        assert_eq!(
            colors(&lines),
            [(1, tag_color(1)), (2, tag_color(0)), (3, tag_color(0))]
        );
    }

    #[test]
    fn us78_a_trip_without_a_track_shifts_no_other_trips_color() {
        let data = TagSummaries {
            tags: vec![tag("alps", &[1, 2])],
            trips: vec![trip(1, Hiking), trip(2, Hiking)],
        };

        let lines = lines(&data, &[track(2)]);

        assert_eq!(colors(&lines), [(2, activity_color::shades(Hiking)[1])]);
    }

    #[test]
    fn us78_every_chosen_tag_has_a_color_of_its_own_until_the_palette_runs_out() {
        let colors: Vec<&str> = (0..SLOTS.len()).map(tag_color).collect();
        let distinct: std::collections::HashSet<&str> = colors.iter().copied().collect();

        assert_eq!(distinct.len(), SLOTS.len());
        assert_eq!(tag_color(SLOTS.len()), tag_color(0));
    }
}
