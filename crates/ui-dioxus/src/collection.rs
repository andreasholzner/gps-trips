//! A collection page (US-86), where its import ends: the details, every
//! track in its activity's color and every waypoint on one map, the tracks
//! and the waypoints (US-98) as lists leading to their own pages, and the
//! files it came from to download (US-88). The details are edited and the
//! collection tagged (US-93) here, the collection or some of its tracks
//! shared (US-94), and the whole collection deleted. Its tracks and
//! waypoints with a recorded trip are counted and marked, and the lists
//! narrowed to them or to the rest (US-99).

use dioxus::prelude::*;
use trip_archive_types::{
    ActivityType, CollectionDetail, CollectionEdit, CollectionTrackSummary, TripTrack, Waypoint,
};

use crate::activity_color::{self, ActivityLegend};
use crate::activity_icon::ActivityIcon;
use crate::activity_select::{choices, ActivitySelect, Choice};
use crate::api::{self, ApiClient, Tagged};
use crate::filters::Filters;
use crate::interop::{self, MapWaypoint, OverviewEvent, OverviewLine};
use crate::linked::RecordedTripChoice;
use crate::overlay::Overlay;
use crate::share::{ShareButton, ShareTarget};
use crate::tagging::Tagging;
use crate::track;
use crate::Route;

mod files;
mod tracks;
mod view;
mod waypoints;
use files::CollectionFiles;
use tracks::Tracks;
pub(crate) use tracks::{track_route, TrackList};
pub use view::CollectionView;
use waypoints::CollectionWaypoints;
pub(crate) use waypoints::{by_name, WaypointList};

/// Where a collection's page is, its lists not narrowed.
pub fn collection_route(id: i64) -> Route {
    Route::Collection {
        id,
        view: CollectionView::default(),
    }
}

/// The screen. `id` comes from the route (`/collections/:id`), and what its
/// lists are narrowed to from its query (US-99).
#[component]
pub fn Collection(id: i64, #[props(default)] view: CollectionView) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    // The id travels back with the answer, as on the trip page.
    let mut collection = use_resource(use_reactive!(|id| async move {
        (id, api::get_collection(&archive(), id).await)
    }));
    // Read once for the map and the list, each drawn when they are read.
    let waypoints = use_resource(use_reactive!(|id| async move {
        (id, api::list_collection_waypoints(&archive(), id).await)
    }));
    let waypoints = waypoints.read_unchecked();
    let waypoints = match &*waypoints {
        Some((read_for, read)) if *read_for == id => Some(read.clone()),
        _ => None,
    };

    rsx! {
        match &*collection.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            Some((read_for, _)) if *read_for != id => rsx! { p { "Loading…" } },
            Some((_, Err(err))) if err.is_not_found() => rsx! {
                p { class: "error", "There is no such collection — it may have been deleted." }
            },
            Some((_, Err(err))) => rsx! {
                p { class: "error", "Could not load this collection: {err}" }
            },
            Some((_, Ok(detail))) => rsx! {
                CollectionHeader { collection: detail.clone() }
                Tagging { tagged: Tagged::Collection(id) }
                CollectionMap {
                    id,
                    tracks: detail.tracks.clone(),
                    waypoints: waypoints.clone().map(drawn_waypoints),
                }
                if !detail.tracks.is_empty() || detail.waypoint_count > 0 {
                    RecordedTripChoice {
                        id: "collection-recorded-trip",
                        value: view.recorded_trip,
                        onchange: move |recorded_trip| {
                            navigator().replace(Route::Collection { id, view: CollectionView { recorded_trip } });
                        },
                    }
                }
                Tracks { id, tracks: detail.tracks.clone(), view }
                if let Some(read) = &waypoints {
                    CollectionWaypoints {
                        read: read.clone(),
                        with_recorded_trip: detail.waypoints_with_recorded_trip.clone(),
                        view,
                    }
                }
                CollectionFiles { id }
                div { class: "trip-actions",
                    EditCollection { collection: detail.clone(), on_saved: move |_| collection.restart() }
                    ShareButton {
                        id: "share-collection",
                        target: ShareTarget::Collections(vec![id]),
                    }
                    DeleteCollection {
                        id,
                        tracks: detail.tracks.len(),
                        waypoints: detail.waypoint_count as usize,
                    }
                }
            },
        }
    }
}

/// How much a collection holds, joined by `and`: "3 tracks · 1 waypoint"
/// under its name, "3 tracks and 1 waypoint" in a sentence.
pub fn counts(tracks: usize, waypoints: usize, and: &str) -> String {
    let tally =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    format!(
        "{}{and}{}",
        tally(tracks, "track", "tracks"),
        tally(waypoints, "waypoint", "waypoints")
    )
}

