//! Harness for testing the view layer on the host target — no browser, no
//! wasm, no Android device (ADR-0012's 2026-08-26a amendment).
//!
//! Two levels, both used by the exemplar tests in `list.rs`:
//!
//! * [`render`] renders a view to HTML and hands back the string to assert on.
//! * [`render_against_archive`] does the same with a **real Axum server**
//!   behind it — a temporary SQLite database and blob store — polling until
//!   the screen's `use_resource` fetches have resolved. Nothing internal is
//!   mocked; [`serve_test_archive_with_komoot`] adds the one external
//!   collaborator ADR-0012 does mock, the network.
//!
//! Both wrap the view in a `Router`: `Link` panics without one, so any
//! component that navigates can only render inside a router context.
//!
//! What this cannot reach: `dioxus-ssr` renders, it does not click, so event
//! handlers are not exercised; and `document::eval` does nothing headless.
//! Those belong to the Playwright layer (ADR-0012's 2026-08-26b amendment),
//! and only those — anything assertable on a rendered string belongs here.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;

use crate::api::ApiClient;

/// A view to render, as a closure — so a test can pass a component with
/// whatever props it likes without the harness knowing their type.
///
/// Compared by pointer identity: props must be `PartialEq`, and two closures
/// have no other meaningful notion of equality.
#[derive(Clone)]
pub struct View(Rc<dyn Fn() -> Element>);

impl View {
    pub fn new(view: impl Fn() -> Element + 'static) -> Self {
        Self(Rc::new(view))
    }
}

impl PartialEq for View {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Routable, Clone, PartialEq)]
enum HarnessRoute {
    #[route("/")]
    Subject {},
}

#[component]
fn Subject() -> Element {
    (use_context::<View>().0)()
}

#[component]
fn Harness(view: View, archive: ApiClient) -> Element {
    use_context_provider(|| view.clone());
    // The same context the real `App` provides, so screens fetch from
    // wherever the test put the server — and, since US-19, with the session
    // the test signed in with.
    use_context_provider(|| Signal::new(archive.clone()));
    rsx! { Router::<HarnessRoute> {} }
}

fn harness(view: impl Fn() -> Element + 'static, archive: &ApiClient) -> VirtualDom {
    VirtualDom::new_with_props(
        Harness,
        HarnessProps {
            view: View::new(view),
            archive: archive.clone(),
        },
    )
}

