//! Editing a trip from its detail screen: name and activity type (US-15),
//! and the linked Komoot tour's privacy (US-35).
//!
//! What the form asks the archive for is decided by [`changes`], a plain
//! function over the trip as loaded and the form as filled in — so the rule
//! that matters (only what actually changed is sent) is unit-tested rather
//! than inferred from a screen.

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, KomootLink, KomootPrivacy, TripDetail as Trip};

use crate::activity_select::{choices, ActivitySelect, Choice};
use crate::api::{self, ApiClient, TripEdit};
use crate::overlay::Overlay;
use crate::suggested::{MapCredits, Suggested};

/// The form's fields, as strings, exactly as the inputs hold them.
#[derive(Clone, Debug, PartialEq)]
pub struct EditForm {
    pub name: String,
    /// An activity's wire value, or empty for "unspecified" (`Unknown`).
    pub activity: String,
    /// A settable privacy's wire value, or empty — which means "whatever the
    /// archive already has", including a privacy it could not map.
    pub privacy: String,
}

impl EditForm {
    /// The form as the trip fills it in when it opens.
    pub fn of(trip: &Trip) -> Self {
        Self {
            name: trip.name.clone(),
            activity: activity_value(trip.activity_type).to_string(),
            privacy: privacy_value(trip.komoot.as_ref()).to_string(),
        }
    }
}

/// An activity's value in the picker: `Unknown` is the blank choice, the same
/// as an import that named no activity.
fn activity_value(activity: ActivityType) -> &'static str {
    if activity == ActivityType::Unknown {
        ""
    } else {
        activity.as_str()
    }
}

/// A linked tour's privacy in the picker, or blank.
///
/// Blank for a privacy that is not one the owner may choose — not read from
/// Komoot yet, or read as something the archive could not map. Without that
/// placeholder the picker would show its first option and silently claim a
/// privacy the archive does not have; and since only a changed value is sent,
/// that claim would also make the shown value the one privacy the owner could
/// not then pick.
fn privacy_value(link: Option<&KomootLink>) -> &'static str {
    match link.and_then(|link| link.privacy) {
        Some(privacy) if KomootPrivacy::SELECTABLE.contains(&privacy) => privacy.as_str(),
        _ => "",
    }
}

/// What to ask the archive to change: every field the owner actually altered,
/// and nothing else (US-15). A name goes without the space around it — a
/// suggestion of the date alone ends in one (US-74). A privacy left on its
/// placeholder asks for nothing, which is what keeps an unmappable one from
/// being pushed back to Komoot as a choice (US-35, ADR-0021).
pub fn changes(trip: &Trip, form: &EditForm) -> TripEdit {
    let opened_with = EditForm::of(trip);
    let name = form.name.trim();
    TripEdit {
        name: (name != opened_with.name).then(|| name.to_string()),
        activity_type: (form.activity != opened_with.activity).then(|| form.activity.clone()),
        privacy_status: (form.privacy != opened_with.privacy).then(|| form.privacy.clone()),
    }
}

