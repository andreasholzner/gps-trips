use super::waypoints::{by_name, WaypointList};
use super::*;
use crate::test_support::{
    import_collection, render, render_against_archive, serve_test_archive, COLLECTION_GPX,
};

fn a_track(id: i64, name: &str, activity: ActivityType) -> CollectionTrackSummary {
    CollectionTrackSummary {
        id,
        name: name.to_string(),
        activity_type: activity,
        distance_m: 1_000.0,
        ascent_m: 100.0,
        descent_m: 50.0,
        has_recorded_trip: false,
    }
}

fn a_collection() -> CollectionDetail {
    CollectionDetail {
        id: 2,
        name: "Lakes & Passes".to_string(),
        description: "Ten tours".to_string(),
        activity_type: Some(ActivityType::Cycling),
        waypoint_count: 2,
        tracks: vec![a_track(1, "Stage 1", ActivityType::Cycling)],
        waypoints_with_recorded_trip: Vec::new(),
    }
}

// US-86: where an import ends — against a real archive (ADR-0012).
#[tokio::test]
async fn us86_the_page_shows_the_details_the_map_and_the_tracks() {
    let (archive, _dir) = serve_test_archive().await;
    let (id, tracks) = import_collection(
        &archive,
        COLLECTION_GPX,
        &[("name", "Lakes"), ("activity_type", "cycling")],
    )
    .await;

    let html = render_against_archive(
        &archive,
        move || rsx! { Collection { id } },
        |html| html.contains("collection-tracks"),
    )
    .await;

    assert!(html.contains(">Lakes</h1>"), "{html}");
    assert!(html.contains("3 tracks · 2 waypoints"), "{html}");
    assert!(html.contains("Ten tours around the lakes."), "{html}");
    assert!(
        html.contains(r#"<div id="overview-map" class="overview-map"></div>"#),
        "{html}"
    );
    for track in tracks {
        assert!(
            html.contains(&format!(r#"href="/collection-tracks/{track}""#)),
            "{track}: {html}"
        );
    }
    assert!(html.contains("Lakes 2"), "the unnamed track: {html}");
    assert!(html.contains(r#"id="edit-collection""#), "{html}");
    assert!(html.contains(r#"id="delete-collection""#), "{html}");
    // US-103: a check for links started from here.
    assert!(html.contains(r#"id="look-for-links""#), "{html}");
    // US-94: the whole collection is shared from here, and its tracks are
    // selected to share some of them.
    assert!(html.contains(r#"id="share-collection""#), "{html}");
    assert!(html.contains(r#"aria-label="Select all""#), "{html}");
}

// US-93: tagged as a trip is; US-107: each tag leading to its summary.
#[tokio::test]
async fn us93_the_page_shows_the_collections_own_tags() {
    let (archive, _dir) = serve_test_archive().await;
    let (id, tracks) = import_collection(&archive, COLLECTION_GPX, &[]).await;
    api::add_tag(&archive, api::Tagged::Collection(id), "lakes")
        .await
        .unwrap();
    api::add_tag(&archive, api::Tagged::CollectionTrack(tracks[0]), "pass")
        .await
        .unwrap();

    let html = render_against_archive(
        &archive,
        move || rsx! { Collection { id } },
        |html| html.contains("chip"),
    )
    .await;

    assert!(html.contains(r#"href="/summary?tags=lakes""#), "{html}");
    assert!(html.contains(r#"title="Remove lakes""#), "{html}");
    assert!(!html.contains("pass"), "a track's tag is its own: {html}");
    assert!(html.contains(r#"id="add-tag""#), "{html}");
}

// US-88: the files it came from, as uploaded.
#[tokio::test]
async fn us88_the_page_lists_the_files_to_download() {
    let (archive, _dir) = serve_test_archive().await;
    let (id, _) = import_collection(&archive, COLLECTION_GPX, &[]).await;
    let files = api::list_collection_files(&archive, id).await.unwrap();

    let html = render_against_archive(
        &archive,
        move || rsx! { Collection { id } },
        |html| html.contains("collection-files"),
    )
    .await;

    assert!(html.contains(r#"id="collection-files""#), "{html}");
    assert!(
        html.contains(&format!("/api/collection-files/{}/gpx", files[0].id)),
        "{html}"
    );
    assert!(html.contains(">book.gpx</a>"), "{html}");
}

#[tokio::test]
async fn us86_a_collection_that_no_longer_exists_says_so() {
    let (archive, _dir) = serve_test_archive().await;

    let html = render_against_archive(
        &archive,
        || rsx! { Collection { id: 9_999 } },
        |html| !html.contains("Loading"),
    )
    .await;

    assert!(html.contains("no such collection"), "{html}");
}

#[test]
fn us86_the_counts_read_as_words() {
    assert_eq!(counts(3, 2, " · "), "3 tracks · 2 waypoints");
    assert_eq!(counts(1, 1, " and "), "1 track and 1 waypoint");
    assert_eq!(counts(0, 46, " · "), "0 tracks · 46 waypoints");
}

#[test]
fn us86_each_track_is_drawn_in_its_activitys_color_and_a_track_not_read_shifts_none() {
    let tracks = [
        a_track(1, "Stage 1", ActivityType::Cycling),
        a_track(2, "Stage 2", ActivityType::Cycling),
        a_track(3, "Stage 3", ActivityType::Hiking),
    ];
    let positions = [
        TripTrack {
            id: 1,
            coordinates: vec![[9.32, 46.55]],
        },
        TripTrack {
            id: 3,
            coordinates: vec![[9.84, 46.58]],
        },
    ];

    let lines = collection_lines(&tracks, &positions);

    let colors = activity_color::in_list_order(tracks.iter().map(|t| t.activity_type));
    assert_eq!(lines.len(), 2);
    assert_eq!((lines[0].id, lines[0].color), (1, colors[0]));
    assert_eq!((lines[1].id, lines[1].color), (3, colors[2]));
    assert_eq!(lines[1].name, "Stage 3");
    // US-104: named in the map's hint after its activity's icon.
    assert_eq!(
        lines[1].icon,
        crate::activity_icon::map_icon(ActivityType::Hiking)
    );
    // Leaflet takes `[lat, lon]`.
    assert_eq!(lines[0].points, [[46.55, 9.32]]);
}

#[test]
fn us86_only_what_changed_is_sent() {
    let collection = a_collection();
    let mut form = CollectionForm::of(&collection);
    assert_eq!(changes(&collection, &form), CollectionEdit::default());

    form.description = " Eleven tours ".to_string();
    form.activity = String::new();
    assert_eq!(
        changes(&collection, &form),
        CollectionEdit {
            name: None,
            description: Some("Eleven tours".to_string()),
            activity_type: Some(String::new()),
        }
    );
}

#[test]
fn us86_the_form_starts_on_the_details() {
    let form = CollectionForm::of(&a_collection());

    assert_eq!(form.name, "Lakes & Passes");
    assert_eq!(form.description, "Ten tours");
    assert_eq!(form.activity, "cycling");
}

#[test]
fn us86_deleting_is_confirmed_naming_what_goes_with_it() {
    let html = render(|| {
        rsx! {
            ConfirmDeleteCollection { tracks: 3, waypoints: 1, on_confirm: |_| {}, on_cancel: |_| {} }
        }
    });

    assert!(
        html.contains("with its 3 tracks and 1 waypoint? This cannot be undone."),
        "{html}"
    );
}

#[test]
fn us94_the_owner_selects_tracks_to_share_them() {
    let html = render(|| {
        let tracks = vec![
            a_track(1, "Day 1", ActivityType::Hiking),
            a_track(2, "Day 2", ActivityType::Hiking),
        ];
        let selected = use_signal(|| std::collections::BTreeSet::from([2]));
        rsx! {
            TrackList { tracks, selected }
        }
    });
    assert_eq!(html.matches(r#"aria-label="Select""#).count(), 2, "{html}");
    assert!(html.contains(r#"aria-label="Select all""#), "{html}");
    assert_eq!(html.matches("checked").count(), 1, "{html}");

    let html = render(|| {
        let tracks = vec![a_track(1, "Day 1", ActivityType::Hiking)];
        rsx! { TrackList { tracks } }
    });
    assert!(!html.contains("checkbox"), "no selection, no boxes: {html}");
}

#[test]
fn us86_a_book_of_waypoints_alone_says_so_instead_of_an_empty_list() {
    let html = render(|| rsx! { TrackList { tracks: Vec::new() } });

    assert!(html.contains("waypoints only"), "{html}");
    assert!(!html.contains("<table"), "{html}");
}

#[test]
fn us86_waypoints_that_cannot_be_read_cost_the_markers_not_the_map() {
    assert!(drawn_waypoints(Err(api::ApiError::new("unreachable"))).is_empty());

    let read = trip_archive_types::Waypoint {
        id: 1,
        collection_track_id: None,
        name: Some("Albula Pass".to_string()),
        description: None,
        lat: 46.58,
        lon: 9.84,
        ele: None,
        symbol: None,
    };
    assert_eq!(
        drawn_waypoints(Ok(vec![read.clone()])),
        [MapWaypoint::from(&read)]
    );
}

// ── US-98: the waypoints, below the tracks ───────────────────────────────────

fn a_waypoint(id: i64, name: Option<&str>, symbol: Option<&str>) -> Waypoint {
    Waypoint {
        id,
        collection_track_id: None,
        name: name.map(str::to_string),
        description: None,
        lat: 46.5,
        lon: 9.2,
        ele: Some(2065.0),
        symbol: symbol.map(str::to_string),
    }
}

#[test]
fn us98_the_waypoints_are_sorted_by_name_ignoring_case_unnamed_last() {
    let sorted = by_name(vec![
        a_waypoint(1, None, None),
        a_waypoint(2, Some("passo"), None),
        a_waypoint(3, Some("Albula"), None),
        a_waypoint(4, Some("Maloja"), None),
    ]);

    let ids: Vec<i64> = sorted.iter().map(|waypoint| waypoint.id).collect();
    assert_eq!(ids, [3, 4, 2, 1]);
}

#[test]
fn us98_each_waypoint_links_to_its_page_with_its_icon_and_elevation() {
    let html = render(|| {
        let waypoints = vec![
            a_waypoint(7, Some("Albula"), Some("Summit")),
            Waypoint {
                ele: None,
                ..a_waypoint(8, None, Some("Shipwreck"))
            },
        ];
        rsx! { WaypointList { waypoints } }
    });

    let rows: Vec<&str> = html.split("<tr").skip(2).collect();
    assert!(rows[0].contains(r#"href="/waypoints/7""#), "{html}");
    assert!(rows[0].contains("waypoint-symbol"), "{html}");
    assert!(rows[0].contains("2065 m"), "{html}");
    assert!(rows[1].contains("Unnamed waypoint"), "{html}");
    assert!(rows[1].contains("waypoint-mark"), "no icon: {html}");
    assert!(rows[1].contains("—"), "no elevation: {html}");
}

#[test]
fn us98_a_collection_without_waypoints_shows_no_list() {
    let html = render(|| rsx! { WaypointList { waypoints: Vec::new() } });

    assert!(html.is_empty(), "{html}");
}

#[tokio::test]
async fn us98_the_page_lists_its_waypoints_below_its_tracks() {
    let (archive, _dir) = serve_test_archive().await;
    let (id, _) = import_collection(&archive, COLLECTION_GPX, &[]).await;
    let waypoints = api::list_collection_waypoints(&archive, id).await.unwrap();

    let html = render_against_archive(
        &archive,
        move || rsx! { Collection { id } },
        |html| html.contains("collection-waypoints"),
    )
    .await;

    let tracks = html.find("collection-tracks").unwrap();
    let listed = html.find("collection-waypoints").unwrap();
    assert!(tracks < listed, "{html}");
    for waypoint in waypoints {
        assert!(
            html.contains(&format!(r#"href="/waypoints/{}""#, waypoint.id)),
            "{html}"
        );
    }
}

mod recorded;
