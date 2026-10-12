//! A collection's waypoints below its tracks (US-98), each leading to its
//! own page, where it is corrected.

use dioxus::prelude::*;
use trip_archive_types::Waypoint;

use crate::api::{self, ApiClient};
use crate::format;
use crate::waypoint::name_of;
use crate::waypoint_icon::WaypointSymbol;
use crate::Route;

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

/// The list, read on its own as the files are: waypoints that cannot be
/// listed cost the list, not the page.
#[component]
pub(super) fn CollectionWaypoints(id: i64) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let waypoints = use_resource(use_reactive!(|id| async move {
        api::list_collection_waypoints(&archive(), id).await
    }));

    match &*waypoints.read_unchecked() {
        Some(Ok(waypoints)) => rsx! { WaypointList { waypoints: by_name(waypoints.clone()) } },
        Some(Err(err)) => {
            dioxus::logger::tracing::error!("could not list the waypoints: {err}");
            rsx! {}
        }
        None => rsx! {},
    }
}

/// One row per waypoint: its icon and name, linking to its page, and its
/// elevation. Nothing for a collection without waypoints.
#[component]
pub fn WaypointList(waypoints: Vec<Waypoint>) -> Element {
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
                                Link { to: Route::Waypoint { id: waypoint.id },
                                    WaypointSymbol { symbol: waypoint.symbol.clone() }
                                    "{name_of(&waypoint)}"
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
