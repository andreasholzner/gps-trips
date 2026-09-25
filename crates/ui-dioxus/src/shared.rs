//! US-53 — what someone holding a share's link sees: no login, no menu, and
//! nothing that changes a trip. The screens read through the client the app
//! made for the share ([`ApiClient::for_share`]), so every call already goes
//! to the share's own routes.

use dioxus::prelude::*;
use trip_archive_types::{ShareOverview, SharedTrip, SharedTripSummary, TripDetail};

use crate::api::{self, ApiClient, ApiError};
use crate::format;
use crate::interop::{self, OverviewLine};
use crate::track::{self, Track};
use crate::Route;

mod detail;

pub use detail::SharedTripView;

/// What a link that opens nothing says — unknown, expired or stopped, which
/// the archive deliberately does not tell apart.
const DEAD_LINK: &str = "This link does not work (any more). Ask whoever sent it for a new one.";

/// The share's own page, `/s/:token`: its title, a map of every track, and
/// the trips. A share of one trip is that trip's page straight away.
#[component]
pub fn Shared(token: String) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let overview = use_resource(move || async move { api::share_overview(&archive()).await });

    match &*overview.read_unchecked() {
        None => rsx! { p { "Loading…" } },
        Some(Err(err)) => rsx! { ShareError { err: err.clone() } },
        Some(Ok(overview)) => match &overview.trips[..] {
            [only] => rsx! {
                SharedTripView { token, id: only.id, overview: overview.clone() }
            },
            _ => rsx! { SharedTrips { token, overview: overview.clone() } },
        },
    }
}

/// `/s/:token/trips/:id` — one trip of a share of several.
#[component]
pub fn SharedTripDetail(token: String, id: i64) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let overview = use_resource(move || async move { api::share_overview(&archive()).await });

    match &*overview.read_unchecked() {
        None => rsx! { p { "Loading…" } },
        Some(Err(err)) => rsx! { ShareError { err: err.clone() } },
        Some(Ok(overview)) => rsx! {
            SharedTripView { token, id, overview: overview.clone() }
        },
    }
}

/// Why a share could not be read, in the recipient's terms.
#[component]
fn ShareError(err: ApiError) -> Element {
    if err.is_not_found() {
        rsx! { p { class: "error", "{DEAD_LINK}" } }
    } else {
        rsx! { p { class: "error", "Could not load what was shared: {err}" } }
    }
}

/// The title a share goes by: its label, or a plain description.
pub fn title(overview: &ShareOverview) -> &str {
    overview.label.as_deref().unwrap_or("Shared trips")
}

/// The list of a share's trips, under a map of them all.
#[component]
fn SharedTrips(token: String, overview: ShareOverview) -> Element {
    let title = title(&overview).to_string();
    rsx! {
        h1 { id: "share-title", "{title}" }
        OverviewMap { token: token.clone(), trips: overview.trips.clone() }
        table { id: "shared-trips",
            thead {
                tr {
                    th { "Name" }
                    th { "Date" }
                    th { "Activity" }
                    th { "Distance" }
                }
            }
            tbody {
                for trip in overview.trips.iter() {
                    tr { key: "{trip.id}",
                        td {
                            Link {
                                to: Route::SharedTripDetail { token: token.clone(), id: trip.id },
                                "{trip.name}"
                            }
                        }
                        td { {format::date(trip.start_time.as_deref())} }
                        td { "{trip.activity_type.label()}" }
                        td { {format::km(trip.distance_m)} }
                    }
                }
            }
        }
    }
}

/// Every track on one map; clicking one opens that trip.
#[component]
fn OverviewMap(token: String, trips: Vec<SharedTripSummary>) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let _draw = use_resource(move || {
        let trips = trips.clone();
        let token = token.clone();
        async move {
            let mut tracks = Vec::with_capacity(trips.len());
            for trip in &trips {
                // A track that cannot be read leaves its line off the map;
                // the trip is still in the list below it.
                if let Ok(track) = api::get_track(&archive(), trip.id).await {
                    tracks.push((trip.id, track));
                }
            }
            let mut map = interop::start_overview_map(overview_lines(&trips, &tracks));
            while let Ok(id) = map.recv::<i64>().await {
                navigator().push(Route::SharedTripDetail {
                    token: token.clone(),
                    id,
                });
            }
        }
    });
    rsx! { div { id: "overview-map", class: "overview-map" } }
}

/// One line per trip whose track was read, named after the trip. `tracks`
/// lacks the ones that could not be read, so each is matched to its trip by
/// id rather than by position.
pub fn overview_lines(trips: &[SharedTripSummary], tracks: &[(i64, Track)]) -> Vec<OverviewLine> {
    trips
        .iter()
        .filter_map(|trip| {
            let (_, track) = tracks.iter().find(|(id, _)| *id == trip.id)?;
            Some(OverviewLine {
                id: trip.id,
                name: trip.name.clone(),
                points: track::polyline(track),
            })
        })
        .collect()
}

/// The recipient's trip as the detail screen's stats take it. Built here, on
/// the recipient's side, from what the share already chose to send — the
/// owner-only fields are simply absent.
pub fn as_trip_detail(trip: &SharedTrip) -> TripDetail {
    TripDetail {
        id: trip.id,
        name: trip.name.clone(),
        activity_type: trip.activity_type,
        tz_name: trip.tz_name.clone(),
        start_time: trip.start_time.clone(),
        start_date: trip.start_date.clone(),
        end_time: trip.end_time.clone(),
        distance_m: trip.distance_m,
        ascent_m: trip.ascent_m,
        descent_m: trip.descent_m,
        duration_secs: trip.duration_secs,
        min_lat: trip.min_lat,
        min_lon: trip.min_lon,
        max_lat: trip.max_lat,
        max_lon: trip.max_lon,
        komoot: None,
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
