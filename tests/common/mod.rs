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
    (
        http::router(AppState {
            pool,
            store,
            komoot: None,
        }),
        dir,
    )
}

/// As [`test_app`], but with `state.komoot` set — for the Komoot sync
/// routes (US-20/22/24/25), which 400 without it.
pub async fn test_app_with_komoot(
    client: Arc<dyn trip_archive::server::komoot::KomootClient>,
) -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let pool = db::create_pool(&dir.path().join("test.db"))
        .await
        .expect("create pool");
    let store: Arc<dyn BlobStore> = Arc::new(LocalDisk::new(dir.path().join("blobs")));
    (
        http::router(AppState {
            pool,
            store,
            komoot: Some(client),
        }),
        dir,
    )
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

/// DELETE `uri` (US-9).
pub async fn delete(app: &Router, uri: &str) -> Response {
    send(
        app,
        Request::builder()
            .method(Method::DELETE)
            .uri(uri)
            .body(Body::empty())
            .unwrap(),
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

/// Append one `multipart/form-data` plain text field to `body` (e.g. `name`,
/// `activity_type`, `timezone`).
fn append_text_part(body: &mut Vec<u8>, field: &str, value: &str) {
    body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{field}\"\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(value.as_bytes());
    body.extend_from_slice(b"\r\n");
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

/// A JSON request with an arbitrary method (e.g. `PATCH /api/trips/:id`, US-15).
pub fn json_request(method: Method, uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// A `multipart/form-data` POST to `/api/import` carrying a single `gpx` file.
pub fn import_request(gpx: &[u8]) -> Request<Body> {
    import_request_with_photos(gpx, &[])
}

/// An import POST carrying the `gpx` file plus `(filename, bytes)` photo parts
/// (US-2: photos uploaded with the import).
pub fn import_request_with_photos(gpx: &[u8], photos: &[(&str, &[u8])]) -> Request<Body> {
    import_request_with_fields(gpx, &[], photos)
}

/// An import POST carrying the `gpx` file plus arbitrary text fields (e.g.
/// `name`, `activity_type`, `timezone`) and `(filename, bytes)` photo parts.
pub fn import_request_with_fields(
    gpx: &[u8],
    fields: &[(&str, &str)],
    photos: &[(&str, &[u8])],
) -> Request<Body> {
    let mut body = Vec::new();
    append_file_part(&mut body, "gpx", "track.gpx", "application/gpx+xml", gpx);
    for (field, value) in fields {
        append_text_part(&mut body, field, value);
    }
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
pub fn trip_id_from_redirect(response: &Response) -> i64 {
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

/// The exact fragment `render_detail` (`src/server/render.rs`) emits for the
/// trip name — scoped to the element US-15 introduced so a match can't be
/// satisfied by some unrelated part of the page.
pub fn detail_name_fragment(name: &str) -> String {
    format!("<h1 id=\"trip-name\">{name}</h1>")
}

/// The exact fragment `render_detail` emits for the activity type — scoped so
/// it can't be satisfied by `tz_name`'s own independent "unknown" fallback on
/// the same page.
pub fn detail_activity_fragment(activity: &str) -> String {
    format!("<span id=\"trip-activity\">{activity}</span>")
}

/// The exact fragment `render_trip_row` emits for the activity column — the
/// column right after the trip's name link, so this can't match some other
/// coincidental appearance of the word elsewhere on the list page.
pub fn list_activity_fragment(activity: &str) -> String {
    format!("</td><td>{activity}</td>")
}