/// Editing, folded away until the owner asks for it. Opening builds the form
/// afresh from the trip, so a cancelled edit leaves nothing behind.
///
/// The button sits in the quiet row at the foot of the screen (US-62), a long
/// scroll from the name and activity it changes, so the form opens over the
/// screen rather than beside either.
#[component]
pub fn EditTrip(trip: Trip, on_saved: EventHandler<()>) -> Element {
    let mut open = use_signal(|| false);
    // The router shows the next trip through this same scope, so an editor
    // left open would carry over to it — with the previous trip's typed
    // values in it, aimed at the new trip's id. Changing trip closes it.
    let id = trip.id;
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
            Overlay { label: "Edit trip", on_close: move |_| open.set(false),
                div { class: "panel",
                    EditTripForm {
                        trip,
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

/// The form itself. Mounted only while it is open, so its fields start from
/// the trip every time — and so this component can be rendered on its own,
/// which is how the rules below are tested without a browser.
#[component]
fn EditTripForm(trip: Trip, on_saved: EventHandler<()>, on_cancel: EventHandler<()>) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut form = use_signal(|| EditForm::of(&trip));
    let mut error = use_signal(|| None::<String>);
    let id = trip.id;
    let komoot = trip.komoot.clone();
    // US-74: what the archive suggests for the trip as stored, asked for as
    // the form opens. Only offered: a failure just offers nothing.
    // US-76: and the activity type the track looks like.
    // Asked again for another trip, should the form be shown for one.
    let suggestion = use_resource(use_reactive!(|id| async move {
        api::trip_suggestion(&archive(), id).await.ok()
    }));
    let suggested = suggestion.read().clone().flatten();
    // Neither is offered when it is what the field already says.
    let offered_name = suggested
        .as_ref()
        .map(|s| s.name.clone())
        .filter(|name| name.trim() != form.read().name.trim());
    let offered_activity = suggested
        .and_then(|s| s.activity_type)
        .filter(|activity| activity_value(*activity) != form.read().activity);
    // Belt and braces with `EditTrip`'s own reset above: the fields are the
    // trip's, so they follow the trip if this form is ever mounted across a
    // change of one.
    let subject = trip.clone();
    use_effect(use_reactive!(|subject| {
        form.set(EditForm::of(&subject));
        error.set(None);
    }));

    rsx! {
            form {
                id: "edit-trip-form",
                onsubmit: move |event| {
                    let trip = trip.clone();
                    async move {
                        event.prevent_default();
                        let edit = changes(&trip, &form.read());
                        // Nothing changed: there is nothing to ask the
                        // archive for, and nothing to re-read afterwards.
                        if edit.is_empty() {
                            on_cancel.call(());
                            return;
                        }
                        match api::edit_trip(&archive(), id, &edit).await {
                            Ok(()) => {
                                error.set(None);
                                on_saved.call(());
                            }
                            Err(err) => error.set(Some(err.to_string())),
                        }
                    }
                },
                label {
                    "Name "
                    input {
                        id: "edit-name",
                        value: "{form.read().name}",
                        oninput: move |event| form.write().name = event.value(),
                    }
                }
                if let Some(name) = offered_name.clone() {
                    Suggested {
                        id: "edit-name-suggestion",
                        what: "name",
                        text: name.clone(),
                        on_use: move |_| form.write().name = name.clone(),
                    }
                }
                ActivitySelect {
                    id: "edit-activity_type",
                    label: "Activity",
                    value: form.read().activity.clone(),
                    choices: choices(&[Choice::unspecified("")]),
                    onchange: move |value| form.write().activity = value,
                }
                if let Some(activity) = offered_activity {
                    Suggested {
                        id: "edit-activity-suggestion",
                        what: "activity",
                        text: activity.label(),
                        on_use: move |_| form.write().activity = activity.as_str().to_string(),
                    }
                }
                // Once, under whatever is suggested.
                if offered_name.is_some() || offered_activity.is_some() {
                    MapCredits {}
                }
                // US-35: privacy belongs to the linked Komoot tour, so a trip
                // that never came from Komoot is offered none — the archive
                // rejects such an edit for the same reason.
                if komoot.is_some() {
                    label {
                        "Komoot privacy "
                        select {
                            id: "edit-privacy_status",
                            value: "{form.read().privacy}",
                            oninput: move |event| form.write().privacy = event.value(),
                            // Shown, and selected, only while the archive has
                            // no privacy the owner could have chosen.
                            if form.read().privacy.is_empty() {
                                option { value: "", "{KomootPrivacy::Unknown.label()}" }
                            }
                            for privacy in KomootPrivacy::SELECTABLE {
                                option {
                                    key: "{privacy}",
                                    value: privacy.as_str(),
                                    "{privacy.label()}"
                                }
                            }
                        }
                    }
                }
                div { class: "form-actions",
                    button { r#type: "submit", id: "edit-trip-save", "Save" }
                    button {
                        r#type: "button",
                        id: "edit-trip-cancel",
                        class: "quiet",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                }
            }
            if let Some(message) = error() {
                p { class: "error", "Could not save the changes: {message}" }
            }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
