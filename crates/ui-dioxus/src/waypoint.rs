//! A collection's waypoint on a page of its own (US-98): its details, a map
//! with it and the line of the track it belongs to, and where it is
//! corrected — its details in a form over the screen, its position as a
//! photo is placed (US-30) — or deleted.

use dioxus::prelude::*;
use trip_archive_types::WaypointDetail;

use crate::activity_color;
use crate::api::{self, ApiClient, LinkEnd};
use crate::collection::collection_route;
use crate::format;
use crate::interop::{self, MapWaypoint, OverviewEvent, OverviewLine};
use crate::linked::LinkedTo;
use crate::track;
use crate::waypoint_icon::WaypointSymbol;
use crate::Route;

mod edit;
mod position;

use edit::EditWaypoint;
use position::MoveWaypoint;

/// What a waypoint the file gave no name is called.
pub const UNNAMED: &str = "Unnamed waypoint";

/// A waypoint's name, or what one without is called.
pub fn name_of(waypoint: &trip_archive_types::Waypoint) -> &str {
    waypoint.name.as_deref().unwrap_or(UNNAMED)
}

/// The screen. `id` comes from the route (`/waypoints/:id`).
#[component]
pub fn Waypoint(id: i64) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    // The id travels back with the answer, as on a collection track's page.
    let mut waypoint = use_resource(use_reactive!(|id| async move {
        (id, api::get_waypoint(&archive(), id).await)
    }));

    rsx! {
        match &*waypoint.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            Some((read_for, _)) if *read_for != id => rsx! { p { "Loading…" } },
            Some((_, Err(err))) if err.is_not_found() => rsx! {
                p { class: "error", "There is no such waypoint — it may have been deleted." }
            },
            Some((_, Err(err))) => rsx! {
                p { class: "error", "Could not load this waypoint: {err}" }
            },
            Some((_, Ok(detail))) => rsx! {
                WaypointDetails { detail: detail.clone() }
                LinkedTo { end: LinkEnd::Waypoint(id) }
                WaypointMap { detail: detail.clone() }
                div { class: "trip-actions",
                    EditWaypoint {
                        waypoint: detail.waypoint.clone(),
                        on_saved: move |_| waypoint.restart(),
                    }
                    MoveWaypoint { detail: detail.clone(), on_saved: move |_| waypoint.restart() }
                    DeleteWaypoint {
                        id,
                        name: name_of(&detail.waypoint).to_string(),
                        collection_id: detail.collection_id,
                    }
                }
            },
        }
    }
}

/// The waypoint's name under its icon, its collection, and its details.
#[component]
fn WaypointDetails(detail: WaypointDetail) -> Element {
    let waypoint = &detail.waypoint;
    rsx! {
        hgroup {
            h1 { id: "waypoint-name",
                WaypointSymbol { symbol: waypoint.symbol.clone() }
                "{name_of(waypoint)}"
            }
            p { id: "collection-of-waypoint",
                "From the collection "
                Link { to: collection_route(detail.collection_id), "{detail.collection_name}" }
            }
        }
        dl { class: "stats",
            div {
                dt { "Symbol" }
                dd { id: "waypoint-symbol", {format::or_dash(waypoint.symbol.as_deref())} }
            }
            div {
                dt { "Elevation" }
                dd { id: "waypoint-ele", {format::metres(waypoint.ele)} }
            }
            div {
                dt { "Position" }
                dd { id: "waypoint-position", {format::position(waypoint.lat, waypoint.lon)} }
            }
        }
        if let Some(description) = &waypoint.description {
            p { id: "waypoint-description", class: "collection-description", "{description}" }
        }
    }
}

/// The waypoint as its collection's map draws it, with the line of the track
/// it belongs to — a click on which opens the track — and nothing else of
/// the collection.
#[component]
fn WaypointMap(detail: WaypointDetail) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let _draw = use_resource(use_reactive!(|detail| async move {
        let mut lines = Vec::new();
        if let Some(track) = &detail.track {
            // Without its line the waypoint is still worth showing.
            match api::get_collection_track_geometry(&archive(), track.id).await {
                Ok(geometry) => lines.push(OverviewLine {
                    id: track.id,
                    name: track.name.clone(),
                    color: activity_color::color(track.activity_type),
                    points: track::polyline(&geometry),
                }),
                Err(err) => dioxus::logger::tracing::error!("could not read the track: {err}"),
            }
        }
        let mut map = interop::start_overview_map(lines, vec![MapWaypoint::from(&detail.waypoint)]);
        while let Ok(event) = map.recv::<OverviewEvent>().await {
            if let OverviewEvent::Open(track) = event {
                navigator().push(Route::CollectionTrack { id: track });
            }
        }
    }));

    rsx! { div { id: "overview-map", class: "overview-map" } }
}

/// The confirmation's question: which waypoint, that nothing else goes with
/// it, and that it is final.
pub fn delete_question(name: &str) -> String {
    format!(
        "Delete the waypoint “{name}”? Its collection and its tracks stay. \
         This cannot be undone."
    )
}

/// Deleting the waypoint, after a confirmation naming it; the screen goes
/// back to its collection.
#[component]
fn DeleteWaypoint(id: i64, name: String, collection_id: i64) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut arming = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    // The router shows the next one through this same scope; an armed
    // confirmation must not survive what it was armed on.
    use_effect(use_reactive!(|id| {
        let _ = id;
        arming.set(false);
        error.set(None);
    }));

    rsx! {
        if arming() {
            p { class: "confirm",
                "{delete_question(&name)} "
                button {
                    r#type: "button",
                    class: "danger",
                    disabled: deleting(),
                    onclick: move |_| async move {
                        if deleting() {
                            return;
                        }
                        deleting.set(true);
                        match api::delete_waypoint(&archive(), id).await {
                            Ok(()) => {
                                navigator().replace(collection_route(collection_id));
                            }
                            Err(err) => {
                                arming.set(false);
                                error.set(Some(err.to_string()));
                            }
                        }
                        deleting.set(false);
                    },
                    "Delete it"
                }
                button { r#type: "button", onclick: move |_| arming.set(false), "Cancel" }
            }
        } else {
            button {
                id: "delete-waypoint",
                r#type: "button",
                class: "quiet danger",
                onclick: move |_| {
                    error.set(None);
                    arming.set(true);
                },
                "Delete waypoint"
            }
        }
        if let Some(message) = error() {
            p { class: "error", "Could not delete this waypoint: {message}" }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
