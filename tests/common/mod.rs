//! Shared helpers for the HTTP-level acceptance tests (US-1, US-21, …).
//!
//! Lives under `tests/common/` so Cargo treats it as a module included via
//! `mod common;` rather than compiling it as its own test binary. Not every
//! test binary uses every helper, so dead-code warnings are silenced here.
#![allow(dead_code)]

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    response::Response,
    Router,
};
use tower::ServiceExt; // .oneshot()
use trip_archive::models::StagedImport;
use trip_archive::server::{
    auth::Auth,
    db, http,
    state::AppState,
    storage::{BlobStore, LocalDisk},
};

/// The shared password every test server is configured with (US-19). The
/// archive refuses to start without one, so there is no such thing as an
/// ungated router to test against — the harness sets a known one in this
/// single place and [`send`] signs every request with it, which is why the
/// story's arrival left the other test files unchanged.
pub const TEST_PASSWORD: &str = "a test password";

/// The gate the test servers are built with.
pub fn test_auth() -> Auth {
    Auth::new(TEST_PASSWORD).expect("a non-empty test password")
}

/// A valid session token for [`TEST_PASSWORD`]. Any `Auth` built from the
/// same password signs interchangeably — the key is derived from the secret,
/// not generated per instance — so this needs no handle on the router's own.
pub fn test_token() -> String {
    test_auth().mint(time::OffsetDateTime::now_utc()).token
}

pub const SAMPLE_GPX: &[u8] = include_bytes!("../fixtures/sample.gpx");
pub const NO_TRACKS_GPX: &[u8] = include_bytes!("../fixtures/no_tracks.gpx");
/// A track with timestamps but no `<name>` — US-12's bare-date-prefix case.
pub const UNNAMED_GPX: &[u8] = include_bytes!("../fixtures/unnamed.gpx");
/// A track starting 22:30 UTC near Oslo — half past midnight the *next* day
/// where it was ridden, so the suggested date has to come from the track's
/// own timezone rather than from UTC.
pub const LATE_EVENING_GPX: &[u8] = include_bytes!("../fixtures/late_evening.gpx");
/// A track in the Alps — far from `SAMPLE_GPX`'s Oslo coordinates, so the
/// US-14 region filter can be tested on a list holding trips in two places.
pub const REGION_ALPS_GPX: &[u8] = include_bytes!("../fixtures/region_alps.gpx");

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
        http::router(AppState::new(pool, store, None, test_auth())),
        dir,
    )
}

/// As [`test_app`], with a different shared password — for US-19's
/// assertion that rotating the secret ends every session that exists.
pub async fn test_app_with_password(password: &str) -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let pool = db::create_pool(&dir.path().join("test.db"))
        .await
        .expect("create pool");
    let store: Arc<dyn BlobStore> = Arc::new(LocalDisk::new(dir.path().join("blobs")));
    let auth = Auth::new(password).expect("a non-empty password");
    (http::router(AppState::new(pool, store, None, auth)), dir)
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
        http::router(AppState::new(pool, store, Some(client), test_auth())),
        dir,
    )
}

/// As [`test_app`]/[`test_app_with_komoot`], but also returns the
/// `AppState` so a test can call `state.set_sync_in_progress_for_test`
/// (US-26) to simulate an in-flight "Sync now" run without a real
/// concurrent request. `komoot` is `None` unless the test also needs the
/// sync routes (which 400 without it).
pub async fn test_app_with_state(
    komoot: Option<Arc<dyn trip_archive::server::komoot::KomootClient>>,
) -> (Router, AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let pool = db::create_pool(&dir.path().join("test.db"))
        .await
        .expect("create pool");
    let store: Arc<dyn BlobStore> = Arc::new(LocalDisk::new(dir.path().join("blobs")));
    let state = AppState::new(pool, store, komoot, test_auth());
    (http::router(state.clone()), state, dir)
}

/// Drive a single request through the router, signed in as the owner
/// (US-19).
///
/// The token goes on as `Authorization: Bearer` rather than as a cookie
/// because it is the same token either way and a header is one line to
/// build. Every helper below funnels through here, which is what keeps the
/// gate invisible to tests that are about something else — the ones that are
/// *about* the gate use [`send_unauthenticated`].
pub async fn send(app: &Router, request: Request<Body>) -> Response {
    let mut request = request;
    request.headers_mut().insert(
        axum::http::header::AUTHORIZATION,
        format!("Bearer {}", test_token()).parse().unwrap(),
    );
    send_unauthenticated(app, request).await
}

/// Drive a single request through the router exactly as given — no session,
/// unless the request carries one it built itself (US-19).
pub async fn send_unauthenticated(app: &Router, request: Request<Body>) -> Response {
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

/// A `multipart/form-data` POST to `/api/import/staged` carrying the `gpx`
/// file alone (US-12, phase one of the two-phase import).
pub fn stage_import_request(gpx: &[u8]) -> Request<Body> {
    let mut body = Vec::new();
    append_file_part(&mut body, "gpx", "track.gpx", "application/gpx+xml", gpx);
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    multipart_request("/api/import/staged", body)
}

/// Stage `gpx` and return the parsed suggestions — the happy path every
/// confirm test starts from.
pub async fn stage_import(app: &Router, gpx: &[u8]) -> StagedImport {
    let response = send(app, stage_import_request(gpx)).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "staging failed: {body}");
    serde_json::from_str(&body).expect("staged import JSON")
}

/// `POST /api/import/staged/:id/confirm` with a JSON body (US-12, phase two).
pub fn confirm_import_request(staging_id: i64, body: &str) -> Request<Body> {
    json_request(
        Method::POST,
        &format!("/api/import/staged/{staging_id}/confirm"),
        body,
    )
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

/// Parse the `/app/trips/<id>` redirect target into a trip id.
pub fn trip_id_from_redirect(response: &Response) -> i64 {
    response
        .headers()
        .get("location")
        .expect("Location header")
        .to_str()
        .unwrap()
        .strip_prefix("/app/trips/")
        .expect("redirect to /app/trips/<id>")
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

/// The archive's own sentence out of a refusal's JSON body (ADR-0008: every
/// error answers `{"error": "…"}`). Panics if the body is not that, which is
/// itself the assertion — a route answering an error in some other shape is
/// a route no client can read.
pub async fn error_message(response: Response) -> String {
    let body = body_string(response).await;
    let error: trip_archive::models::ErrorResponse = serde_json::from_str(&body)
        .unwrap_or_else(|e| panic!("an error must answer with JSON: {e}; got {body}"));
    error.error
}

pub async fn body_string(response: Response) -> String {
    String::from_utf8(body_bytes(response).await).unwrap()
}
