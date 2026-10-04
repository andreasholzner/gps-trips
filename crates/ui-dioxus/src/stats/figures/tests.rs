//! US-77's figures, on hand-made trips (ADR-0012).

use super::*;
use time::macros::date;

fn trip(id: i64, activity: ActivityType, start: &str, end: &str, km: f64) -> StatsTrip {
    StatsTrip {
        id,
        name: format!("Trip {id}"),
        activity_type: activity,
        start_date: start.to_string(),
        end_date: end.to_string(),
        distance_m: km * 1000.0,
        ascent_m: Some(km * 100.0),
        descent_m: Some(km * 100.0),
        moving_secs: Some((km * 900.0) as i64),
        moving_distance_m: Some(km * 1000.0),
    }
}

use ActivityType::{Cycling, Hiking, Kayaking};

/// Two years of trips: hikes and rides in 2024, one multi-day hike over New
/// Year, a paddle in 2025.
fn archive() -> Vec<StatsTrip> {
    vec![
        trip(1, Hiking, "2024-03-10", "2024-03-10", 10.0),
        trip(2, Cycling, "2024-03-10", "2024-03-10", 40.0),
        trip(3, Hiking, "2024-07-01", "2024-07-03", 30.0),
        trip(4, Hiking, "2024-12-31", "2025-01-01", 20.0),
        trip(5, Kayaking, "2025-05-05", "2025-05-05", 8.0),
    ]
}

fn view(year: Option<i32>, measure: Measure, activities: &[ActivityType]) -> StatsView {
    StatsView {
        year,
        measure,
        activities: activities.to_vec(),
    }
}

#[test]
fn us77_the_controls_offer_the_years_and_activities_there_are() {
    let trips = archive();
    let dated = dated(&trips, &[]);

    assert_eq!(years(&dated), [2025, 2024]);
    assert_eq!(activities(&dated), [Hiking, Cycling, Kayaking]);
}

#[test]
fn us77_all_years_give_a_column_per_year_and_a_row_per_activity_with_its_share() {
    let trips = archive();

    let totals = totals(&dated(&trips, &[]), &view(None, Measure::Distance, &[]));

    assert_eq!(totals.columns, ["2024", "2025"]);
    let hiking = &totals.rows[0];
    assert_eq!(hiking.activity, Some(Hiking));
    assert_eq!(hiking.values, [60.0, 0.0]);
    assert_eq!(hiking.total, 60.0);
    assert_eq!(hiking.share, Some(60.0 / 108.0));
    let sum = totals.sum.expect("a sum row with all activities");
    assert_eq!(sum.values, [100.0, 8.0]);
    assert_eq!(sum.total, 108.0);
    let rows: Vec<_> = totals.rows.iter().map(|row| row.activity).collect();
    assert_eq!(rows, [Some(Hiking), Some(Cycling), Some(Kayaking)]);
}

#[test]
fn us77_one_year_gives_a_column_per_month_and_only_that_years_activities() {
    let trips = archive();

    let totals = totals(&dated(&trips, &[]), &view(Some(2024), Measure::Trips, &[]));

    assert_eq!(totals.columns.len(), 12);
    assert_eq!(totals.columns[0], "Jan");
    let sum = totals.sum.unwrap();
    assert_eq!(sum.values[2], 2.0, "two trips in March");
    assert_eq!(sum.values[6], 1.0);
    assert_eq!(sum.values[11], 1.0, "a trip counts in the month it starts");
    assert_eq!(sum.total, 4.0);
    let rows: Vec<_> = totals.rows.iter().map(|row| row.activity).collect();
    assert_eq!(rows, [Some(Hiking), Some(Cycling)]);
}

#[test]
fn us77_a_chosen_activity_has_its_row_alone_and_no_share() {
    let trips = archive();
    let view = view(None, Measure::Ascent, &[Hiking]);

    let totals = totals(&dated(&trips, &view.activities), &view);

    assert_eq!(totals.rows.len(), 1);
    assert_eq!(totals.rows[0].values, [6000.0, 0.0]);
    assert_eq!(totals.rows[0].share, None);
    assert_eq!(totals.sum, None);
}

