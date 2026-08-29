//! Trip Archive's Dioxus SPA (ADR-0024): one crate built for two platforms —
//! a client-side-rendered WASM SPA on the web and an Android app (US-16) —
//! both against the JSON API (ADR-0008). US-41 builds the trip-list screen;
//! US-42/43/44 add the remaining screens, after which the server-rendered
//! PoC UI is retired (ADR-0012's migration rule).

use dioxus::prelude::*;

mod api;
mod bulk_tag;
mod filters;
mod format;
mod list;
#[cfg(test)]
mod test_support;
mod trip_table;

use list::TripList;

/// Pico's classless build (MIT, v2.1.1), vendored rather than fetched from a
/// CDN: the archive is self-contained (US-10) and the Android app has no
/// server to fetch from at all. Classless because the markup is plain
/// elements — a fieldset is a fieldset — so styling costs no class names in
/// the components. `app.css` holds only what Pico has no opinion about.
const PICO_CSS: Asset = asset!("/assets/pico.classless.min.css");
const APP_CSS: Asset = asset!("/assets/app.css");

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
        document::Link { rel: "stylesheet", href: PICO_CSS }
        document::Link { rel: "stylesheet", href: APP_CSS }

        main {
            if loaded() {
                Router::<Route> {}
            } else {
                p { "Starting…" }
            }
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
