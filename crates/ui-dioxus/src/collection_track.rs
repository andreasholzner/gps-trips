//! A guide book's track (US-86): its map, elevation profile and waypoints,
//! drawn by the trip page's own views, and only the figures a guide book can
//! give — distance, ascent and descent. Its timestamps, where the file had
//! any, are not when anyone went, so no date, duration or speed is shown.
//! Its name and activity can be edited, an activity set here overriding the
//! collection's, and it can be deleted.

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, CollectionTrackDetail, CollectionTrackEdit};

use crate::activity_icon::ActivityIcon;
use crate::activity_select::{choices, ActivitySelect, Choice};
use crate::api::{self, ApiClient};
use crate::collection::drawn_waypoints;
use crate::detail::TrackViews;
use crate::format;
use crate::overlay::Overlay;
use crate::Route;

/// The screen. `id` comes from the route (`/collection-tracks/:id`).
#[component]
pub fn CollectionTrack(id: i64) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    // The id travels back with the answer, as on the trip page: a resource
    // keeps its previous value while the next track is being read.
    let mut track = use_resource(use_reactive!(|id| async move {
        (id, api::get_collection_track(&archive(), id).await)
    }));

    rsx! {
        match &*track.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            Some((read_for, _)) if *read_for != id => rsx! { p { "Loading…" } },
            Some((_, Err(err))) if err.is_not_found() => rsx! {
                p { class: "error", "There is no such track — it may have been deleted." }
            },
            Some((_, Err(err))) => rsx! {
                p { class: "error", "Could not load this track: {err}" }
            },
            Some((_, Ok(detail))) => rsx! {
                TrackFigures { track: detail.clone() }
                TrackMapSection { id, activity: detail.activity_type }
                div { class: "trip-actions",
                    EditCollectionTrack { track: detail.clone(), on_saved: move |_| track.restart() }
                    a {
                        class: "quiet",
                        href: api::collection_track_gpx_url(&archive(), id),
                        "Download GPX"
                    }
                    DeleteCollectionTrack { id, collection_id: detail.collection_id }
                }
            },
        }
    }
}

/// The track's name, its guide book, and its figures.
#[component]
fn TrackFigures(track: CollectionTrackDetail) -> Element {
    rsx! {
        hgroup {
            h1 { id: "trip-name",
                if track.activity_type != ActivityType::Unknown {
                    ActivityIcon { activity: track.activity_type }
                }
                "{track.name}"
            }
            p { id: "collection-of-track",
                "From the guide book "
                Link { to: Route::Collection { id: track.collection_id }, "{track.collection_name}" }
            }
        }
        dl { class: "stats",
            div {
                dt { "Activity" }
                dd { id: "trip-activity",
                    "{track.activity_type.label()}"
                    // An activity the track only takes from its collection
                    // changes with it; one of its own does not.
                    if track.own_activity.is_none() && track.activity_type != ActivityType::Unknown {
                        small { " (the guide book's)" }
                    }
                }
            }
            div {
                dt { "Distance" }
                dd { {format::km(track.distance_m)} }
            }
            div {
                dt { "Ascent" }
                dd { {format::metres(Some(track.ascent_m))} }
            }
            div {
                dt { "Descent" }
                dd { {format::metres(Some(track.descent_m))} }
            }
        }
    }
}

/// The map with the track's own waypoints, and the elevation profile — read
/// together, so the map is drawn once with both.
#[component]
fn TrackMapSection(id: i64, activity: ActivityType) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let geometry = use_resource(use_reactive!(|id| async move {
        let track = api::get_collection_track_geometry(&archive(), id).await?;
        let waypoints = drawn_waypoints(api::list_collection_track_waypoints(&archive(), id).await);
        Ok::<_, api::ApiError>((track, waypoints))
    }));

    rsx! {
        match &*geometry.read_unchecked() {
            None => rsx! { p { "Loading the track…" } },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load the track: {err}" } },
            Some(Ok((track, waypoints))) => rsx! {
                TrackViews {
                    track: track.clone(),
                    activity,
                    climbs: Vec::new(),
                    markers: Vec::new(),
                    waypoints: waypoints.clone(),
                    on_open_photos: |_| {},
                }
            },
        }
    }
}

