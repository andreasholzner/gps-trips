//! A collection's waypoints below its tracks (US-98), each leading to its
//! own page, where it is corrected, and marked when it has a recorded trip
//! (US-99).

use dioxus::prelude::*;
use trip_archive_types::Waypoint;

use crate::api;
use crate::format;
use crate::linked::RecordedTripMark;
use crate::waypoint::name_of;
use crate::waypoint_icon::WaypointSymbol;
use crate::Route;

use super::CollectionView;

/// The waypoints by name, ignoring case; those without one last, in file
/// order.
pub fn by_name(mut waypoints: Vec<Waypoint>) -> Vec<Waypoint> {
    waypoints.sort_by_cached_key(|waypoint| {
        (
            waypoint.name.is_none(),
            waypoint.name.as_deref().map(str::to_lowercase),
        )
    });
    waypoints
}

/// The list, from the waypoints as the page `read` them, of those `view`
/// keeps: waypoints that cannot be listed cost the list, not the page.
#[component]
pub(super) fn CollectionWaypoints(
    read: Result<Vec<Waypoint>, api::ApiError>,
    with_recorded_trip: Vec<i64>,
    view: CollectionView,
) -> Element {
    match read {
        Ok(waypoints) => {
            let waypoints = waypoints
                .into_iter()
                .filter(|waypoint| view.keeps(with_recorded_trip.contains(&waypoint.id)))
                .collect();
            rsx! { WaypointList { waypoints: by_name(waypoints), with_recorded_trip } }
        }
        Err(err) => {
            dioxus::logger::tracing::error!("could not list the waypoints: {err}");
            rsx! {}
        }
    }
}

/// One row per waypoint: its icon and name, linking to its page unless
/// `linked` is off — a share's recipient has no waypoint pages (US-94) —
/// marked when it is among `with_recorded_trip`, and its elevation. Nothing
/// for a collection without waypoints.
#[component]
pub fn WaypointList(
    waypoints: Vec<Waypoint>,
    #[props(default = true)] linked: bool,
    #[props(default)] with_recorded_trip: Vec<i64>,
) -> Element {
    if waypoints.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "table-scroll",
            table { id: "collection-waypoints",
                thead {
                    tr {
                        th { scope: "col", "Waypoint" }
                        th { scope: "col", class: "num", "Elevation" }
                    }
                }
                tbody {
                    for waypoint in waypoints {
                        tr { key: "{waypoint.id}",
                            td {
                                if linked {
                                    Link { to: Route::Waypoint { id: waypoint.id },
                                        WaypointSymbol { symbol: waypoint.symbol.clone() }
                                        "{name_of(&waypoint)}"
                                    }
                                } else {
                                    WaypointSymbol { symbol: waypoint.symbol.clone() }
                                    "{name_of(&waypoint)}"
                                }
                                if with_recorded_trip.contains(&waypoint.id) {
                                    RecordedTripMark {}
                                }
                            }
                            td { class: "num", {format::metres(waypoint.ele)} }
                        }
                    }
                }
            }
        }
    }
}
