use super::*;
use crate::test_support::{
    import_collection, render, render_against_archive, serve_test_archive, COLLECTION_GPX,
};

fn a_track(own_activity: Option<ActivityType>) -> CollectionTrackDetail {
    CollectionTrackDetail {
        id: 4,
        collection_id: 2,
        collection_name: "Lakes & Passes".to_string(),
        name: "Stage 1".to_string(),
        activity_type: own_activity.unwrap_or(ActivityType::Cycling),
        own_activity,
        collection_activity: Some(ActivityType::Cycling),
        distance_m: 12_345.0,
        ascent_m: 615.0,
        descent_m: 20.0,
    }
}

// US-86: the screen against a real archive, nothing mocked (ADR-0012).
#[tokio::test]
async fn us86_the_screen_shows_the_track_its_collection_and_its_figures() {
    let (archive, _dir) = serve_test_archive().await;
    let (collection, tracks) = import_collection(
        &archive,
        COLLECTION_GPX,
        &[("name", "Lakes"), ("activity_type", "cycling")],
    )
    .await;
    let id = tracks[0];

    let html = render_against_archive(
        &archive,
        move || rsx! { CollectionTrack { id } },
        |html| html.contains("Stage 1: Splügen") && html.contains("track-map"),
    )
    .await;

    assert!(html.contains("From the collection "), "{html}");
    assert!(
        html.contains(&format!(r#"href="/collections/{collection}?">Lakes</a>"#)),
        "{html}"
    );
    assert!(html.contains("Cycling"), "{html}");
    assert!(
        html.contains(r#"<div id="track-map" class="track-map"></div>"#),
        "{html}"
    );
    assert!(
        html.contains(&format!(
            "{}/api/collection-tracks/{id}/gpx",
            archive.base_url()
        )),
        "{html}"
    );
    assert!(html.contains(r#"id="edit-trip""#), "{html}");
    assert!(html.contains(r#"id="delete-trip""#), "{html}");
    assert!(html.contains(r#"id="share-collection-track""#), "{html}");
}

// US-93: tagged as a trip is, its tags leading nowhere.
#[tokio::test]
async fn us93_the_screen_shows_the_tracks_own_tags() {
    let (archive, _dir) = serve_test_archive().await;
    let (collection, tracks) = import_collection(&archive, COLLECTION_GPX, &[]).await;
    let id = tracks[0];
    api::add_tag(&archive, api::Tagged::Collection(collection), "lakes")
        .await
        .unwrap();
    api::add_tag(&archive, api::Tagged::CollectionTrack(id), "pass")
        .await
        .unwrap();

    let html = render_against_archive(
        &archive,
        move || rsx! { CollectionTrack { id } },
        |html| html.contains("chip"),
    )
    .await;

    assert!(html.contains(r#"<span class="chip">pass "#), "{html}");
    assert!(
        !html.contains("lakes"),
        "the collection's tag is its own: {html}"
    );
    assert!(html.contains(r#"id="add-tag""#), "{html}");
}

#[test]
fn us86_only_distance_ascent_and_descent_are_shown() {
    // A collection's timestamps are not when anyone went.
    let track = a_track(None);

    let html = render(move || rsx! { TrackFigures { track: track.clone() } });

    let labels: Vec<&str> = html
        .split("<dt>")
        .skip(1)
        .filter_map(|rest| rest.split("</dt>").next())
        .collect();
    assert_eq!(labels, ["Activity", "Distance", "Ascent", "Descent"]);
    assert!(html.contains("12.35 km"), "{html}");
    assert!(html.contains("615 m"), "{html}");
    assert!(!html.contains("trip-date"), "{html}");
}

#[test]
fn us86_an_activity_taken_from_the_collection_is_shown_as_any_other() {
    let taken = a_track(None);

    let taken = render(move || rsx! { TrackFigures { track: taken.clone() } });

    assert!(
        taken.contains(r#"id="trip-activity">Cycling</dd>"#),
        "{taken}"
    );
}

#[test]
fn us86_the_collections_activity_is_offered_first_with_its_icon() {
    let first = activity_choices(&a_track(Some(ActivityType::Hiking)))[0];

    assert_eq!(first.value, "");
    assert_eq!(first.label, "Collection");
    assert_eq!(first.icon, Some(ActivityType::Cycling));
}

#[test]
fn us86_a_collection_without_an_activity_is_offered_without_an_icon() {
    let track = CollectionTrackDetail {
        collection_activity: None,
        ..a_track(None)
    };

    assert_eq!(activity_choices(&track)[0].icon, None);
}

#[test]
fn us86_the_form_starts_on_the_tracks_own_activity_or_the_collections() {
    assert_eq!(TrackForm::of(&a_track(None)).activity, "");
    assert_eq!(
        TrackForm::of(&a_track(Some(ActivityType::Hiking))).activity,
        "hiking"
    );
}

#[test]
fn us86_only_what_changed_is_sent() {
    let track = a_track(None);
    let mut form = TrackForm::of(&track);
    assert_eq!(changes(&track, &form), CollectionTrackEdit::default());

    form.name = "  Day one ".to_string();
    form.activity = "hiking".to_string();
    assert_eq!(
        changes(&track, &form),
        CollectionTrackEdit {
            name: Some("Day one".to_string()),
            activity_type: Some("hiking".to_string()),
        }
    );
}

#[test]
fn us86_an_own_activity_can_be_given_back_to_the_collection() {
    let track = a_track(Some(ActivityType::Hiking));
    let form = TrackForm {
        activity: String::new(),
        ..TrackForm::of(&track)
    };

    assert_eq!(changes(&track, &form).activity_type.as_deref(), Some(""));
}

#[tokio::test]
async fn us86_a_track_that_no_longer_exists_says_so() {
    let (archive, _dir) = serve_test_archive().await;

    let html = render_against_archive(
        &archive,
        || rsx! { CollectionTrack { id: 9_999 } },
        |html| !html.contains("Loading"),
    )
    .await;

    assert!(html.contains("no such track"), "{html}");
}

#[test]
fn us86_deleting_a_track_is_offered_but_not_armed() {
    let html = render(|| rsx! { DeleteCollectionTrack { id: 1, collection_id: 2 } });

    assert!(html.contains("Delete track"), "{html}");
    assert!(!html.contains("cannot be undone"), "{html}");
}
