//! US-88 acceptance tests — "add a file to a collection".
//!
//! Further or corrected files are matched against what a collection holds;
//! the server proposes, the owner decides, and nothing changes before the
//! confirmation, which applies all of it or none (ADR-0029). Several files
//! at once end where adding them one by one would. Driven through the real
//! router against a real temporary database (ADR-0012).

use axum::http::{Method, StatusCode};
use serde_json::json;
use trip_archive::models::{
    ActivityType, CollectionDetail, CollectionFile, CollectionImportReview, CreatedCollection,
    ImportAction, Waypoint,
};

use crate::common::{
    body_string, collection_files_request, delete, error_message, get, import_collection,
    json_request, send, test_app,
};

use ImportAction::{Add, Replace, Skip};

/// A guide book's file: each track a line eastwards along its latitude,
/// each waypoint at its latitude with its symbol.
fn book(tracks: &[(Option<&str>, f64)], waypoints: &[(&str, f64, Option<&str>)]) -> Vec<u8> {
    let mut gpx = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="test" xmlns="http://www.topografix.com/GPX/1/1">
"#,
    );
    for (name, lat, symbol) in waypoints {
        let symbol = symbol.map_or_else(String::new, |s| format!("<sym>{s}</sym>"));
        gpx.push_str(&format!(
            r#"<wpt lat="{lat}" lon="9.2"><ele>2000</ele><name>{name}</name>{symbol}</wpt>
"#
        ));
    }
    for (name, lat) in tracks {
        gpx.push_str("<trk>");
        if let Some(name) = name {
            gpx.push_str(&format!("<name>{name}</name>"));
        }
        gpx.push_str("<trkseg>");
        for i in 0..10 {
            let lon = 9.2 + f64::from(i) * 0.001;
            gpx.push_str(&format!(
                r#"<trkpt lat="{lat}" lon="{lon}"><ele>1000</ele></trkpt>"#
            ));
        }
        gpx.push_str("</trkseg></trk>\n");
    }
    gpx.push_str("</gpx>\n");
    gpx.into_bytes()
}

/// About 11 m north: the same, within the tolerance.
const NUDGE: f64 = 0.0001;

/// Three stages and two waypoints, imported as US-86 does.
fn first_edition() -> Vec<u8> {
    book(
        &[
            (Some("Stage 1"), 46.5),
            (Some("Stage 2"), 46.6),
            (Some("Stage 3"), 46.7),
        ],
        &[("Hut", 46.5, Some("Lodge")), ("Pass", 46.6, Some("Summit"))],
    )
}

/// Stage 1 merely shifted, Stage 2 changed, Stage 3 gone, Stage 4 new; the
/// hut the same, the pass without its symbol.
fn corrected_edition() -> Vec<u8> {
    book(
        &[
            (Some("Stage 1"), 46.5 + NUDGE),
            (Some("Stage 2"), 46.61),
            (Some("Stage 4"), 46.8),
        ],
        &[("Hut", 46.5 + NUDGE, Some("Lodge")), ("Pass", 46.6, None)],
    )
}

async fn read_json<T: serde::de::DeserializeOwned>(app: &axum::Router, uri: &str) -> T {
    let response = get(app, uri).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{uri}: {e}: {body}"))
}

async fn collection(app: &axum::Router, id: i64) -> CollectionDetail {
    read_json(app, &format!("/api/collections/{id}")).await
}

async fn waypoints(app: &axum::Router, id: i64) -> Vec<Waypoint> {
    read_json(app, &format!("/api/collections/{id}/waypoints")).await
}

async fn files(app: &axum::Router, id: i64) -> Vec<String> {
    read_json::<Vec<CollectionFile>>(app, &format!("/api/collections/{id}/files"))
        .await
        .into_iter()
        .map(|file| file.file_name)
        .collect()
}

fn track_names(collection: &CollectionDetail) -> Vec<&str> {
    collection.tracks.iter().map(|t| t.name.as_str()).collect()
}

