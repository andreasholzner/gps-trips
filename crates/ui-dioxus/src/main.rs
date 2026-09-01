//! Trip Archive's Dioxus SPA (ADR-0024): one crate built for two platforms —
//! a client-side-rendered WASM SPA on the web and an Android app (US-16) —
//! both against the JSON API (ADR-0008). US-41 builds the trip-list screen;
//! US-42/43/44 add the remaining screens, after which the server-rendered
//! PoC UI is retired (ADR-0012's migration rule).

use dioxus::prelude::*;

mod api;
mod bulk_tag;
mod delete;
mod detail;
mod edit;
mod filters;
mod format;
mod import;
mod interop;
mod list;
mod photos;
mod region;
#[cfg(test)]
mod test_support;
mod track;
mod trip_table;
mod trip_tags;

use detail::TripDetail;
use filters::Filters;
use import::ImportTrip;
use list::TripList;

/// Pico's classless build (MIT, v2.1.1), vendored rather than fetched from a
/// CDN: the archive is self-contained (US-10) and the Android app has no
/// server to fetch from at all. Classless because the markup is plain
/// elements — a fieldset is a fieldset — so styling costs no class names in
/// the components. `app.css` holds only what Pico has no opinion about.
const PICO_CSS: Asset = asset!("/assets/pico.classless.min.css");
const APP_CSS: Asset = asset!("/assets/app.css");

/// Leaflet and OSM raster tiles, kept from ADR-0005 and vendored rather than
/// fetched from a CDN (US-10). Bundled with `asset!` so it ships inside the
/// APK too, where there is no server to fetch it from ([ADR-0025](./adr/0025-js-widget-interop-via-eval.md)).
///
/// This is a second copy of `public/vendor`'s file — `asset!` refuses paths
/// outside the crate, symlinks included — which that ADR accepts and which
/// resolves when the PoC UI retires (US-42).
const LEAFLET_CSS: Asset = asset!("/assets/leaflet.css");
const LEAFLET_JS: Asset = asset!("/assets/leaflet.js");

/// uPlot, the elevation profile's chart library, on exactly the same terms
/// (ADR-0006, kept by ADR-0025). Only the detail screen draws one, but the
/// bundle is one artifact and the library is 50 KB.
const UPLOT_CSS: Asset = asset!("/assets/uPlot.min.css");
const UPLOT_JS: Asset = asset!("/assets/uPlot.iife.min.js");

/// The screens. The trip-list path mirrors the server-rendered app's own
/// (`/`); the deployed web bundle is mounted under `/app` (Dioxus.toml's
/// `base_path`), which the router applies for us.
///
/// The filters live in the query string (US-52), so a narrowed list is
/// bookmarkable and survives a reload — the same property the
/// server-rendered page had for free, and what lets the region rectangle be
/// restored onto the map on the next load.
#[derive(Routable, Clone, PartialEq)]
enum Route {
    #[route("/?:..filters")]
    TripList { filters: Filters },
    /// One trip, by id (US-42) — the target of every row in the list.
    #[route("/trips/:id")]
    TripDetail { id: i64 },
    /// Importing a trip (US-43/US-12). Where the server-rendered `/import`
    /// page used to be, and where it now redirects.
    #[route("/import")]
    ImportTrip {},
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
        // Injected asynchronously, so anything using `L` waits for it
        // (interop.rs) rather than assuming load order.
        document::Link { rel: "stylesheet", href: LEAFLET_CSS }
        document::Script { src: LEAFLET_JS }
        document::Link { rel: "stylesheet", href: UPLOT_CSS }
        document::Script { src: UPLOT_JS }

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
// Appended to main.rs as a test module.
#[cfg(test)]
mod route_tests {
    use super::*;
    use std::str::FromStr;
    use trip_archive_types::{ActivityType, TripKind};

    /// The router owns the URL, and it percent-decodes the whole query string
    /// before handing it to `FromQuery`. That interacts with this crate's own
    /// escaping, so the round trip is asserted through `Route` itself rather
    /// than reasoned about.
    fn round_trip(filters: Filters) {
        let url = Route::TripList {
            filters: filters.clone(),
        }
        .to_string();
        let parsed = Route::from_str(&url).expect("the router must parse a URL it just wrote");
        let Route::TripList { filters: back } = parsed else {
            panic!("a list URL must parse back to the list screen; url was {url:?}")
        };
        assert_eq!(back, filters, "url was {url:?}");
    }

    #[test]
    fn a_plain_filter_round_trips_through_the_url() {
        round_trip(Filters {
            kind: TripKind::Planned,
            q: "oslo".to_string(),
            activity: Some(ActivityType::Hiking),
            tags: vec!["alpine".to_string()],
            ..Default::default()
        });
    }

    #[test]
    fn a_search_containing_url_separators_round_trips_through_the_url() {
        // `&` and `=` inside a value are the case that breaks a naive scheme.
        round_trip(Filters {
            q: "b&b = 100% fun?".to_string(),
            ..Default::default()
        });
    }

    #[test]
    fn a_trips_url_round_trips_through_the_detail_route() {
        // The property the detail screen rests on (US-42): a row's link, a
        // bookmark and a reload are the same URL, and it names one trip.
        let url = Route::TripDetail { id: 42 }.to_string();
        assert_eq!(url, "/trips/42");

        let parsed = Route::from_str(&url).expect("the router must parse a URL it just wrote");
        let Route::TripDetail { id } = parsed else {
            panic!("a trip URL must parse back to the detail screen; url was {url:?}")
        };
        assert_eq!(id, 42);
    }

    #[test]
    fn the_bare_app_url_is_the_default_view() {
        let parsed = Route::from_str("/").expect("the bare path must parse");
        let Route::TripList { filters } = parsed else {
            panic!("the bare path must be the list screen")
        };
        assert_eq!(filters, Filters::default());
    }
}
