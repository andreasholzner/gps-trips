use super::*;
use crate::test_support::{
    import_collection, render, render_against_archive, serve_test_archive, GUIDE_BOOK_GPX,
};

fn a_track(id: i64, name: &str, activity: ActivityType) -> CollectionTrackSummary {
    CollectionTrackSummary {
        id,
        name: name.to_string(),
        activity_type: activity,
        distance_m: 1_000.0,
        ascent_m: 100.0,
        descent_m: 50.0,
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
    }
}

// US-86: where an import ends — against a real archive (ADR-0012).
#[tokio::test]
async fn us86_the_page_shows_the_details_the_map_and_the_tracks() {
    let (archive, _dir) = serve_test_archive().await;
    let (id, tracks) = import_collection(
        &archive,
        GUIDE_BOOK_GPX,
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

    assert!(html.contains("no such guide book"), "{html}");
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
fn us86_a_book_of_waypoints_alone_says_so_instead_of_an_empty_list() {
    let html = render(|| rsx! { TrackList { tracks: Vec::new() } });

    assert!(html.contains("waypoints only"), "{html}");
    assert!(!html.contains("<table"), "{html}");
}