async fn upload_response(
    app: &axum::Router,
    files: &[(&str, &[u8])],
    collection_id: Option<i64>,
) -> axum::response::Response {
    let id = collection_id.map(|id| id.to_string());
    let fields: Vec<(&str, &str)> = id.iter().map(|id| ("collection_id", id.as_str())).collect();
    send(
        app,
        collection_files_request("/api/collection-imports", files, &fields),
    )
    .await
}

async fn upload(
    app: &axum::Router,
    files: &[(&str, &[u8])],
    collection_id: Option<i64>,
) -> CollectionImportReview {
    let response = upload_response(app, files, collection_id).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    serde_json::from_str(&body).unwrap()
}

async fn review(
    app: &axum::Router,
    staging_id: i64,
    decisions: serde_json::Value,
) -> CollectionImportReview {
    let uri = format!("/api/collection-imports/{staging_id}/review");
    let response = send(
        app,
        json_request(Method::POST, &uri, &decisions.to_string()),
    )
    .await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    serde_json::from_str(&body).unwrap()
}

async fn confirm_response(
    app: &axum::Router,
    review: &CollectionImportReview,
    decisions: serde_json::Value,
    details: serde_json::Value,
) -> axum::response::Response {
    let mut body = json!({ "decisions": decisions, "held_digest": review.held_digest });
    if let (Some(body), Some(details)) = (body.as_object_mut(), details.as_object()) {
        body.extend(details.clone());
    }
    let uri = format!("/api/collection-imports/{}/confirm", review.staging_id);
    send(app, json_request(Method::POST, &uri, &body.to_string())).await
}

async fn confirm(
    app: &axum::Router,
    review: &CollectionImportReview,
    decisions: serde_json::Value,
) -> i64 {
    let response = confirm_response(app, review, decisions, json!({})).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    serde_json::from_str::<CreatedCollection>(&body).unwrap().id
}

fn proposals(items: &[trip_archive::models::ReviewItem]) -> Vec<(Option<&str>, ImportAction)> {
    items
        .iter()
        .map(|item| (item.name.as_deref(), item.proposed))
        .collect()
}

// ── The review ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn us88_the_review_proposes_an_action_for_every_track_and_waypoint() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, &first_edition(), &[]).await;

    let review = upload(&app, &[("corrected.gpx", &corrected_edition())], Some(id)).await;

    assert_eq!(review.collection_id, Some(id));
    assert_eq!(
        proposals(&review.tracks),
        [
            (Some("Stage 1"), Skip),
            (Some("Stage 2"), Replace),
            (Some("Stage 4"), Add)
        ]
    );
    // A symbol stored or not differs too (US-95).
    assert_eq!(
        proposals(&review.waypoints),
        [(Some("Hut"), Skip), (Some("Pass"), Replace)]
    );
    let kept: Vec<_> = review
        .held_tracks
        .iter()
        .map(|held| (held.name.as_deref(), held.remove))
        .collect();
    assert_eq!(kept, [(Some("Stage 3"), false)]);
    assert!(review.held_waypoints.is_empty());
    let matched = review.tracks[1].matched.as_ref().unwrap();
    assert_eq!(matched.name.as_deref(), Some("Stage 2"));
    assert_eq!(matched.file, None, "held by the collection");
    assert_eq!(review.tracks[1].choices, [Skip, Replace, Add]);
    assert_eq!(review.tracks[2].choices, [Add, Skip]);
}

#[tokio::test]
async fn us88_nothing_changes_before_the_owner_confirms() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, &first_edition(), &[]).await;
    let before = collection(&app, id).await;

    let staged = upload(&app, &[("corrected.gpx", &corrected_edition())], Some(id)).await;
    review(&app, staged.staging_id, json!({ "replace_all": true })).await;

    assert_eq!(collection(&app, id).await, before);
    assert_eq!(files(&app, id).await, ["book.gpx"]);
}

