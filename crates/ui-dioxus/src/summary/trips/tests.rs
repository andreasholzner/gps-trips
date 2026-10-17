use super::*;
use crate::summary::lines;
use crate::test_support::render;
use trip_archive_types::{
    ActivityType, StatsTrip, SummaryCollection, SummaryTrack, TagTrips, TripKind, UncountedTrip,
};

fn trip(id: i64, start: &str) -> StatsTrip {
    StatsTrip {
        id,
        name: format!("Trip {id}"),
        activity_type: ActivityType::Hiking,
        start_date: start.to_string(),
        end_date: start.to_string(),
        distance_m: 12_300.0,
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
        ..TagTrips::default()
    }
}

fn data(tags: Vec<TagTrips>) -> TagSummaries {
    TagSummaries {
        tags,
        trips: vec![
            trip(1, "2024-07-01"),
            trip(2, "2024-07-02"),
            trip(3, "2024-08-10"),
        ],
        ..TagSummaries::default()
    }
}

fn shown(data: TagSummaries) -> String {
    render(move || rsx! { TripsByTag { summary: data.clone(), viewer: Viewer::Owner } })
}

#[test]
fn us78_the_rows_link_to_their_trips_with_date_activity_and_distance() {
    let html = shown(data(vec![tag("alps", &[1, 2])]));

    assert!(html.contains(r#"href="/trips/1""#), "{html}");
    assert!(html.contains("Trip 2"), "{html}");
    assert!(html.contains("1. Jul. 2024"), "{html}");
    assert!(html.contains("Hiking"), "{html}");
    assert!(html.contains("12.30 km"), "{html}");
    // One tag: no heading repeating its name.
    assert!(!html.contains("<h3"), "{html}");
}

#[test]
fn us78_several_tags_head_their_trips_with_their_name_and_color() {
    let html = shown(data(vec![tag("norway", &[2, 3]), tag("alps", &[1, 2])]));

    assert_eq!(html.matches("<h3").count(), 2, "{html}");
    assert_eq!(html.matches(r#"href="/trips/2""#).count(), 2, "{html}");
    assert!(html.contains(lines::tag_color(1)), "{html}");
}

/// The story's "Tested" line: a recorded trip, a recorded trip without
/// timestamps, a planned trip, a tagged collection and a tagged track of an
/// untagged collection.
fn everything() -> TagSummaries {
    let track = |id: i64, name: &str| SummaryTrack {
        id,
        name: name.to_string(),
        collection_name: "Lakes".to_string(),
        activity_type: ActivityType::Cycling,
        distance_m: 4_000.0,
    };
    let uncounted = |id: i64, kind: TripKind, start: Option<&str>| UncountedTrip {
        id,
        name: format!("Trip {id}"),
        activity_type: ActivityType::Hiking,
        trip_kind: kind,
        start_date: start.map(str::to_string),
        distance_m: 2_000.0,
    };
    TagSummaries {
        tags: vec![TagTrips {
            name: "alps".to_string(),
            trip_ids: vec![1],
            undated: 1,
            uncounted_trip_ids: vec![3, 2],
            collection_ids: vec![10],
            collection_track_ids: vec![5],
        }],
        trips: vec![trip(1, "2024-07-01")],
        uncounted: vec![
            uncounted(3, TripKind::Planned, Some("2024-07-05")),
            uncounted(2, TripKind::Recorded, None),
        ],
        collections: vec![SummaryCollection {
            id: 10,
            name: "Lakes".to_string(),
            tracks: vec![track(11, "Stage 1")],
        }],
        collection_tracks: vec![track(5, "Nordkette")],
    }
}

/// The table row holding `text`.
fn row_of<'a>(html: &'a str, text: &str) -> &'a str {
    html.split("<tr")
        .find(|row| row.contains(text))
        .unwrap_or_else(|| panic!("no row with {text}: {html}"))
}

#[test]
fn us107_everything_a_tag_holds_is_listed_with_its_kind_in_front() {
    let html = shown(everything());

    for (text, kind, href) in [
        ("Trip 1", "Recorded trip", "/trips/1"),
        ("Trip 2", "Recorded trip", "/trips/2"),
        ("Trip 3", "Planned trip", "/trips/3"),
        ("Lakes", "Collection", "/collections/10"),
        ("Stage 1", "Collection track", "/collection-tracks/11"),
        ("Nordkette", "Collection track", "/collection-tracks/5"),
    ] {
        let row = row_of(&html, text);
        assert!(row.contains(&format!(r#"aria-label="{kind}""#)), "{row}");
        assert!(row.contains(&format!(r#"href="{href}"#)), "{row}");
    }
    // Planned dashed, collection tracks dotted (US-90).
    assert!(row_of(&html, "Trip 3").contains("8 6"), "{html}");
    assert!(row_of(&html, "Nordkette").contains("1 7"), "{html}");
    // A collection track has its activity and distance, and no date.
    let track = row_of(&html, "Nordkette");
    assert!(
        track.contains("Cycling") && track.contains("4.00 km"),
        "{track}"
    );
    assert!(!track.contains("2024"), "{track}");
}

#[test]
fn us107_only_the_counted_trip_goes_without_a_not_counted_note_saying_why() {
    let html = shown(everything());

    assert!(!row_of(&html, "Trip 1").contains("not counted"), "{html}");
    for (text, why) in [
        ("Trip 2", "without timestamps"),
        ("Trip 3", "planned"),
        ("Lakes", "from a collection"),
        ("Stage 1", "from a collection"),
        ("Nordkette", "from a collection"),
    ] {
        let row = row_of(&html, text);
        assert!(row.contains("not counted"), "{row}");
        assert!(
            row.contains(&format!(r#"title="Not counted: {why}""#)),
            "{row}"
        );
        assert!(row.contains(&format!(": {why}</span>")), "{row}");
    }
}

#[test]
fn us107_a_shared_summary_lists_its_tracks_for_its_recipient() {
    let html = render(move || {
        rsx! { TripsByTag { summary: data(vec![tag("alps", &[1])]), viewer: Viewer::Share("t0k".to_string()) } }
    });

    assert!(html.contains(r#"href="/s/t0k/trips/1""#), "{html}");
    assert!(!html.contains("not counted"), "{html}");
}
