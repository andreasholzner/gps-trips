//! US-88 — several files at once, and the original files kept.

use axum::http::{header, StatusCode};
use serde_json::json;
use trip_archive::models::{
    ActivityType, CollectionDetail, CollectionFile, CreatedCollection, ImportAction, Waypoint,
};

use super::{
    book, collection, confirm, confirm_response, corrected_edition, files, first_edition,
    proposals, read_json, track_names, upload, upload_response, waypoints,
};
use crate::common::{
    body_bytes, collection_files_request, error_message, get, import_collection,
    send_unauthenticated, test_app,
};

use ImportAction::{Add, Skip};

// ── Several files at once ────────────────────────────────────────────────────

fn part(stage: u8, lat: f64) -> Vec<u8> {
    let name = format!("Stage {stage}");
    book(
        &[(Some(name.as_str()), lat)],
        &[("Hut", 46.5, Some("Lodge"))],
    )
}

#[tokio::test]
async fn us88_several_files_make_a_new_collection_in_the_order_of_their_names() {
    let (app, _dir) = test_app().await;

    let staged = upload(
        &app,
        &[
            ("part-2.gpx", &part(2, 46.6)),
            ("Part-1.gpx", &part(1, 46.5)),
        ],
        None,
    )
    .await;

    assert_eq!(staged.files, ["Part-1.gpx", "part-2.gpx"]);
    // The details come from the first file.
    assert_eq!(staged.suggested_name, "Part-1");
    assert_eq!(
        proposals(&staged.tracks),
        [(Some("Stage 1"), Add), (Some("Stage 2"), Add)]
    );
    // The hut is in both files: added once, the second time the same.
    assert_eq!(
        proposals(&staged.waypoints),
        [(Some("Hut"), Add), (Some("Hut"), Skip)]
    );
    assert_eq!(staged.waypoints[1].matched.as_ref().unwrap().file, Some(0));

    let response = confirm_response(
        &app,
        &staged,
        json!({}),
        json!({ "name": "Two parts", "activity_type": "hiking" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let id = serde_json::from_slice::<CreatedCollection>(&body_bytes(response).await)
        .unwrap()
        .id;
    let created = collection(&app, id).await;
    assert_eq!(created.name, "Two parts");
    assert_eq!(created.activity_type, Some(ActivityType::Hiking));
    assert_eq!(track_names(&created), ["Stage 1", "Stage 2"]);
    assert_eq!(created.waypoint_count, 1);
    // The hut came with the first file's only track.
    assert_eq!(
        waypoints(&app, id).await[0].collection_track_id,
        Some(created.tracks[0].id)
    );
    assert_eq!(files(&app, id).await, ["Part-1.gpx", "part-2.gpx"]);
}

#[tokio::test]
async fn us88_several_files_end_where_adding_them_one_by_one_would() {
    let (app, _dir) = test_app().await;
    let files_in_turn = [
        ("a.gpx", first_edition()),
        ("b.gpx", corrected_edition()),
        ("c.gpx", part(5, 46.9)),
    ];

    let at_once = import_collection(&app, &files_in_turn[0].1, &[]).await;
    let staged = upload(
        &app,
        &[
            (files_in_turn[1].0, &files_in_turn[1].1),
            (files_in_turn[2].0, &files_in_turn[2].1),
        ],
        Some(at_once),
    )
    .await;
    confirm(&app, &staged, json!({})).await;

    let one_by_one = import_collection(&app, &files_in_turn[0].1, &[]).await;
    for (name, file) in &files_in_turn[1..] {
        let staged = upload(&app, &[(name, file)], Some(one_by_one)).await;
        confirm(&app, &staged, json!({})).await;
    }

    let names = |c: CollectionDetail| {
        c.tracks
            .into_iter()
            .map(|t| (t.name, t.distance_m))
            .collect::<Vec<_>>()
    };
    let (a, b) = (
        collection(&app, at_once).await,
        collection(&app, one_by_one).await,
    );
    assert_eq!(a.waypoint_count, b.waypoint_count);
    assert_eq!(names(a), names(b));
    let symbols = |wps: Vec<Waypoint>| wps.into_iter().map(|w| w.symbol).collect::<Vec<_>>();
    assert_eq!(
        symbols(waypoints(&app, at_once).await),
        symbols(waypoints(&app, one_by_one).await)
    );
}

#[tokio::test]
async fn us88_only_what_was_held_before_the_upload_is_offered_for_removal() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, &first_edition(), &[]).await;

    let staged = upload(
        &app,
        &[("a.gpx", &part(4, 46.8)), ("b.gpx", &part(1, 46.5))],
        Some(id),
    )
    .await;

    let offered: Vec<_> = staged
        .held_tracks
        .iter()
        .map(|t| t.name.as_deref())
        .collect();
    // Stage 4, from the first file, is in neither the collection nor the
    // second file, and still not offered.
    assert_eq!(offered, [Some("Stage 2"), Some("Stage 3")]);
}

#[tokio::test]
async fn us88_one_refused_file_is_named_and_nothing_changes() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, &first_edition(), &[]).await;
    let before = collection(&app, id).await;

    let response = upload_response(
        &app,
        &[("a.gpx", &part(4, 46.8)), ("broken.gpx", b"<gpx")],
        Some(id),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(error_message(response).await.starts_with("broken.gpx: "));
    assert_eq!(collection(&app, id).await, before);
}