#[tokio::test]
async fn us88_confirming_applies_the_proposals() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, &first_edition(), &[]).await;
    let before = collection(&app, id).await;
    let staged = upload(&app, &[("corrected.gpx", &corrected_edition())], Some(id)).await;

    assert_eq!(confirm(&app, &staged, json!({})).await, id);

    let after = collection(&app, id).await;
    assert_eq!(
        track_names(&after),
        ["Stage 1", "Stage 2", "Stage 3", "Stage 4"]
    );
    // Skipped and replaced tracks are the same records.
    assert_eq!(after.tracks[0].id, before.tracks[0].id);
    assert_eq!(after.tracks[1].id, before.tracks[1].id);
    let pass = &waypoints(&app, id).await[1];
    assert_eq!(pass.name.as_deref(), Some("Pass"));
    assert_eq!(pass.symbol, None, "the file names none");
    assert_eq!(files(&app, id).await, ["book.gpx", "corrected.gpx"]);
    // The upload is spent.
    let uri = format!("/api/collection-imports/{}/review", staged.staging_id);
    let response = send(&app, json_request(Method::POST, &uri, "{}")).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn us88_a_replaced_track_takes_the_files_line_and_keeps_its_activity_override() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, &first_edition(), &[]).await;
    let stage_2 = collection(&app, id).await.tracks[1].id;
    let uri = format!("/api/collection-tracks/{stage_2}");
    let response = send(
        &app,
        json_request(Method::PATCH, &uri, r#"{"activity_type":"cycling"}"#),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let positions = |app: axum::Router| async move {
        let tracks: Vec<trip_archive::models::TripTrack> =
            read_json(&app, &format!("/api/collections/{id}/tracks")).await;
        tracks[1].coordinates[0]
    };
    assert_eq!(positions(app.clone()).await, [9.2, 46.6]);

    let staged = upload(&app, &[("corrected.gpx", &corrected_edition())], Some(id)).await;
    confirm(&app, &staged, json!({})).await;

    let replaced = &collection(&app, id).await.tracks[1];
    assert_eq!(replaced.id, stage_2);
    assert_eq!(replaced.activity_type, ActivityType::Cycling);
    assert_eq!(positions(app.clone()).await, [9.2, 46.61]);
}

#[tokio::test]
async fn us88_an_added_unnamed_track_is_numbered_after_the_collections_tracks() {
    let (app, _dir) = test_app().await;
    let first = book(&[(Some("Stage 1"), 46.5), (None, 46.6)], &[]);
    let id = import_collection(&app, &first, &[("name", "Alps")]).await;
    assert_eq!(
        track_names(&collection(&app, id).await),
        ["Stage 1", "Alps 2"]
    );

    let staged = upload(
        &app,
        &[
            ("a.gpx", &book(&[(None, 46.8)], &[])),
            ("b.gpx", &book(&[(None, 46.9)], &[])),
        ],
        Some(id),
    )
    .await;
    confirm(&app, &staged, json!({})).await;

    assert_eq!(
        track_names(&collection(&app, id).await),
        ["Stage 1", "Alps 2", "Alps 3", "Alps 4"]
    );
}

#[tokio::test]
async fn us88_the_owner_decides_item_by_item() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, &first_edition(), &[]).await;
    let staged = upload(&app, &[("corrected.gpx", &corrected_edition())], Some(id)).await;
    let stage_3 = staged.held_tracks[0].id;
    let decisions = json!({
        "tracks": [
            { "file": 0, "index": 1, "action": "skip" },
            { "file": 0, "index": 2, "action": "skip" }
        ],
        "waypoints": [{ "file": 0, "index": 1, "action": "skip" }],
        "remove_tracks": [stage_3]
    });

    let reviewed = review(&app, staged.staging_id, decisions.clone()).await;
    let actions: Vec<_> = reviewed.tracks.iter().map(|t| t.action).collect();
    assert_eq!(actions, [Skip, Skip, Skip]);
    assert!(reviewed.held_tracks[0].remove);
    confirm(&app, &reviewed, decisions).await;

    let after = collection(&app, id).await;
    assert_eq!(track_names(&after), ["Stage 1", "Stage 2"]);
    assert_eq!(
        waypoints(&app, id).await[1].symbol.as_deref(),
        Some("Summit")
    );
}

