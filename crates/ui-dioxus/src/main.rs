//! Dioxus spike (docs/dioxus-spike.md): the trip list and trip detail screens
//! as a client-side-rendered WASM SPA against the existing JSON API
//! (ADR-0008), served alongside — not in place of — the current pages.
//!
//! Scope is deliberately two screens: enough to judge Dioxus against Leptos
//! for the eventual UI decision (ADR-0001), not a replacement UI. Editing,
//! tagging, import, Komoot sync and auth are out of scope.

use dioxus::prelude::*;

mod api;
mod detail;
mod filters;
mod format;
mod interop;
mod list;

/// The two screens. Paths mirror the server-rendered app's own (`/` and
/// `/trips/:id`); the deployed bundle is mounted under `/app` (Dioxus.toml's
/// `base_path`), which the router applies for us.
#[derive(Routable, Clone, PartialEq)]
enum Route {
    #[route("/")]
    TripList {},
    #[route("/trips/:id")]
    TripDetail { id: i64 },
}

use detail::TripDetail;
use list::TripList;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! { Router::<Route> {} }
}