#[test]
fn us77_several_chosen_activities_get_a_row_each_their_shares_and_their_sum() {
    let trips = archive();
    let view = view(None, Measure::Distance, &[Hiking, Kayaking]);
    let dated = dated(&trips, &view.activities);

    let totals = totals(&dated, &view);

    let rows: Vec<_> = totals.rows.iter().map(|row| row.activity).collect();
    assert_eq!(rows, [Some(Hiking), Some(Kayaking)]);
    let sum = totals.sum.expect("a sum of the chosen activities");
    assert_eq!(sum.total, 68.0);
    assert_eq!(totals.rows[1].share, Some(8.0 / 68.0));
    let records = records(&dated, &view);
    let rows: Vec<_> = records.iter().map(|row| row.activity).collect();
    assert_eq!(rows, [None, Some(Hiking), Some(Kayaking)]);
    assert_eq!(records[0].longest[0].id, 3, "the ride is not chosen");
}

#[test]
fn us77_days_out_are_distinct_dates_covered_wherever_they_fall() {
    let trips = archive();

    let totals = totals(&dated(&trips, &[]), &view(None, Measure::DaysOut, &[]));

    // 2024: 10 Mar (two trips, one day), 1–3 Jul, 31 Dec. 2025: 1 Jan, 5 May.
    let sum = totals.sum.unwrap();
    assert_eq!(sum.values, [5.0, 2.0]);
    assert_eq!(sum.total, 7.0);
    assert_eq!(totals.rows[0].values, [5.0, 1.0], "hiking");
}

#[test]
fn us77_moving_time_adds_up_in_hours() {
    let trips = vec![trip(1, Hiking, "2024-03-10", "2024-03-10", 4.0)];

    let totals = totals(&dated(&trips, &[]), &view(None, Measure::MovingTime, &[]));

    assert_eq!(totals.sum.unwrap().total, 1.0);
}

#[test]
fn us77_a_trip_without_moving_time_or_ascent_adds_nothing_to_them() {
    let mut bare = trip(1, Hiking, "2024-03-10", "2024-03-10", 4.0);
    bare.ascent_m = None;
    bare.moving_secs = None;
    let trips = vec![bare];
    let dated = dated(&trips, &[]);

    assert_eq!(
        totals(&dated, &view(None, Measure::Ascent, &[]))
            .sum
            .unwrap()
            .total,
        0.0
    );
    assert_eq!(
        totals(&dated, &view(None, Measure::MovingTime, &[]))
            .sum
            .unwrap()
            .total,
        0.0
    );
}

#[test]
fn us77_one_trips_days_are_capped_at_a_year() {
    let trips = vec![trip(1, Hiking, "2024-01-01", "2030-01-01", 4.0)];

    let totals = totals(&dated(&trips, &[]), &view(None, Measure::DaysOut, &[]));

    assert_eq!(totals.sum.unwrap().total, 366.0);
}

#[test]
fn us77_the_running_total_compares_this_year_with_last_at_the_same_date() {
    let trips = vec![
        trip(1, Hiking, "2024-02-01", "2024-02-01", 10.0),
        trip(2, Hiking, "2024-11-01", "2024-11-01", 50.0),
        trip(3, Hiking, "2025-01-15", "2025-01-15", 7.0),
    ];
    let today = date!(2025 - 06 - 01);

    let running = running(&dated(&trips, &[]), Measure::Distance, None, today);

    assert_eq!(running.highlighted, 2025);
    assert_eq!(running.this_year, 7.0);
    // By 1 June 2024 only February's trip had been done.
    assert_eq!(running.last_year, 10.0);
    let (year, values) = &running.years[0];
    assert_eq!(*year, 2024);
    assert_eq!(values.len(), 366);
    assert_eq!(values[30], Some(0.0), "31 January");
    assert_eq!(values[31], Some(10.0), "1 February");
    assert_eq!(values[365], Some(60.0));
    // This year's line stops at today.
    let (_, values) = &running.years[1];
    let today_index = today.ordinal() as usize - 1;
    assert_eq!(values[today_index], Some(7.0));
    assert_eq!(values[today_index + 1], None);
}

#[test]
fn us77_the_running_total_highlights_the_chosen_year_and_counts_days_out() {
    let trips = archive();

    let running = running(
        &dated(&trips, &[]),
        Measure::DaysOut,
        Some(2024),
        date!(2025 - 06 - 01),
    );

    assert_eq!(running.highlighted, 2024);
    assert_eq!(running.years[0].1[365], Some(5.0));
    assert_eq!(running.this_year, 2.0);
}

