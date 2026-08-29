//! Trip Archive's Dioxus SPA (ADR-0024): one crate built for two platforms —
//! a client-side-rendered WASM SPA on the web and an Android app (US-16) —
//! both against the JSON API (ADR-0008). US-41 builds the trip-list screen;
//! US-42/43/44 add the remaining screens, after which the server-rendered
//! PoC UI is retired (ADR-0012's migration rule).

use dioxus::prelude::*;

mod api;
mod format;
mod list;
#[cfg(test)]
mod test_support;

use list::TripList;

/// The screens. The trip-list path mirrors the server-rendered app's own
/// (`/`); the deployed web bundle is mounted under `/app` (Dioxus.toml's
/// `base_path`), which the router applies for us.
#[derive(Routable, Clone, PartialEq)]
enum Route {
    #[route("/")]
    TripList {},
}

fn main() {
    dioxus::launch(App);
}

/// Resolves the API base URL before anything can query it, and hands it to
/// every screen as context (the test harness provides the same context, so
/// screens fetch from wherever a test put the server).
#[component]
fn App() -> Element {
    let mut base_url = use_signal(String::new);
    let mut loaded = use_signal(|| false);

    use_future(move || async move {
        base_url.set(resolve_origin().await);
        loaded.set(true);
    });

    use_context_provider(|| base_url);

    rsx! {
        if loaded() {
            Router::<Route> {}
        } else {
            p { "Starting…" }
        }
    }
}

/// The page's own origin on the web: `reqwest` — unlike a browser `fetch`
/// wrapper — rejects relative URLs outright, so the web build resolves its
/// origin explicitly at startup. Empty elsewhere; the Android app's
/// owner-configured address arrives with US-16.
async fn resolve_origin() -> String {
    if cfg!(target_arch = "wasm32") {
        let mut eval = document::eval("dioxus.send(window.location.origin);");
        eval.recv::<String>().await.unwrap_or_default()
    } else {
        String::new()
    }
}