/// How many of the collection's tracks have a recorded trip (US-99).
pub fn with_recorded_trip(collection: &CollectionDetail) -> String {
    let with = collection
        .tracks
        .iter()
        .filter(|track| track.has_recorded_trip)
        .count();
    format!(
        "{} with a recorded trip",
        crate::collection_table::with_recorded_trip(with as i64, collection.tracks.len() as i64)
    )
}

#[component]
fn CollectionHeader(collection: CollectionDetail) -> Element {
    rsx! {
        hgroup {
            h1 { id: "collection-name",
                if let Some(activity) = collection.activity_type {
                    ActivityIcon { activity }
                }
                "{collection.name}"
            }
            p { id: "collection-counts",
                {counts(collection.tracks.len(), collection.waypoint_count as usize, " · ")}
            }
            if !collection.tracks.is_empty() {
                p { id: "collection-recorded",
                    {with_recorded_trip(&collection)}
                }
            }
        }
        if !collection.description.is_empty() {
            p { id: "collection-description", class: "collection-description",
                "{collection.description}"
            }
        }
    }
}

/// One line per track whose positions were read, named after the track and
/// in its activity's color — shaded per track, as a share's are (US-75).
/// The colors are taken over every track, so one that could not be read
/// shifts no other.
pub fn collection_lines(
    tracks: &[CollectionTrackSummary],
    positions: &[TripTrack],
) -> Vec<OverviewLine> {
    tracks
        .iter()
        .zip(activity_color::in_list_order(
            tracks.iter().map(|track| track.activity_type),
        ))
        .filter_map(|(track, color)| {
            let read = positions.iter().find(|read| read.id == track.id)?;
            Some(OverviewLine {
                id: track.id,
                name: track.name.clone(),
                color,
                points: track::lat_lon(&read.coordinates),
            })
        })
        .collect()
}

/// Waypoints as a map draws them. Ones that could not be read cost their
/// markers, not the map: the lines, or the track and its profile, are
/// still worth drawing.
pub fn drawn_waypoints(read: Result<Vec<Waypoint>, api::ApiError>) -> Vec<MapWaypoint> {
    match read {
        Ok(waypoints) => waypoints.iter().map(MapWaypoint::from).collect(),
        Err(err) => {
            dioxus::logger::tracing::error!("could not read the waypoints: {err}");
            Vec::new()
        }
    }
}

/// Every track and `waypoints` on one map, drawn once the waypoints are
/// read; a click on a line opens its track.
#[component]
fn CollectionMap(
    id: i64,
    tracks: Vec<CollectionTrackSummary>,
    waypoints: Option<Vec<MapWaypoint>>,
) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut handle = use_signal(|| None::<document::Eval>);
    let _draw = use_resource(use_reactive!(|id, tracks, waypoints| async move {
        let Some(waypoints) = waypoints else {
            return;
        };
        // A map without lines or markers beats no map at all: the list
        // below still leads to every track.
        let positions = api::list_collection_tracks(&archive(), id)
            .await
            .unwrap_or_else(|err| {
                dioxus::logger::tracing::error!("could not read the tracks: {err}");
                Vec::new()
            });
        let mut map = interop::start_overview_map(
            collection_lines(&tracks, &positions),
            waypoints,
            Default::default(),
        );
        handle.set(Some(map));
        while let Ok(event) = map.recv::<OverviewEvent>().await {
            match event {
                OverviewEvent::Open(track) => {
                    navigator().push(Route::CollectionTrack { id: track });
                }
                OverviewEvent::Hover(track) => {
                    if let Some(map) = handle.read().as_ref() {
                        interop::highlight_on_overview_map(map, track);
                    }
                }
            }
        }
    }));

    let shown: Vec<ActivityType> = tracks.iter().map(|track| track.activity_type).collect();
    rsx! {
        div { id: "overview-map", class: "overview-map" }
        ActivityLegend { shown }
    }
}

/// The edit form's fields, as the inputs hold them.
#[derive(Clone, Debug, PartialEq)]
pub struct CollectionForm {
    pub name: String,
    pub description: String,
    /// An activity's wire value, or empty for none.
    pub activity: String,
}

impl CollectionForm {
    pub fn of(collection: &CollectionDetail) -> Self {
        Self {
            name: collection.name.clone(),
            description: collection.description.clone(),
            activity: collection
                .activity_type
                .map_or_else(String::new, |activity| activity.as_str().to_string()),
        }
    }
}

/// What to ask the archive to change: only what the owner altered.
pub fn changes(collection: &CollectionDetail, form: &CollectionForm) -> CollectionEdit {
    let opened_with = CollectionForm::of(collection);
    let name = form.name.trim();
    let description = form.description.trim();
    CollectionEdit {
        name: (name != opened_with.name).then(|| name.to_string()),
        description: (description != opened_with.description).then(|| description.to_string()),
        activity_type: (form.activity != opened_with.activity).then(|| form.activity.clone()),
    }
}