#[test]
fn us77_on_29_february_last_year_is_read_at_28_february() {
    let trips = vec![trip(1, Hiking, "2023-02-28", "2023-02-28", 3.0)];

    let running = running(
        &dated(&trips, &[]),
        Measure::Distance,
        None,
        date!(2024 - 02 - 29),
    );

    assert_eq!(running.last_year, 3.0);
}

#[test]
fn us77_the_months_start_where_a_leap_year_has_them() {
    let starts = month_starts();

    assert_eq!(starts.len(), 12);
    assert_eq!(starts[..3], [0, 31, 60]);
    assert_eq!(starts[11], 335);
}

#[test]
fn us77_every_day_of_a_leap_year_has_a_name() {
    let labels = day_labels();

    assert_eq!(labels.len(), 366);
    assert_eq!(labels[0], "1 Jan");
    assert_eq!(labels[59], "29 Feb");
    assert_eq!(labels[365], "31 Dec");
}

#[test]
fn us77_records_come_overall_and_per_activity() {
    let trips = archive();

    let records = records(&dated(&trips, &[]), &view(None, Measure::Distance, &[]));

    let activities: Vec<_> = records.iter().map(|row| row.activity).collect();
    assert_eq!(
        activities,
        [None, Some(Hiking), Some(Cycling), Some(Kayaking)]
    );
    let overall = &records[0];
    assert_eq!(overall.longest[0].id, 2);
    assert_eq!(overall.longest[0].value, 40.0);
    assert_eq!(overall.most_ascent[0].value, 4000.0);
    // 10 March: a hike and a ride, 50 km between them.
    let day = &overall.longest_day[0];
    assert_eq!(day.date, "2024-03-10");
    assert_eq!(day.km, 50.0);
    assert_eq!(
        day.trips,
        [(1, "Trip 1".to_string()), (2, "Trip 2".to_string())]
    );
    let hiking = &records[1];
    assert_eq!(hiking.longest[0].id, 3);
    assert_eq!(hiking.longest_day[0].date, "2024-07-01");
}

#[test]
fn us77_records_keep_to_the_chosen_year_and_activity() {
    let trips = archive();
    let view = view(Some(2025), Measure::Distance, &[Kayaking]);

    let records = records(&dated(&trips, &view.activities), &view);

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].activity, Some(Kayaking));
    assert_eq!(records[0].longest[0].id, 5);
}

#[test]
fn us77_a_period_without_trips_has_no_records() {
    let trips = archive();

    let records = records(
        &dated(&trips, &[]),
        &view(Some(2019), Measure::Distance, &[]),
    );

    assert_eq!(records.len(), 1);
    assert!(records[0].longest.is_empty());
    assert!(records[0].longest_day.is_empty());
}

#[test]
fn us77_each_record_lists_the_top_three_best_first() {
    let trips = archive();

    let records = records(&dated(&trips, &[]), &view(None, Measure::Distance, &[]));

    let overall = &records[0];
    let longest: Vec<(i64, f64)> = overall.longest.iter().map(|r| (r.id, r.value)).collect();
    assert_eq!(longest, [(2, 40.0), (3, 30.0), (4, 20.0)]);
    let ascent: Vec<i64> = overall.most_ascent.iter().map(|r| r.id).collect();
    assert_eq!(ascent, [2, 3, 4]);
    // 10 March's two trips together, then 1 July, then 31 December.
    let days: Vec<(&str, f64)> = overall
        .longest_day
        .iter()
        .map(|day| (day.date.as_str(), day.km))
        .collect();
    assert_eq!(
        days,
        [
            ("2024-03-10", 50.0),
            ("2024-07-01", 30.0),
            ("2024-12-31", 20.0)
        ]
    );
    // An activity with fewer trips lists what there is.
    let kayaking = records.last().unwrap();
    assert_eq!(kayaking.longest.len(), 1);
}

#[test]
fn us77_equal_records_keep_the_earlier_trip_first() {
    let trips = vec![
        trip(1, Hiking, "2024-03-10", "2024-03-10", 10.0),
        trip(2, Hiking, "2024-04-10", "2024-04-10", 10.0),
    ];

    let records = records(&dated(&trips, &[]), &view(None, Measure::Distance, &[]));

    let ids: Vec<i64> = records[0].longest.iter().map(|r| r.id).collect();
    assert_eq!(ids, [1, 2]);
}
