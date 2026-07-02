//! US-9 — delete a trip (and its files) to fix mistakes.
//!
//! Acceptance criteria: deleting a trip removes its DB rows (cascade) and its
//! photo blobs; no orphaned files remain.

mod common;

use axum::http::StatusCode;
use common::{body_string, delete, get, import_sample, import_sample_with_photos, test_app};

#[tokio::test]
async fn us9_delete_returns_204_and_the_trip_row_is_gone() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = delete(&app, &format!("/api/trips/{id}")).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    assert_eq!(
        get(&app, &format!("/trips/{id}")).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, &format!("/api/trips/{id}/track.geojson"))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, &format!("/api/trips/{id}/photos")).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn us9_delete_removes_photo_blobs_from_disk() {
    let (app, _dir) = test_app().await;
    let id =
        import_sample_with_photos(&app, &[("photo.jpg", b"\xFF\xD8\xFF-fake-jpeg".as_slice())])
            .await;

    let photos_response = get(&app, &format!("/api/trips/{id}/photos")).await;
    let json: serde_json::Value =
        serde_json::from_str(&body_string(photos_response).await).unwrap();
    let url = json[0]["url"]
        .as_str()
        .expect("photo must have a url")
        .to_string();

    let response = delete(&app, &format!("/api/trips/{id}")).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    assert_eq!(
        get(&app, &url).await.status(),
        StatusCode::NOT_FOUND,
        "the photo blob must be gone from disk, not just the DB row"
    );
}

#[tokio::test]
async fn us9_delete_unknown_trip_returns_404() {
    let (app, _dir) = test_app().await;
    let response = delete(&app, "/api/trips/999").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn us9_deleting_a_trip_twice_the_second_call_returns_404() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    assert_eq!(
        delete(&app, &format!("/api/trips/{id}")).await.status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        delete(&app, &format!("/api/trips/{id}")).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn us9_deleting_one_trip_does_not_affect_another_trips_photos() {
    let (app, _dir) = test_app().await;
    let keep = import_sample_with_photos(&app, &[("a.jpg", b"\xFF\xD8\xFF-aaa".as_slice())]).await;
    let gone = import_sample_with_photos(&app, &[("b.jpg", b"\xFF\xD8\xFF-bbb".as_slice())]).await;

    assert_eq!(
        delete(&app, &format!("/api/trips/{gone}")).await.status(),
        StatusCode::NO_CONTENT
    );

    let response = get(&app, &format!("/api/trips/{keep}/photos")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    let url = json[0]["url"].as_str().expect("photo must have a url");
    assert_eq!(get(&app, url).await.status(), StatusCode::OK);
}

#[tokio::test]
async fn us9_detail_page_offers_a_delete_control() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = get(&app, &format!("/trips/{id}")).await;
    let body = body_string(response).await;
    assert!(
        body.contains(r#"id="delete-trip""#),
        "detail page must offer a delete control"
    );
}