/// Render a view once, synchronously. For components that take their data as
/// props and fetch nothing.
pub fn render(view: impl Fn() -> Element + 'static) -> String {
    let mut dom = harness(view, &ApiClient::default());
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

/// Render a screen against `base_url`, letting its fetches resolve, until
/// `done` accepts the HTML. Panics with the last HTML rendered if that never
/// happens — a screen stuck on "Loading…" is otherwise a silent timeout.
pub async fn render_against_archive(
    archive: &ApiClient,
    view: impl Fn() -> Element + 'static,
    done: impl Fn(&str) -> bool,
) -> String {
    let mut dom = harness(view, archive);
    dom.rebuild_in_place();

    let mut html = dioxus_ssr::render(&dom);
    for _ in 0..50 {
        if done(&html) {
            return html;
        }
        if tokio::time::timeout(Duration::from_millis(100), dom.wait_for_work())
            .await
            .is_ok()
        {
            dom.render_immediate(&mut dioxus_core::NoOpMutations);
        }
        html = dioxus_ssr::render(&dom);
    }
    panic!("the screen never reached the expected state; last render:\n{html}");
}

/// The canonical GPX fixture (`tests/fixtures/sample.gpx`): one recorded
/// track named "Oslo Hills Walk", near Oslo (59.91 N, 10.75 E).
pub const SAMPLE_GPX: &[u8] = include_bytes!("../../../tests/fixtures/sample.gpx");

/// A second track, in the Alps (47.26 N, 11.38 E) — far enough from
/// [`SAMPLE_GPX`] that a region can hold one and not the other (US-14).
pub const ALPS_GPX: &[u8] = include_bytes!("../../../tests/fixtures/region_alps.gpx");

const BOUNDARY: &str = "UiDioxusTestBoundary";

/// Seed a trip through the real import API (`POST /api/import`), the same
/// path the owner's own uploads take — no direct database writes. `fields`
/// are the import form's text fields (`name`, `activity_type`, `kind`, …);
/// returns the new trip's id, parsed from the import's redirect.
pub async fn import_sample(archive: &ApiClient, fields: &[(&str, &str)]) -> i64 {
    import_gpx(archive, SAMPLE_GPX, fields).await
}

/// As [`import_sample`], for a caller that needs a particular track.
pub async fn import_gpx(archive: &ApiClient, gpx: &[u8], fields: &[(&str, &str)]) -> i64 {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"gpx\"; filename=\"track.gpx\"\r\n\
          Content-Type: application/gpx+xml\r\n\r\n",
    );
    body.extend_from_slice(gpx);
    body.extend_from_slice(b"\r\n");
    for (field, value) in fields {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{field}\"\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");
    let response = client
        .post(format!("{}/api/import", archive.base_url()))
        .bearer_auth(token())
        .header(
            "content-type",
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(body)
        .send()
        .await
        .expect("import request");
    assert!(
        response.status().is_redirection(),
        "import failed: {}",
        response.status()
    );
    response.headers()["location"]
        .to_str()
        .unwrap()
        .strip_prefix("/app/trips/")
        .expect("redirect to /app/trips/<id>")
        .parse()
        .expect("numeric trip id")
}

/// Tag a trip through the real API (`POST /api/trips/:id/tags`, US-33) —
/// the seeding path for tag-filter and bulk-tag tests.
pub async fn tag_trip(archive: &ApiClient, trip_id: i64, name: &str) {
    let response = reqwest::Client::new()
        .post(format!("{}/api/trips/{trip_id}/tags", archive.base_url()))
        .bearer_auth(token())
        .json(&serde_json::json!({ "name": name }))
        .send()
        .await
        .expect("tag request");
    assert!(
        response.status().is_success(),
        "tagging failed: {}",
        response.status()
    );
}

/// Start a real Trip Archive server on an ephemeral port, backed by a fresh
/// temporary database and blob store — the same "real collaborators, mock
/// only externals" setup the server's own tests use (ADR-0012).
///
/// Keep the returned `TempDir` alive for the whole test: dropping it deletes
/// the database out from under the running server.
pub async fn serve_test_archive() -> (ApiClient, tempfile::TempDir) {
    let (archive, _state, dir) = serve_archive(None).await;
    (archive, dir)
}

/// As [`serve_test_archive`], with Komoot configured — for the sync screen
/// (US-44), whose every fetch goes through this client.
///
/// The network is the one collaborator ADR-0012 does mock, and
/// `KomootClient` is the seam it named for it. Everything on this side of it
/// is still real: the router, the database, the blob store and the import
/// pipeline a pulled tour travels through.
///
/// The `AppState` comes back too, so a test can say "a sync is already in
/// flight" (`set_sync_in_progress_for_test`) and get US-26's `409` out of
/// the real router rather than constructing one by hand.
pub async fn serve_test_archive_with_komoot(
    client: Arc<dyn trip_archive::server::komoot::KomootClient>,
) -> (
    ApiClient,
    trip_archive::server::state::AppState,
    tempfile::TempDir,
) {
    serve_archive(Some(client)).await
}

/// The password every test server is configured with (US-19). The archive
/// refuses to start without one, so there is no ungated server to render a
/// screen against; the harness sets a known one here and hands every screen
/// a client already holding a session.
pub const TEST_PASSWORD: &str = "a test password";

/// A session token for [`TEST_PASSWORD`] — the credential the host target
/// has to carry by hand, having no cookie store to keep one in (the same
/// position the Android app is in, US-16).
fn token() -> String {
    trip_archive::server::auth::Auth::new(TEST_PASSWORD)
        .expect("a non-empty test password")
        .mint(time::OffsetDateTime::now_utc())
        .token
}

/// A client for an archive nobody has signed in to — for the screens that
/// are *about* being signed out (US-19's login screen).
pub fn anonymous(archive: &ApiClient) -> ApiClient {
    ApiClient::new(archive.base_url())
}

async fn serve_archive(
    komoot: Option<Arc<dyn trip_archive::server::komoot::KomootClient>>,
) -> (
    ApiClient,
    trip_archive::server::state::AppState,
    tempfile::TempDir,
) {
    use trip_archive::server::{
        auth::Auth,
        db, http,
        state::AppState,
        storage::{BlobStore, LocalDisk},
    };

    let dir = tempfile::tempdir().expect("temp dir");
    let pool = db::create_pool(&dir.path().join("test.db"))
        .await
        .expect("create pool");
    let store: Arc<dyn BlobStore> = Arc::new(LocalDisk::new(dir.path().join("blobs")));
    let auth = Auth::new(TEST_PASSWORD).expect("a non-empty test password");
    let state = AppState::new(pool, store, komoot, auth);
    let router = http::router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind an ephemeral port");
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve");
    });

    (ApiClient::new(base_url).with_token(token()), state, dir)
}
