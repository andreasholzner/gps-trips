//! Moving a waypoint (US-98) as a photo is placed (US-30): over the screen,
//! the line of the track it belongs to as a guide, where it is now, and a
//! tap to say where it goes instead, saved explicitly. No warning comes
//! first: no position of a waypoint is automatic.

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, WaypointDetail, WaypointEdit};

use crate::activity_color;
use crate::api::{self, ApiClient};
use crate::placing::PlaceOnMap;
use crate::track;

use super::name_of;

/// The button, and the placing map once it is pressed.
#[component]
pub(super) fn MoveWaypoint(detail: WaypointDetail, on_saved: EventHandler<()>) -> Element {
    let mut open = use_signal(|| false);
    // A placement left open would carry over to the next waypoint the router
    // shows through this same scope.
    let id = detail.waypoint.id;
    use_effect(use_reactive!(|id| {
        let _ = id;
        open.set(false);
    }));

    rsx! {
        button {
            id: "move-waypoint",
            r#type: "button",
            class: "quiet",
            onclick: move |_| open.set(true),
            "Move waypoint"
        }
        if open() {
            PlaceWaypoint {
                detail,
                on_placed: move |_| {
                    open.set(false);
                    on_saved.call(());
                },
                on_close: move |_| open.set(false),
            }
        }
    }
}

/// The placing map over the screen; the track is read only once it opens.
#[component]
fn PlaceWaypoint(
    detail: WaypointDetail,
    on_placed: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let track_id = detail.track.as_ref().map(|track| track.id);
    let guide = use_resource(move || async move {
        match track_id {
            Some(track) => api::get_collection_track_geometry(&archive(), track)
                .await
                .map(|geometry| track::polyline(&geometry))
                .map_err(|err| err.to_string()),
            None => Ok(Vec::new()),
        }
    });
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let waypoint = &detail.waypoint;
    let id = waypoint.id;
    let activity = detail
        .track
        .as_ref()
        .map_or(ActivityType::Unknown, |track| track.activity_type);

    rsx! {
        PlaceOnMap {
            title: "Move {name_of(waypoint)}",
            hint: "Tap the map where the waypoint goes.",
            guide: guide.read_unchecked().clone(),
            color: activity_color::color(activity),
            current: Some([waypoint.lat, waypoint.lon]),
            error: error().map(|message| format!("Could not move the waypoint: {message}")),
            saving: saving(),
            on_save: move |[lat, lon]: [f64; 2]| async move {
                // One request per press, as for a photo.
                if saving() {
                    return;
                }
                saving.set(true);
                error.set(None);
                let edit = WaypointEdit {
                    lat: Some(lat),
                    lon: Some(lon),
                    ..Default::default()
                };
                match api::edit_waypoint(&archive(), id, &edit).await {
                    Ok(()) => on_placed.call(()),
                    Err(err) => error.set(Some(err.to_string())),
                }
                saving.set(false);
            },
            on_close,
        }
    }
}
