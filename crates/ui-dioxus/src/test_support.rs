//! Harness for testing the view layer on the host target — no browser, no
//! wasm, no Android device (ADR-0012's amendment).
//!
//! Two levels, both used by the exemplar tests in `list.rs`:
//!
//! * [`render`] renders a view to HTML and hands back the string to assert on.
//! * [`render_against_archive`] does the same with a **real Axum server**
//!   behind it — a temporary SQLite database and blob store, no mocks —
//!   polling until the screen's `use_resource` fetches have resolved.
//!
//! Both wrap the view in a `Router`: `Link` panics without one, so any
//! component that navigates can only render inside a router context.
//!
//! What this cannot reach: `dioxus-ssr` renders, it does not click, so event
//! handlers are not exercised; and `document::eval` does nothing headless, so
//! the map and chart draw into nothing. Those need a browser.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;

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
fn Harness(view: View, base_url: String) -> Element {
    use_context_provider(|| view.clone());
    // The same context the real `App` provides, so screens fetch from
    // wherever the test put the server.
    use_context_provider(|| Signal::new(base_url.clone()));
    rsx! { Router::<HarnessRoute> {} }
}

fn harness(view: impl Fn() -> Element + 'static, base_url: &str) -> VirtualDom {
    VirtualDom::new_with_props(
        Harness,
        HarnessProps {
            view: View::new(view),
            base_url: base_url.to_string(),
        },
    )
}

/// Render a view once, synchronously. For components that take their data as
/// props and fetch nothing.
pub fn render(view: impl Fn() -> Element + 'static) -> String {
    let mut dom = harness(view, "");
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

/// Render a screen against `base_url`, letting its fetches resolve, until
/// `done` accepts the HTML. Panics with the last HTML rendered if that never
/// happens — a screen stuck on "Loading…" is otherwise a silent timeout.
pub async fn render_against_archive(
    base_url: &str,
    view: impl Fn() -> Element + 'static,
    done: impl Fn(&str) -> bool,
) -> String {
    let mut dom = harness(view, base_url);
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

/// Start a real Trip Archive server on an ephemeral port, backed by a fresh
/// temporary database and blob store — the same "real collaborators, mock
/// only externals" setup the server's own tests use (ADR-0012).
///
/// Keep the returned `TempDir` alive for the whole test: dropping it deletes
/// the database out from under the running server.
pub async fn serve_test_archive() -> (String, tempfile::TempDir) {
    use trip_archive::server::{
        db, http,
        state::AppState,
        storage::{BlobStore, LocalDisk},
    };

    let dir = tempfile::tempdir().expect("temp dir");
    let pool = db::create_pool(&dir.path().join("test.db"))
        .await
        .expect("create pool");
    let store: Arc<dyn BlobStore> = Arc::new(LocalDisk::new(dir.path().join("blobs")));
    let router = http::router(AppState::new(pool, store, None));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind an ephemeral port");
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve");
    });

    (base_url, dir)
}