/// The edit form's fields, as the inputs hold them.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackForm {
    pub name: String,
    /// The track's own activity's wire value, or empty for the guide book's.
    pub activity: String,
}

impl TrackForm {
    pub fn of(track: &CollectionTrackDetail) -> Self {
        Self {
            name: track.name.clone(),
            activity: track
                .own_activity
                .map_or_else(String::new, |activity| activity.as_str().to_string()),
        }
    }
}

/// What to ask the archive to change: only what the owner altered, as on a
/// trip's edit (US-15).
pub fn changes(track: &CollectionTrackDetail, form: &TrackForm) -> CollectionTrackEdit {
    let opened_with = TrackForm::of(track);
    let name = form.name.trim();
    CollectionTrackEdit {
        name: (name != opened_with.name).then(|| name.to_string()),
        activity_type: (form.activity != opened_with.activity).then(|| form.activity.clone()),
    }
}

/// Editing, folded away until asked for, over the screen as a trip's is.
#[component]
fn EditCollectionTrack(track: CollectionTrackDetail, on_saved: EventHandler<()>) -> Element {
    let mut open = use_signal(|| false);
    // An editor left open would carry over to the next one the router shows
    // through this same scope, aimed at its id with this one's values.
    let id = track.id;
    use_effect(use_reactive!(|id| {
        let _ = id;
        open.set(false);
    }));

    rsx! {
        button {
            id: "edit-trip",
            r#type: "button",
            class: "quiet",
            onclick: move |_| open.set(true),
            "Edit name / activity"
        }
        if open() {
            Overlay { label: "Edit track", on_close: move |_| open.set(false),
                div { class: "panel",
                    EditCollectionTrackForm {
                        track,
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
fn EditCollectionTrackForm(
    track: CollectionTrackDetail,
    on_saved: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut form = use_signal(|| TrackForm::of(&track));
    let mut error = use_signal(|| None::<String>);
    let id = track.id;

    rsx! {
        form {
            id: "edit-trip-form",
            onsubmit: move |event| {
                event.prevent_default();
                let edit = changes(&track, &form.read());
                async move {
                    if edit == CollectionTrackEdit::default() {
                        on_cancel.call(());
                        return;
                    }
                    match api::edit_collection_track(&archive(), id, &edit).await {
                        Ok(()) => on_saved.call(()),
                        Err(err) => error.set(Some(err.to_string())),
                    }
                }
            },
            p {
                label { r#for: "edit-name", "Name" }
                input {
                    id: "edit-name",
                    r#type: "text",
                    value: "{form.read().name}",
                    oninput: move |event| form.write().name = event.value(),
                }
            }
            ActivitySelect {
                id: "edit-activity",
                label: "Activity",
                value: form.read().activity.clone(),
                choices: choices(&[Choice::plain("", "The guide book's")]),
                onchange: move |value| form.write().activity = value,
            }
            button { r#type: "submit", "Save" }
            button { r#type: "button", onclick: move |_| on_cancel.call(()), "Cancel" }
            if let Some(message) = error() {
                p { class: "error", "{message}" }
            }
        }
    }
}

/// Deleting one track, after a confirmation; the screen goes back to its
/// guide book.
#[component]
fn DeleteCollectionTrack(id: i64, collection_id: i64) -> Element {
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
            p { class: "confirm",
                "Delete this track? Its waypoints stay with the guide book. This cannot be undone. "
                button {
                    r#type: "button",
                    class: "danger",
                    disabled: deleting(),
                    onclick: move |_| async move {
                        if deleting() {
                            return;
                        }
                        deleting.set(true);
                        match api::delete_collection_track(&archive(), id).await {
                            Ok(()) => {
                                navigator().replace(Route::Collection { id: collection_id });
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
                id: "delete-trip",
                r#type: "button",
                class: "quiet danger",
                onclick: move |_| {
                    error.set(None);
                    arming.set(true);
                },
                "Delete track"
            }
        }
        if let Some(message) = error() {
            p { class: "error", "Could not delete this track: {message}" }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