#[component]
fn EditCollection(collection: CollectionDetail, on_saved: EventHandler<()>) -> Element {
    let mut open = use_signal(|| false);
    // An editor left open would carry over to the next one the router shows
    // through this same scope, aimed at its id with this one's values.
    let id = collection.id;
    use_effect(use_reactive!(|id| {
        let _ = id;
        open.set(false);
    }));

    rsx! {
        button {
            id: "edit-collection",
            r#type: "button",
            class: "quiet",
            onclick: move |_| open.set(true),
            "Edit details"
        }
        if open() {
            Overlay { label: "Edit collection", on_close: move |_| open.set(false),
                div { class: "panel",
                    EditCollectionForm {
                        collection,
                        on_saved: move |_| {
                            open.set(false);
                            on_saved.call(());
                        },
                        on_cancel: move |_| open.set(false),
                    }
                }
            }
        }
    }
}

#[component]
fn EditCollectionForm(
    collection: CollectionDetail,
    on_saved: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let form = use_signal(|| CollectionForm::of(&collection));
    let mut error = use_signal(|| None::<String>);
    let id = collection.id;

    rsx! {
        form {
            id: "edit-collection-form",
            onsubmit: move |event| {
                event.prevent_default();
                let edit = changes(&collection, &form.read());
                async move {
                    if edit == CollectionEdit::default() {
                        on_cancel.call(());
                        return;
                    }
                    match api::edit_collection(&archive(), id, &edit).await {
                        Ok(()) => on_saved.call(()),
                        Err(err) => error.set(Some(err.to_string())),
                    }
                }
            },
            CollectionFields { form }
            button { r#type: "submit", "Save" }
            button { r#type: "button", onclick: move |_| on_cancel.call(()), "Cancel" }
            if let Some(message) = error() {
                p { class: "error", "{message}" }
            }
        }
    }
}

/// A collection's details as fields — the same at import (US-86) as when
/// they are edited here.
#[component]
pub fn CollectionFields(form: Signal<CollectionForm>) -> Element {
    let mut form = form;
    rsx! {
        p {
            label { r#for: "collection-name-field", "Name" }
            input {
                id: "collection-name-field",
                r#type: "text",
                value: "{form.read().name}",
                oninput: move |event| form.write().name = event.value(),
            }
        }
        p {
            label { r#for: "collection-description-field", "Description" }
            textarea {
                id: "collection-description-field",
                rows: "4",
                value: "{form.read().description}",
                oninput: move |event| form.write().description = event.value(),
            }
        }
        ActivitySelect {
            id: "collection-activity",
            label: "Activity",
            value: form.read().activity.clone(),
            choices: choices(&[Choice::plain("", "None")]),
            onchange: move |value| form.write().activity = value,
        }
    }
}

/// Deleting the whole collection, after a confirmation naming what goes
/// with it.
#[component]
fn DeleteCollection(id: i64, tracks: usize, waypoints: usize) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut arming = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    // The router shows the next one through this same scope; an armed
    // confirmation must not survive what it was armed on, as on a trip's.
    use_effect(use_reactive!(|id| {
        let _ = id;
        arming.set(false);
        error.set(None);
    }));

    rsx! {
        if arming() {
            ConfirmDeleteCollection {
                tracks,
                waypoints,
                busy: deleting(),
                on_confirm: move |_| async move {
                    if deleting() {
                        return;
                    }
                    deleting.set(true);
                    match api::delete_collection(&archive(), id).await {
                        Ok(()) => {
                            navigator().replace(Route::TripList { filters: Filters::default() });
                        }
                        Err(err) => {
                            arming.set(false);
                            error.set(Some(err.to_string()));
                        }
                    }
                    deleting.set(false);
                },
                on_cancel: move |_| arming.set(false),
            }
        } else {
            button {
                id: "delete-collection",
                r#type: "button",
                class: "quiet danger",
                onclick: move |_| {
                    error.set(None);
                    arming.set(true);
                },
                "Delete collection"
            }
        }
        if let Some(message) = error() {
            p { class: "error", "Could not delete this collection: {message}" }
        }
    }
}

#[component]
fn ConfirmDeleteCollection(
    tracks: usize,
    waypoints: usize,
    #[props(default)] busy: bool,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let held = counts(tracks, waypoints, " and ");
    rsx! {
        p { class: "confirm",
            "Delete this collection with its {held}? This cannot be undone. "
            button {
                r#type: "button",
                class: "danger",
                disabled: busy,
                onclick: move |_| on_confirm.call(()),
                "Delete it"
            }
            button { r#type: "button", onclick: move |_| on_cancel.call(()), "Cancel" }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
