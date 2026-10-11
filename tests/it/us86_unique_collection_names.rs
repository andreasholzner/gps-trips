//! US-86 acceptance tests — no two collections share a name, whatever the
//! case of A–Z, so the QMapShack export can give each a folder of its own
//! (US-89).
//! An import or a rename to a taken name is refused, naming it, and changes
//! nothing. US-88's new collection of several files is tested with its
//! other confirmations.

use axum::http::{Method, StatusCode};
use trip_archive::models::{CollectionCount, CollectionDetail};

use crate::common::{
    body_string, collection_request, error_message, get, import_collection, json_request, send,
    test_app, GUIDE_BOOK_GPX, GUIDE_SINGLE_GPX,
};

async fn count(app: &axum::Router) -> i64 {
    let response = get(app, "/api/collections/count").await;
    serde_json::from_str::<CollectionCount>(&body_string(response).await)
        .unwrap()
        .count
}

#[tokio::test]
async fn us86_an_import_under_a_taken_name_is_refused_whatever_the_case() {
    let (app, _dir) = test_app().await;
    import_collection(&app, GUIDE_SINGLE_GPX, &[("name", "Nordkette")]).await;

    let request = collection_request(
        "/api/collections",
        GUIDE_BOOK_GPX,
        "book.gpx",
        &[("name", "NORDKETTE")],
    );
    let response = send(&app, request).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(error_message(response).await.contains("NORDKETTE"));
    assert_eq!(count(&app).await, 1, "nothing created");
}

#[tokio::test]
async fn us86_an_import_under_a_suggested_name_that_is_taken_is_refused_too() {
    let (app, _dir) = test_app().await;
    import_collection(&app, GUIDE_BOOK_GPX, &[]).await;

    let request = collection_request("/api/collections", GUIDE_BOOK_GPX, "book.gpx", &[]);
    let response = send(&app, request).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(error_message(response).await.contains("Lakes & Passes"));
    assert_eq!(count(&app).await, 1);
}

#[tokio::test]
async fn us86_a_rename_to_a_taken_name_is_refused_and_changes_nothing() {
    let (app, _dir) = test_app().await;
    import_collection(&app, GUIDE_BOOK_GPX, &[]).await;
    let other = import_collection(&app, GUIDE_SINGLE_GPX, &[("name", "Nordkette")]).await;
    let uri = format!("/api/collections/{other}");

    let body = r#"{"name":"lakes & passes","description":"changed too"}"#;
    let response = send(&app, json_request(Method::PATCH, &uri, body)).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(error_message(response).await.contains("lakes & passes"));
    let kept: CollectionDetail =
        serde_json::from_str(&body_string(get(&app, &uri).await).await).unwrap();
    assert_eq!(kept.name, "Nordkette");
    assert_ne!(kept.description, "changed too");

    // Its own name in another case takes no other collection's.
    let response = send(
        &app,
        json_request(Method::PATCH, &uri, r#"{"name":"NORDKETTE"}"#),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}
