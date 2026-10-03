//! US-78's figures, on hand-made trips (ADR-0012).

use super::*;
use trip_archive_types::{StatsTrip, TagTrips};

use ActivityType::{Cycling, Hiking, Kayaking};

fn trip(id: i64, activity: ActivityType, start: &str, end: &str, km: f64) -> StatsTrip {
    StatsTrip {
        id,
        name: format!("Trip {id}"),
        activity_type: activity,
        start_date: start.to_string(),
        end_date: end.to_string(),
        distance_m: km * 1000.0,
        ascent_m: Some(km * 100.0),
        descent_m: Some(km * 50.0),
        moving_secs: Some((km * 360.0) as i64),
    }
}

fn tag(name: &str, trip_ids: &[i64], undated: u32) -> TagTrips {
    TagTrips {
        name: name.to_string(),
        trip_ids: trip_ids.to_vec(),
        undated,
    }
}

/// A week in the Alps — hikes and a ride, one hike over two days — and a
/// paddle in Norway; trip 2 is under both tags.
fn archive() -> TagSummaries {
    TagSummaries {
        tags: vec![tag("alps", &[1, 2, 3, 4], 1), tag("norway", &[2, 5], 0)],
        trips: vec![
            trip(1, Hiking, "2024-07-01", "2024-07-02", 20.0),
            trip(2, Cycling, "2024-07-02", "2024-07-02", 60.0),
            trip(3, Hiking, "2024-07-04", "2024-07-04", 12.0),
            trip(4, Hiking, "2024-07-04", "2024-07-04", 8.0),
            trip(5, Kayaking, "2024-08-10", "2024-08-10", 15.0),
        ],
    }
}

fn sums_of(figures: &TagFigures, activity: ActivityType) -> &Sums {
    figures
        .of(activity)
        .unwrap_or_else(|| panic!("no {activity:?} figures"))
}

#[test]
fn us78_there_is_a_summary_per_chosen_tag_in_the_order_chosen() {
    let summaries = summaries(&archive());

    let names: Vec<&str> = summaries.iter().map(|tag| tag.name.as_str()).collect();
    assert_eq!(names, ["alps", "norway"]);
    assert_eq!(summaries[0].undated, 1);
    assert_eq!(summaries[1].undated, 0);
}

#[test]
fn us78_a_tag_spans_its_first_trips_start_to_its_last_trips_end() {
    let mut archive = archive();
    // Started before the others ended, ending last of all.
    archive.trips[0].end_date = "2024-07-09".to_string();

    let summaries = summaries(&archive);
    let alps = summaries[0].figures.as_ref().unwrap();

    assert_eq!(alps.first, "2024-07-01");
    assert_eq!(alps.last, "2024-07-09");
}

#[test]
fn us78_each_activity_adds_up_its_trips() {
    let summaries = summaries(&archive());
    let alps = summaries[0].figures.as_ref().unwrap();

    let hiking = sums_of(alps, Hiking);
    assert_eq!(hiking.trips, 3);
    // 1 and 2 July, and 4 July twice over.
    assert_eq!(hiking.days_out, 3);
    assert!((hiking.km - 40.0).abs() < 1e-9);
    assert!((hiking.ascent_m - 4000.0).abs() < 1e-9);
    assert!((hiking.descent_m - 2000.0).abs() < 1e-9);
    assert!((hiking.moving_hours - 4.0).abs() < 1e-9);
    assert_eq!(sums_of(alps, Cycling).trips, 1);
    assert!(alps.of(Kayaking).is_none());
}

#[test]
fn us78_the_activities_listed_follow_the_import_forms_order() {
    let summaries = summaries(&archive());
    let alps = summaries[0].figures.as_ref().unwrap();

    let activities: Vec<ActivityType> = alps.activities.iter().map(|(a, _)| *a).collect();
    assert_eq!(activities, [Hiking, Cycling]);
}

#[test]
fn us78_together_a_day_out_is_counted_once_whatever_was_done_on_it() {
    let summaries = summaries(&archive());
    let alps = summaries[0].figures.as_ref().unwrap();

    // 2 July was hiked and ridden: two activities' day, one day out.
    assert_eq!(alps.together.trips, 4);
    assert_eq!(alps.together.days_out, 3);
    assert!((alps.together.km - 100.0).abs() < 1e-9);
}

#[test]
fn us78_a_trip_under_two_chosen_tags_counts_in_each() {
    let summaries = summaries(&archive());
    let norway = summaries[1].figures.as_ref().unwrap();

    assert_eq!(norway.together.trips, 2);
    assert_eq!(sums_of(norway, Cycling).trips, 1);
    assert_eq!(norway.first, "2024-07-02");
    assert_eq!(norway.last, "2024-08-10");
}

#[test]
fn us78_the_longest_day_is_the_most_distance_started_on_one_date() {
    let mut archive = archive();
    // 4 July's two hikes, 20 km together, outdo neither the ride's 60 km —
    // so make them.
    archive.trips[2].distance_m = 50_000.0;
    archive.trips[3].distance_m = 30_000.0;

    let summaries = summaries(&archive);
    let day = summaries[0]
        .figures
        .as_ref()
        .unwrap()
        .longest_day
        .clone()
        .unwrap();

    assert_eq!(day.date, "2024-07-04");
    assert!((day.km - 80.0).abs() < 1e-9);
    let ids: Vec<i64> = day.trips.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, [3, 4]);
}

#[test]
fn us78_a_missing_ascent_or_descent_adds_nothing() {
    let mut archive = archive();
    archive.trips[2].ascent_m = None;
    archive.trips[2].descent_m = None;

    let summaries = summaries(&archive);
    let hiking = sums_of(summaries[0].figures.as_ref().unwrap(), Hiking);

    assert!((hiking.ascent_m - 2800.0).abs() < 1e-9);
    assert!((hiking.descent_m - 1400.0).abs() < 1e-9);
}

#[test]
fn us78_a_tag_without_recorded_trips_has_no_figures() {
    let mut archive = archive();
    archive.tags.push(tag("empty", &[], 2));

    let summaries = summaries(&archive);

    assert!(summaries[2].figures.is_none());
    assert_eq!(summaries[2].undated, 2);
}

#[test]
fn us78_the_table_lists_every_activity_any_tag_holds() {
    let summaries = summaries(&archive());

    assert_eq!(activities(&summaries), [Hiking, Cycling, Kayaking]);
}
