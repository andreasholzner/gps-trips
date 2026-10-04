//! US-53 — what someone holding a share's link sees: no login, no menu, and
//! nothing that changes a trip — a few trips, or a tag summary (US-82). The
//! screens read through the client the app made for the share
//! ([`ApiClient::for_share`]), so every call already goes to the share's own
//! routes.

use dioxus::prelude::*;
use trip_archive_types::{ShareOverview, SharedTrip, SharedTripSummary, TripDetail, TripTrack};

use crate::activity_color::{self, ActivityLegend, Swatch};
use crate::api::{self, ApiClient, ApiError};
use crate::format;
use crate::interop::{self, OverviewEvent, OverviewLine};
use crate::track;
use crate::Route;

mod detail;
mod summary;

pub use detail::SharedTripView;
pub use summary::SharedSummaryView;

/// What a link that opens nothing says — unknown, expired or stopped, which
/// the archive deliberately does not tell apart.
const DEAD_LINK: &str = "This link does not work (any more). Ask whoever sent it for a new one.";

/// The share's own page, `/s/:token`: its title, a map of every track, and
/// the trips. A share of one trip is that trip's page straight away; a
/// share of tags is their summary, however many trips it holds.
#[component]
pub fn Shared(token: String) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let overview = use_resource(move || async move { api::share_overview(&archive()).await });

    match &*overview.read_unchecked() {
        None => rsx! { p { "Loading…" } },
        Some(Err(err)) => rsx! { ShareError { err: err.clone() } },
        Some(Ok(overview)) if overview.summary.is_some() => rsx! {
            SharedSummaryView { token, overview: overview.clone() }
        },
        Some(Ok(overview)) => match &overview.trips[..] {
            [only] => rsx! {
                SharedTripView { token, id: only.id, overview: overview.clone() }
            },
            _ => rsx! { SharedTrips { token, overview: overview.clone() } },
        },
    }
}

/// `/s/:token/trips/:id` — one trip of a share of several, or of a shared
/// summary.
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

/// The title a share goes by: its label, or else a shared summary's tag
/// names (US-82), or else a plain description.
pub fn title(overview: &ShareOverview) -> String {
    match (&overview.label, &overview.summary) {
        (Some(label), _) => label.clone(),
        (None, Some(summary)) => summary
            .tags
            .iter()
            .map(|tag| tag.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        (None, None) => "Shared trips".to_string(),
    }
}

/// The list of a share's trips, under a map of them all. Each row carries its
/// line's color, and pointing at either highlights both (US-72).
#[component]
fn SharedTrips(token: String, overview: ShareOverview) -> Element {
    let title = title(&overview);
    let mut highlighted = use_signal(|| None::<i64>);
    rsx! {
        h1 { id: "share-title", "{title}" }
        OverviewMap { token: token.clone(), trips: overview.trips.clone(), highlighted }
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
                for (trip, color) in overview.trips.iter().zip(trip_colors(&overview.trips)) {
                    tr {
                        key: "{trip.id}",
                        class: if highlighted() == Some(trip.id) { "highlighted" },
                        onmouseenter: {
                            let id = trip.id;
                            move |_| highlighted.set(Some(id))
                        },
                        onmouseleave: move |_| highlighted.set(None),
                        onfocusin: {
                            let id = trip.id;
                            move |_| highlighted.set(Some(id))
                        },
                        onfocusout: move |_| highlighted.set(None),
                        td { class: "shared-trip-name",
                            Swatch { color }
                            Link {
                                to: Route::SharedTripDetail { token: token.clone(), id: trip.id },
                                "{trip.name}"
                            }
                        }
                        td { {format::or_dash(trip.start_date.as_deref())} }
                        td { "{trip.activity_type.label()}" }
                        td { {format::km(trip.distance_m)} }
                    }
                }
            }
        }
    }
}

/// Every track on one map; clicking one opens that trip, and pointing at one
/// highlights it and its row (US-72). Under it, which color is which
/// activity, for the lines actually drawn (US-75).
#[component]
fn OverviewMap(
    token: String,
    trips: Vec<SharedTripSummary>,
    highlighted: Signal<Option<i64>>,
) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut shown = use_signal(Vec::new);
    let mut handle = use_signal(|| None::<document::Eval>);
    let _draw = use_resource(move || {
        let trips = trips.clone();
        let token = token.clone();
        async move {
            // Every shared trip's track in one request (US-73). One that
            // cannot be read is absent from the answer and leaves its line
            // off the map; the trip is still in the list below it. A request
            // that fails outright leaves the map without lines.
            let ids: Vec<i64> = trips.iter().map(|trip| trip.id).collect();
            let tracks = api::list_tracks(&archive(), &ids)
                .await
                .unwrap_or_else(|err| {
                    dioxus::logger::tracing::error!("could not read the shared tracks: {err}");
                    Vec::new()
                });
            shown.set(
                trips
                    .iter()
                    .filter(|trip| tracks.iter().any(|track| track.id == trip.id))
                    .map(|trip| trip.activity_type)
                    .collect(),
            );
            let mut map = interop::start_overview_map(overview_lines(&trips, &tracks));
            handle.set(Some(map));
            while let Ok(event) = map.recv::<OverviewEvent>().await {
                match event {
                    OverviewEvent::Open(id) => {
                        navigator().push(Route::SharedTripDetail {
                            token: token.clone(),
                            id,
                        });
                    }
                    OverviewEvent::Hover(id) => highlighted.set(id),
                }
            }
        }
    });
    // Whichever side pointed, the map follows the one highlighted trip.
    use_effect(move || {
        let id = highlighted();
        if let Some(map) = handle.read().as_ref() {
            interop::highlight_on_overview_map(map, id);
        }
    });
    rsx! {
        div { id: "overview-map", class: "overview-map" }
        ActivityLegend { shown: shown() }
    }
}

/// Each trip's color, in the list's order (US-72).
pub fn trip_colors(trips: &[SharedTripSummary]) -> Vec<&'static str> {
    activity_color::in_list_order(trips.iter().map(|trip| trip.activity_type))
}

/// One line per trip whose track was read, named after the trip and in its
/// color (US-72, US-75). `tracks` lacks the ones that could not be read, so
/// each is matched to its trip by id rather than by position. The colors are
/// taken over every trip, so one that could not be read shifts no other.
pub fn overview_lines(trips: &[SharedTripSummary], tracks: &[TripTrack]) -> Vec<OverviewLine> {
    trips
        .iter()
        .zip(trip_colors(trips))
        .filter_map(|(trip, color)| {
            let track = tracks.iter().find(|track| track.id == trip.id)?;
            Some(OverviewLine {
                id: trip.id,
                name: trip.name.clone(),
                color,
                points: track::lat_lon(&track.coordinates),
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
        moving_secs: trip.moving_secs,
        moving_distance_m: trip.moving_distance_m,
        climb_gain_m: trip.climb_gain_m,
        climb_secs: trip.climb_secs,
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
