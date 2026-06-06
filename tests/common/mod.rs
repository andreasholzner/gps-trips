//! Shared helpers for the HTTP-level acceptance tests (US-1, US-21, …).
//!
//! Lives under `tests/common/` so Cargo treats it as a module included via
//! `mod common;` rather than compiling it as its own test binary. Not every
//! test binary uses every helper, so dead-code warnings are silenced here.
#![allow(dead_code)]

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Method, Request},
    response::Response,
    Router,
};
use tower::ServiceExt; // .oneshot()
use trip_archive::server::{
    db, http,
    state::AppState,
    storage::{BlobStore, LocalDisk},
};

pub const SAMPLE_GPX: &[u8] = include_bytes!("../fixtures/sample.gpx");
pub const NO_TRACKS_GPX: &[u8] = include_bytes!("../fixtures/no_tracks.gpx");

/// A router backed by a fresh temp database and a `LocalDisk` blob store, both
/// under one `TempDir`. Keep the returned `TempDir` alive for the whole test —
/// dropping it deletes the database and the stored photos.
pub async fn test_app() -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let pool = db::create_pool(&dir.path().join("test.db"))
        .await
        .expect("create pool");
    let store: Arc<dyn BlobStore> = Arc::new(LocalDisk::new(dir.path().join("blobs")));
    (http::router(AppState { pool, store }), dir)
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

const BOUNDARY: &str = "TripArchiveTestBoundary";

/// Append one `multipart/form-data` file part to `body`.
fn append_file_part(
    body: &mut Vec<u8>,
    field: &str,
    filename: &str,
    content_type: &str,
    data: &[u8],
) {
    body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{field}\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(b"\r\n");
}

/// Append `(filename, bytes)` photo parts under the `photos` field.
fn append_photo_parts(body: &mut Vec<u8>, photos: &[(&str, &[u8])]) {
    for (filename, data) in photos {
        append_file_part(body, "photos", filename, "image/jpeg", data);
    }
}

fn multipart_request(uri: &str, body: Vec<u8>) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(
            "content-type",
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(body))
        .unwrap()
}

/// A `multipart/form-data` POST to `/api/import` carrying a single `gpx` file.
pub fn import_request(gpx: &[u8]) -> Request<Body> {
    import_request_with_photos(gpx, &[])
}

/// An import POST carrying the `gpx` file plus `(filename, bytes)` photo parts
/// (US-2: photos uploaded with the import).
pub fn import_request_with_photos(gpx: &[u8], photos: &[(&str, &[u8])]) -> Request<Body> {
    let mut body = Vec::new();
    append_file_part(&mut body, "gpx", "track.gpx", "application/gpx+xml", gpx);
    append_photo_parts(&mut body, photos);
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    multipart_request("/api/import", body)
}

/// A `POST /api/trips/:id/photos` carrying `(filename, bytes)` photo parts
/// (US-2: photos added at a later time).
pub fn add_photos_request(trip_id: i64, photos: &[(&str, &[u8])]) -> Request<Body> {
    let mut body = Vec::new();
    append_photo_parts(&mut body, photos);
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    multipart_request(&format!("/api/trips/{trip_id}/photos"), body)
}

/// POST a GPX import and return the raw response (for asserting the redirect).
pub async fn import(app: &Router, gpx: &[u8]) -> Response {
    send(app, import_request(gpx)).await
}

/// Parse the `/trips/<id>` redirect target into a trip id.
fn trip_id_from_redirect(response: &Response) -> i64 {
    response
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

/// Import the sample GPX and return the new trip id (parsed from the redirect).
pub async fn import_sample(app: &Router) -> i64 {
    let redirect = import(app, SAMPLE_GPX).await;
    trip_id_from_redirect(&redirect)
}

/// Import the sample GPX with photos and return the new trip id.
pub async fn import_sample_with_photos(app: &Router, photos: &[(&str, &[u8])]) -> i64 {
    let redirect = send(app, import_request_with_photos(SAMPLE_GPX, photos)).await;
    trip_id_from_redirect(&redirect)
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
