//! Shared helpers for the HTTP-level acceptance tests (US-1, US-21, …).
//!
//! Lives under `tests/common/` so Cargo treats it as a module included via
//! `mod common;` rather than compiling it as its own test binary. Not every
//! test binary uses every helper, so dead-code warnings are silenced here.
#![allow(dead_code)]

use axum::{
    body::Body,
    http::{Method, Request},
    response::Response,
    Router,
};
use tower::ServiceExt; // .oneshot()
use trip_archive::server::{db, http, state::AppState};

pub const SAMPLE_GPX: &[u8] = include_bytes!("../fixtures/sample.gpx");
pub const NO_TRACKS_GPX: &[u8] = include_bytes!("../fixtures/no_tracks.gpx");

/// A router backed by a fresh temp database. Keep the returned `TempDir` alive
/// for the whole test — dropping it deletes the database.
pub async fn test_app() -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let pool = db::create_pool(&dir.path().join("test.db"))
        .await
        .expect("create pool");
    (http::router(AppState { pool }), dir)
}

/// Drive a single request through the router.
pub async fn send(app: &Router, request: Request<Body>) -> Response {
    app.clone().oneshot(request).await.unwrap()
}

/// GET `uri`.
pub async fn get(app: &Router, uri: &str) -> Response {
    send(
        app,
        Request::builder().uri(uri).body(Body::empty()).unwrap(),
    )
    .await
}

/// A `multipart/form-data` POST to `/api/import` carrying a single `gpx` file.
pub fn import_request(gpx: &[u8]) -> Request<Body> {
    let boundary = "TripArchiveTestBoundary";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"gpx\"; filename=\"track.gpx\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: application/gpx+xml\r\n\r\n");
    body.extend_from_slice(gpx);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    Request::builder()
        .method(Method::POST)
        .uri("/api/import")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

/// POST a GPX import and return the raw response (for asserting the redirect).
pub async fn import(app: &Router, gpx: &[u8]) -> Response {
    send(app, import_request(gpx)).await
}

/// Import the sample GPX and return the new trip id (parsed from the redirect).
pub async fn import_sample(app: &Router) -> i64 {
    let redirect = import(app, SAMPLE_GPX).await;
    redirect
        .headers()
        .get("location")
        .expect("Location header")
        .to_str()
        .unwrap()
        .strip_prefix("/trips/")
        .expect("redirect to /trips/<id>")
        .parse()
        .expect("numeric trip id")
}

pub async fn body_bytes(response: Response) -> Vec<u8> {
    axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
}

pub async fn body_string(response: Response) -> String {
    String::from_utf8(body_bytes(response).await).unwrap()
}
