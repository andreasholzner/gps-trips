//! Trip Archive's Dioxus SPA (ADR-0024): one crate built for two platforms —
//! a client-side-rendered WASM SPA on the web and an Android app (US-16) —
//! both against the JSON API (ADR-0008). US-41 builds the trip-list screen;
//! US-42/43/44 add the remaining screens, after which the server-rendered
//! PoC UI is retired (ADR-0012's migration rule).

use dioxus::prelude::*;

mod list;

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

#[component]
fn App() -> Element {
    rsx! {
        Router::<Route> {}
    }
}
