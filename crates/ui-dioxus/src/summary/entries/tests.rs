use super::*;
use trip_archive_types::SummaryCollection;

use ActivityType::{Cycling, Hiking};

fn counted(id: i64, start: &str, activity: ActivityType) -> StatsTrip {
    StatsTrip {
        id,
        name: format!("Trip {id}"),
        activity_type: activity,
        start_date: start.to_string(),
        end_date: start.to_string(),
        distance_m: 1000.0,
        ascent_m: None,
        descent_m: None,
        moving_secs: None,
        moving_distance_m: None,
        climb_gain_m: None,
        climb_secs: None,
    }
}

fn uncounted(id: i64, kind: TripKind, start: Option<&str>) -> UncountedTrip {
    UncountedTrip {
        id,
        name: format!("Trip {id}"),
        activity_type: Hiking,
        trip_kind: kind,
        start_date: start.map(str::to_string),
        distance_m: 2000.0,
    }
}

fn track(id: i64) -> SummaryTrack {
    SummaryTrack {
        id,
        name: format!("Track {id}"),
        collection_name: "Lakes".to_string(),
        activity_type: Cycling,
        distance_m: 3000.0,
    }
}

/// One of everything (the story's "Tested" line): recorded trip 1, recorded
/// trip 2 without timestamps, planned trip 3, collection 10 with tracks 1
/// and 2 — sharing ids with trips — and track 5 of an untagged collection.
fn everything(tags: Vec<TagTrips>) -> TagSummaries {
    TagSummaries {
        tags,
        trips: vec![counted(1, "2024-07-02", Hiking)],
        uncounted: vec![
            uncounted(3, TripKind::Planned, Some("2024-07-01")),
            uncounted(2, TripKind::Recorded, None),
        ],
        collections: vec![SummaryCollection {
            id: 10,
            name: "Lakes".to_string(),
            tracks: vec![track(1), track(2)],
        }],
        collection_tracks: vec![track(5)],
    }
}

fn holding_everything(name: &str) -> TagTrips {
    TagTrips {
        name: name.to_string(),
        trip_ids: vec![1],
        undated: 1,
        uncounted_trip_ids: vec![3, 2],
        collection_ids: vec![10],
        collection_track_ids: vec![5],
    }
}

fn keys(entries: &[Entry]) -> Vec<(MapSource, i64)> {
    entries.iter().map(Entry::key).collect()
}

fn names(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            Row::Entry(entry) => entry.name.clone(),
            Row::Collection { name, .. } => name.clone(),
        })
        .collect()
}

use MapSource::{CollectionTracks, Trips};

#[test]
fn us107_trips_come_first_by_date_then_collections_then_tracks_on_their_own() {
    let groups = groups(&everything(vec![holding_everything("alps")]));

    assert_eq!(groups.len(), 1);
    // The planned trip by its date, before the recorded one; the untimed
    // one last.
    assert_eq!(
        names(&groups[0].rows),
        ["Trip 3", "Trip 1", "Trip 2", "Lakes", "Track 5"]
    );
    let Row::Collection { tracks, .. } = &groups[0].rows[3] else {
        panic!("a collection row");
    };
    assert_eq!(keys(tracks), [(CollectionTracks, 1), (CollectionTracks, 2)]);
}

#[test]
fn us107_each_entry_has_its_kind_and_only_the_counted_trip_goes_without_a_note() {
    let groups = groups(&everything(vec![holding_everything("alps")]));

    let entries: Vec<&Entry> = groups[0]
        .rows
        .iter()
        .flat_map(|row| match row {
            Row::Entry(entry) => vec![entry],
            Row::Collection { tracks, .. } => tracks.iter().collect(),
        })
        .collect();
    let kinds: Vec<(Kind, Option<Uncounted>)> = entries
        .iter()
        .map(|entry| (entry.kind, entry.uncounted))
        .collect();
    assert_eq!(
        kinds,
        [
            (Kind::Planned, Some(Uncounted::Planned)),
            (Kind::Recorded, None),
            (Kind::Recorded, Some(Uncounted::WithoutTimestamps)),
            (Kind::Collections, Some(Uncounted::FromCollection)),
            (Kind::Collections, Some(Uncounted::FromCollection)),
            (Kind::Collections, Some(Uncounted::FromCollection)),
        ]
    );
    // A collection track has no date.
    assert_eq!(entries[3].date, None);
}

#[test]
fn us107_the_map_draws_every_listed_entry_once_telling_trips_and_tracks_apart() {
    let data = everything(vec![
        holding_everything("alps"),
        holding_everything("norway"),
    ]);

    let drawn = drawn(&data);

    assert_eq!(
        keys(&drawn),
        [
            (Trips, 3),
            (Trips, 1),
            (Trips, 2),
            (CollectionTracks, 1),
            (CollectionTracks, 2),
            (CollectionTracks, 5),
        ]
    );
}

#[test]
fn us107_with_one_tag_each_entry_gets_a_shade_of_its_activity_in_list_order() {
    let drawn = drawn(&everything(vec![holding_everything("alps")]));

    let hiking = activity_color::shades(Hiking);
    let cycling = activity_color::shades(Cycling);
    let colors: Vec<&str> = drawn.iter().map(|entry| entry.color).collect();
    assert_eq!(
        colors,
        [hiking[0], hiking[1], hiking[2], cycling[0], cycling[1], cycling[2]]
    );
}

#[test]
fn us107_with_several_tags_an_entry_wears_its_first_chosen_tags_color_under_each() {
    let norway = TagTrips {
        name: "norway".to_string(),
        collection_track_ids: vec![5],
        ..TagTrips::default()
    };
    let data = everything(vec![norway, holding_everything("alps")]);

    let groups = groups(&data);

    assert_eq!(groups[0].color, Some(tag_color(0)));
    let Row::Entry(lone) = &groups[1].rows[4] else {
        panic!("the lone track");
    };
    assert_eq!(lone.color, tag_color(0));
    let Row::Collection { color, tracks, .. } = &groups[1].rows[3] else {
        panic!("a collection row");
    };
    assert_eq!(*color, Some(tag_color(1)));
    assert_eq!(tracks[0].color, tag_color(1));
}

#[test]
fn us107_a_tag_holding_nothing_has_no_group() {
    let empty = TagTrips {
        name: "empty".to_string(),
        ..TagTrips::default()
    };

    let groups = groups(&everything(vec![holding_everything("alps"), empty]));

    assert_eq!(groups.len(), 1);
}

#[test]
fn us107_a_tag_with_no_recorded_trip_still_lists_what_it_holds() {
    let planned_only = TagTrips {
        name: "someday".to_string(),
        uncounted_trip_ids: vec![3],
        ..TagTrips::default()
    };

    let groups = groups(&everything(vec![planned_only]));

    assert_eq!(names(&groups[0].rows), ["Trip 3"]);
}
