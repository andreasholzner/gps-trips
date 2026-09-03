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
mod komoot;
mod list;
mod login;
mod photos;
mod region;
#[cfg(test)]
mod test_support;
mod track;
mod trip_table;
mod trip_tags;

use api::ApiClient;
use detail::TripDetail;
use filters::Filters;
use import::ImportTrip;
use komoot::KomootSync;
use list::TripList;
use login::Login;

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
    /// Reviewing and running a Komoot sync (US-44) — the last screen the
    /// proof-of-concept UI still owned. Same path it had there, so its
    /// redirect is a straight move under `/app`.
    #[route("/komoot/sync")]
    KomootSync {},
}

fn main() {
    dioxus::launch(App);
}

/// Whether there is a session, which is what the app renders around
/// (US-19). `Resolving` is the moment before the archive has answered: the
/// screens must not fetch yet — they would fetch from an unresolved origin —
/// and showing a login screen the owner may not need would be a flicker,
/// not information.
#[derive(Clone, PartialEq)]
enum Access {
    Resolving,
    /// No session. `notice` carries what the owner should know about how
    /// they got here: an archive that could not be reached at all, so it is
    /// not disguised as a password prompt, or a session that ended under
    /// them.
    SignedOut {
        notice: Option<String>,
    },
    SignedIn,
}

/// What the login screen says when a session ends mid-use. Rotating the
/// password is the archive's only revocation (ADR-0010's amendment), so this
/// is what the owner meets on every other device after doing it — and being
/// told beats a screen that simply asks for the password again.
const SESSION_ENDED: &str = "Your session has ended. Sign in again.";

/// Resolves the API base URL and asks the archive who we are, before
/// anything can query it, and hands the resulting client to every screen as
/// context (the test harness provides the same context, so screens fetch
/// from wherever a test put the server, already signed in).
#[component]
fn App() -> Element {
    let mut archive = use_signal(ApiClient::default);
    let mut access = use_signal(|| Access::Resolving);
    // Set by the API client when the archive stops recognising us — a
    // rotated password, or a session that finally ran out. It arrives on
    // whatever a screen fetched next, so it is watched here rather than
    // handled screen by screen.
    let mut refused = use_signal(|| false);

    use_effect(move || {
        if refused() {
            refused.set(false);
            // `peek`, not a read: this is about what the app is showing, not
            // something to re-run the effect for. And only while signed *in*
            // — a refused sign-in is the login screen's own business, and its
            // "that is not the password" must not be overwritten by this.
            if *access.peek() == Access::SignedIn {
                access.set(Access::SignedOut {
                    notice: Some(SESSION_ENDED.to_string()),
                });
            }
        }
    });

    use_future(move || async move {
        let client = ApiClient::new(resolve_origin().await).reporting_refusals_to(refused);
        // One request answers both questions: a session, or the gate's 401
        // saying there is none (US-19).
        let resolved = match api::session(&client).await {
            Ok(_) => Access::SignedIn,
            Err(err) if err.is_unauthorized() => Access::SignedOut { notice: None },
            Err(err) => Access::SignedOut {
                notice: Some(err.to_string()),
            },
        };
        archive.set(client);
        access.set(resolved);
    });

    use_context_provider(|| archive);

    let body = match access() {
        Access::Resolving => rsx! { p { "Starting…" } },
        Access::SignedOut { notice } => rsx! {
            Login {
                archive: archive(),
                notice,
                // The token matters only where no cookie store does the
                // carrying — Android (US-16), and the host target. In a
                // browser the archive's own cookie is already the credential
                // and this is a spare copy.
                on_signed_in: move |token: String| {
                    archive.set(archive().with_token(token).reporting_refusals_to(refused));
                    access.set(Access::SignedIn);
                },
            }
        },
        Access::SignedIn => rsx! {
            // Web only, deliberately. Signing out is a "leave this device
            // clean while I am still holding it" action, which is a browser
            // situation: the archive can be opened in one you are about to
            // walk away from. The Android app cannot be, and the case where
            // its access *should* be revoked — a lost or stolen phone — is
            // the one case where no button on that phone can be reached.
            // Rotating the password is the answer there, and it is the answer
            // whether or not this exists (US-16).
            if cfg!(feature = "web") {
                nav { class: "session",
                    button {
                        r#type: "button",
                        id: "sign-out",
                        onclick: move |_| async move {
                            let client = archive();
                            // Whether the archive heard or not, this client is
                            // done with the session: clearing it locally is what
                            // signing out means here.
                            let _ = api::logout(&client).await;
                            archive.set(
                                ApiClient::new(client.base_url()).reporting_refusals_to(refused),
                            );
                            access.set(Access::SignedOut { notice: None });
                        },
                        "Sign out"
                    }
                }
            }
            Router::<Route> {}
        },
    };

    rsx! {
        document::Link { rel: "stylesheet", href: PICO_CSS }
        document::Link { rel: "stylesheet", href: APP_CSS }
        // Injected asynchronously, so anything using `L` waits for it
        // (interop.rs) rather than assuming load order.
        document::Link { rel: "stylesheet", href: LEAFLET_CSS }
        document::Script { src: LEAFLET_JS }
        document::Link { rel: "stylesheet", href: UPLOT_CSS }
        document::Script { src: UPLOT_JS }

        main { {body} }
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
    fn the_komoot_url_round_trips_and_keeps_the_path_the_page_had() {
        // The server redirects `/komoot/sync` here, so this path is part of
        // that contract rather than an internal detail (US-44).
        let url = Route::KomootSync {}.to_string();
        assert_eq!(url, "/komoot/sync");

        let parsed = Route::from_str(&url).expect("the router must parse a URL it just wrote");
        assert!(
            matches!(parsed, Route::KomootSync {}),
            "a komoot URL must parse back to the sync screen; url was {url:?}"
        );
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
