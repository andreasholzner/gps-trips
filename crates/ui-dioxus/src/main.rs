//! Dioxus spike (docs/dioxus-spike.md): the trip list and trip detail screens
//! as a Rust UI running on two platforms from one source — a client-side
//! rendered WASM SPA on the web, and an Android app — both against the
//! existing JSON API (ADR-0008), alongside rather than in place of the
//! current pages.
//!
//! Scope is deliberately two screens plus a settings screen: enough to judge
//! Dioxus against Leptos for the eventual UI decision (ADR-0001) and to test
//! the multi-platform claim that motivated looking at Dioxus at all. Editing,
//! tagging, import, Komoot sync and auth are out of scope.

use dioxus::prelude::*;

mod api;
mod detail;
mod filters;
mod format;
mod interop;
mod list;
mod settings;

/// The screens. The two trip paths mirror the server-rendered app's own (`/`
/// and `/trips/:id`); the deployed web bundle is mounted under `/app`
/// (Dioxus.toml's `base_path`), which the router applies for us.
#[derive(Routable, Clone, PartialEq)]
enum Route {
    #[route("/")]
    TripList {},
    #[route("/trips/:id")]
    TripDetail { id: i64 },
    #[route("/settings")]
    Settings {},
}

use detail::TripDetail;
use list::TripList;
use settings::Settings;

/// The vendored, self-hosted map/chart libraries (ADR-0005/0006, US-10),
/// bundled into the build: served from the web bundle on the web, and
/// packaged inside the APK on Android, where there is no server to fetch them
/// from at all.
///
/// `assets/` is a second copy of `public/vendor`'s files, not a symlink to
/// them: `asset!` resolves symlinks and then rejects anything outside the
/// crate. Duplicating vendored libraries is a real (if small) cost of the
/// asset pipeline — noted in docs/dioxus-spike.md rather than hidden.
const LEAFLET_CSS: Asset = asset!("/assets/leaflet.css");
const LEAFLET_JS: Asset = asset!("/assets/leaflet.js");
const UPLOT_CSS: Asset = asset!("/assets/uPlot.min.css");
const UPLOT_JS: Asset = asset!("/assets/uPlot.iife.min.js");

fn main() {
    dioxus::launch(App);
}

/// Loads the configured API base URL before anything can query it, and hands
/// it to every screen as context.
///
/// On the web this resolves to `""` — same-origin, nothing to configure — so
/// the extra step costs one `localStorage` read. On Android an unconfigured
/// app has nowhere to fetch from, so it opens on the settings screen instead
/// of a list that could only ever fail.
#[component]
fn App() -> Element {
    let mut base_url = use_signal(String::new);
    let mut loaded = use_signal(|| false);

    use_future(move || async move {
        interop::install_ready_helper();
        base_url.set(settings::load().await);
        loaded.set(true);
    });

    use_context_provider(|| base_url);

    rsx! {
        document::Link { rel: "stylesheet", href: LEAFLET_CSS }
        document::Link { rel: "stylesheet", href: UPLOT_CSS }
        document::Script { src: LEAFLET_JS }
        document::Script { src: UPLOT_JS }

        if !loaded() {
            p { "Starting…" }
        } else if settings::needs_configuration(&base_url()) {
            Settings {}
        } else {
            Router::<Route> {}
        }
    }
}
