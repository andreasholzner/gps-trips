//! US-113 — the trip list shows each row's tags. They come with the rows
//! from the JSON API (ADR-0008): a trip's, a collection's and a collection
//! track's own, alphabetically; the column itself is the UI crate's.

use axum::{http::Method, http::StatusCode, Router};
use trip_archive::models::{CollectionDetail, CollectionSummary, TripSummary};

use crate::common::{
    body_string, get, import_collection, import_sample, json_request, send, test_app,
    COLLECTION_GPX,
};

async fn read_json<T: serde::de::DeserializeOwned>(app: &Router, uri: &str) -> T {
    let response = get(app, uri).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{uri}: {e}: {body}"))
}

async fn tag(app: &Router, uri: &str, name: &str) {
    let body = serde_json::json!({ "name": name }).to_string();
    let response = send(app, json_request(Method::POST, uri, &body)).await;
    assert_eq!(response.status(), StatusCode::CREATED, "{uri}");
}

#[tokio::test]
async fn us113_a_listed_trip_comes_with_its_tags_alphabetically() {
    let (app, _dir) = test_app().await;
    let tagged = import_sample(&app).await;
    let untagged = import_sample(&app).await;
    for name in ["summer", "alpine", "norway"] {
        tag(&app, &format!("/api/trips/{tagged}/tags"), name).await;
    }

    let trips: Vec<TripSummary> = read_json(&app, "/api/trips").await;

    let tags_of = |id: i64| &trips.iter().find(|trip| trip.id == id).unwrap().tags;
    assert_eq!(tags_of(tagged), &["alpine", "norway", "summer"]);
    assert!(tags_of(untagged).is_empty());
}

#[tokio::test]
async fn us113_a_listed_collection_and_its_tracks_come_with_their_own_tags() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, COLLECTION_GPX, &[("name", "Alps")]).await;
    let detail: CollectionDetail = read_json(&app, &format!("/api/collections/{id}")).await;
    let track = detail.tracks[0].id;
    tag(&app, &format!("/api/collections/{id}/tags"), "guidebook").await;
    tag(&app, &format!("/api/collections/{id}/tags"), "alps").await;
    tag(&app, &format!("/api/collection-tracks/{track}/tags"), "ridge").await;

    let collections: Vec<CollectionSummary> = read_json(&app, "/api/collections").await;

    let collection = collections.iter().find(|c| c.id == id).unwrap();
    assert_eq!(collection.tags, ["alps", "guidebook"]);
    for listed in &collection.tracks {
        // A collection's tags do not pass to its tracks (US-93).
        let expected: &[&str] = if listed.id == track { &["ridge"] } else { &[] };
        assert_eq!(listed.tags, expected, "track {}", listed.id);
    }
}