// ── The original files ───────────────────────────────────────────────────────

#[tokio::test]
async fn us88_every_file_is_kept_and_downloads_byte_for_byte() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, &first_edition(), &[]).await;
    // Everything in it skipped: kept all the same.
    let staged = upload(&app, &[("Lakes – again.gpx", &first_edition())], Some(id)).await;
    assert!(staged.tracks.iter().all(|t| t.proposed == Skip));
    confirm(&app, &staged, json!({})).await;

    let kept: Vec<CollectionFile> = read_json(&app, &format!("/api/collections/{id}/files")).await;
    assert_eq!(kept.len(), 2);
    let response = get(&app, &format!("/api/collection-files/{}/gpx", kept[1].id)).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/gpx+xml"
    );
    let disposition = response.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        disposition.contains("filename*=UTF-8''Lakes%20%E2%80%93%20again.gpx"),
        "{disposition}"
    );
    assert_eq!(body_bytes(response).await, first_edition());
}

#[tokio::test]
async fn us88_files_of_no_such_collection_or_file_are_not_found() {
    let (app, _dir) = test_app().await;

    assert_eq!(
        get(&app, "/api/collections/999/files").await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, "/api/collection-files/999/gpx").await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn us88_adding_files_is_the_owners_alone() {
    let (app, _dir) = test_app().await;

    let request = collection_files_request("/api/collection-imports", &[("a.gpx", b"<gpx/>")], &[]);
    let response = send_unauthenticated(&app, request).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ── The default name ─────────────────────────────────────────────────────────

#[tokio::test]
async fn us88_a_collection_with_no_name_to_go_by_is_called_collection() {
    let (app, _dir) = test_app().await;
    // No name in the file, and none for the file.
    let unnamed = book(&[(Some("Stage 1"), 46.5)], &[]);

    let preview = crate::common::collection_request("/api/collections/preview", &unnamed, "", &[]);
    let preview: trip_archive::models::CollectionPreview =
        serde_json::from_slice(&body_bytes(crate::common::send(&app, preview).await).await)
            .unwrap();
    let staged = upload(&app, &[("", &unnamed), ("", &unnamed)], None).await;

    assert_eq!(preview.suggested_name, "Collection");
    assert_eq!(staged.suggested_name, "Collection");
    assert_eq!(staged.files, ["Collection.gpx", "Collection.gpx"]);
}