#[tokio::test]
async fn us88_replacing_the_whole_collection_keeps_only_its_details() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, &first_edition(), &[("name", "Lakes")]).await;
    let staged = upload(&app, &[("corrected.gpx", &corrected_edition())], Some(id)).await;

    let reviewed = review(&app, staged.staging_id, json!({ "replace_all": true })).await;
    assert!(reviewed.replace_all);
    assert!(reviewed.held_tracks.is_empty());
    assert!(reviewed.tracks.iter().all(|t| t.proposed == Add));
    confirm(&app, &reviewed, json!({ "replace_all": true })).await;

    let after = collection(&app, id).await;
    assert_eq!(after.name, "Lakes");
    assert_eq!(track_names(&after), ["Stage 1", "Stage 2", "Stage 4"]);
    assert_eq!(after.waypoint_count, 2);
    assert_eq!(files(&app, id).await, ["corrected.gpx"]);
}

#[tokio::test]
async fn us93_a_replaced_track_keeps_its_tags_and_a_removed_one_takes_them() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, &first_edition(), &[]).await;
    let tracks = collection(&app, id).await.tracks;
    for track in &tracks {
        let uri = format!("/api/collection-tracks/{}/tags", track.id);
        let response = send(&app, json_request(Method::POST, &uri, r#"{"name":"alps"}"#)).await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    let tag_count = |app: axum::Router, track: i64| async move {
        let tags: Vec<trip_archive::models::Tag> =
            read_json(&app, &format!("/api/collection-tracks/{track}/tags")).await;
        tags.len()
    };

    let staged = upload(&app, &[("corrected.gpx", &corrected_edition())], Some(id)).await;
    confirm(&app, &staged, json!({ "remove_tracks": [tracks[2].id] })).await;

    // Stage 2 was replaced: the same record, with its tag.
    assert_eq!(tag_count(app.clone(), tracks[1].id).await, 1);
    let overview: Vec<trip_archive::models::TagOverview> =
        read_json(&app, "/api/tags/overview").await;
    assert_eq!(overview[0].collection_track_count, 2);
}

#[tokio::test]
async fn us88_cancelling_leaves_the_collection_untouched() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, &first_edition(), &[]).await;
    let before = collection(&app, id).await;
    let staged = upload(&app, &[("corrected.gpx", &corrected_edition())], Some(id)).await;

    let uri = format!("/api/collection-imports/{}", staged.staging_id);
    assert_eq!(delete(&app, &uri).await.status(), StatusCode::NO_CONTENT);

    assert_eq!(collection(&app, id).await, before);
    let response = confirm_response(&app, &staged, json!({}), json!({})).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn us88_a_collection_changed_since_the_review_is_refused_and_the_upload_stays() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, &first_edition(), &[]).await;
    let staged = upload(&app, &[("corrected.gpx", &corrected_edition())], Some(id)).await;
    let stage_3 = collection(&app, id).await.tracks[2].id;
    let deleted = delete(&app, &format!("/api/collection-tracks/{stage_3}")).await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let before = collection(&app, id).await;

    let response = confirm_response(&app, &staged, json!({}), json!({})).await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert!(error_message(response).await.contains("review them again"));
    assert_eq!(collection(&app, id).await, before);
    let again = review(&app, staged.staging_id, json!({})).await;
    assert!(again.held_tracks.is_empty());
    confirm(&app, &again, json!({})).await;
}

#[tokio::test]
async fn us88_a_file_with_nothing_to_collect_is_refused_as_in_us86() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, &first_edition(), &[]).await;

    let not_gpx = upload_response(&app, &[("notes.gpx", b"not xml")], Some(id)).await;
    assert_eq!(not_gpx.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let empty = upload_response(&app, &[("empty.gpx", &book(&[], &[]))], Some(id)).await;
    assert_eq!(empty.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(error_message(empty).await.contains("empty.gpx"));
}

#[tokio::test]
async fn us88_an_upload_is_refused_without_a_file_or_for_no_such_collection() {
    let (app, _dir) = test_app().await;

    let none = upload_response(&app, &[], None).await;
    assert_eq!(none.status(), StatusCode::BAD_REQUEST);
    let missing = upload_response(&app, &[("a.gpx", &first_edition())], Some(999)).await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

mod several_files;
