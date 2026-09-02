//! Editing a trip from its detail screen: name and activity type (US-15),
//! and the linked Komoot tour's privacy (US-35).
//!
//! What the form asks the archive for is decided by [`changes`], a plain
//! function over the trip as loaded and the form as filled in — so the rule
//! that matters (only what actually changed is sent) is unit-tested rather
//! than inferred from a screen.

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, KomootLink, KomootPrivacy, TripDetail as Trip};

use crate::api::{self, ApiClient, TripEdit};

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
/// and nothing else (US-15). A privacy left on its placeholder asks for
/// nothing, which is what keeps an unmappable one from being pushed back to
/// Komoot as a choice (US-35, ADR-0021).
pub fn changes(trip: &Trip, form: &EditForm) -> TripEdit {
    let opened_with = EditForm::of(trip);
    TripEdit {
        name: (form.name != opened_with.name).then(|| form.name.clone()),
        activity_type: (form.activity != opened_with.activity).then(|| form.activity.clone()),
        privacy_status: (form.privacy != opened_with.privacy).then(|| form.privacy.clone()),
    }
}

/// Editing, folded away until the owner asks for it. Opening builds the form
/// afresh from the trip, so a cancelled edit leaves nothing behind.
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
        p {
            button {
                id: "edit-trip",
                r#type: "button",
                onclick: move |_| open.toggle(),
                if open() { "Cancel" } else { "Edit name / activity" }
            }
        }
        if open() {
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
                label {
                    "Activity "
                    select {
                        id: "edit-activity_type",
                        value: "{form.read().activity}",
                        oninput: move |event| form.write().activity = event.value(),
                        option { value: "", "{ActivityType::Unknown.label()}" }
                        for activity in ActivityType::SELECTABLE {
                            option {
                                key: "{activity}",
                                value: activity.as_str(),
                                "{activity.label()}"
                            }
                        }
                    }
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
                button { r#type: "submit", id: "edit-trip-save", "Save" }
            }
            if let Some(message) = error() {
                p { class: "error", "Could not save the changes: {message}" }
            }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;
    use trip_archive_types::{KomootLink, KomootPrivacy};

    fn a_trip(activity: ActivityType, komoot: Option<KomootLink>) -> Trip {
        Trip {
            id: 1,
            name: "Oslo Hills Walk".to_string(),
            activity_type: activity,
            tz_name: None,
            start_time: None,
            end_time: None,
            distance_m: 1000.0,
            ascent_m: None,
            descent_m: None,
            duration_secs: None,
            min_lat: None,
            min_lon: None,
            max_lat: None,
            max_lon: None,
            komoot,
        }
    }

    fn a_form(trip: &Trip) -> EditForm {
        EditForm::of(trip)
    }

    /// Just the privacy picker's own options. The form carries a second
    /// `<select>` — the activity one — with a blank option of its own, so a
    /// claim about "the blank option" has to say which picker it means.
    fn privacy_picker(html: &str) -> &str {
        let from = html
            .find(r#"id="edit-privacy_status""#)
            .expect("a privacy picker to look at");
        let rest = &html[from..];
        &rest[..rest.find("</select>").expect("a closed picker")]
    }

    /// The form as the trip opens it, rendered.
    fn rendered(trip: Trip) -> String {
        render(move || {
            rsx! {
                EditTripForm {
                    trip: trip.clone(),
                    on_saved: move |_| {},
                    on_cancel: move |_| {},
                }
            }
        })
    }

    #[test]
    fn a_form_nobody_touched_asks_for_no_change_at_all() {
        let trip = a_trip(ActivityType::Hiking, None);

        assert!(changes(&trip, &a_form(&trip)).is_empty());
    }

    #[test]
    fn only_the_fields_that_changed_are_asked_for() {
        // US-15's reason for a PATCH: an omitted field is left unchanged, so
        // renaming a trip cannot write back an activity type that a Komoot
        // sync altered after this screen loaded.
        let trip = a_trip(ActivityType::Hiking, None);
        let mut form = a_form(&trip);
        form.name = "Renamed Trip".to_string();

        let edit = changes(&trip, &form);

        assert_eq!(edit.name.as_deref(), Some("Renamed Trip"));
        assert_eq!(edit.activity_type, None);
        assert_eq!(edit.privacy_status, None);
    }

    #[test]
    fn clearing_the_activity_asks_for_the_unspecified_one() {
        // The picker's blank choice is `Unknown` on the wire — the same value
        // an import that chose no activity stores.
        let trip = a_trip(ActivityType::Hiking, None);
        let mut form = a_form(&trip);
        form.activity = String::new();

        assert_eq!(changes(&trip, &form).activity_type.as_deref(), Some(""));
    }

    #[test]
    fn a_privacy_left_as_the_archive_reported_it_is_never_pushed_back() {
        // US-35/ADR-0021: a privacy the archive could not map shows as
        // "Unknown" and must never be sent to Komoot as a choice. The picker
        // opens on a placeholder, and leaving it there asks for nothing.
        let trip = a_trip(
            ActivityType::Hiking,
            Some(KomootLink {
                tour_id: "111".to_string(),
                privacy: Some(KomootPrivacy::Unknown),
            }),
        );

        assert!(changes(&trip, &a_form(&trip)).is_empty());
    }

    #[test]
    fn a_chosen_privacy_is_asked_for() {
        let trip = a_trip(
            ActivityType::Hiking,
            Some(KomootLink {
                tour_id: "111".to_string(),
                privacy: Some(KomootPrivacy::Private),
            }),
        );
        let mut form = a_form(&trip);
        form.privacy = KomootPrivacy::Public.as_str().to_string();

        assert_eq!(
            changes(&trip, &form).privacy_status.as_deref(),
            Some("public")
        );
    }

    #[test]
    fn the_form_opens_on_the_trips_current_values() {
        let trip = a_trip(ActivityType::Hiking, None);

        let html = render(move || {
            rsx! {
                EditTripForm {
                    trip: trip.clone(),
                    on_saved: move |_| {},
                    on_cancel: move |_| {},
                }
            }
        });

        assert!(html.contains(r#"value="Oslo Hills Walk""#), "{html}");
        assert!(html.contains(ActivityType::Hiking.label()), "{html}");
        // Every activity the owner may choose, plus the unspecified one.
        assert!(html.contains(ActivityType::Cycling.label()), "{html}");
        assert!(html.contains(ActivityType::Unknown.label()), "{html}");
    }

    #[test]
    fn a_trip_that_never_came_from_komoot_is_offered_no_privacy() {
        let trip = a_trip(ActivityType::Hiking, None);

        let html = render(move || {
            rsx! {
                EditTripForm {
                    trip: trip.clone(),
                    on_saved: move |_| {},
                    on_cancel: move |_| {},
                }
            }
        });

        assert!(!html.contains("Komoot privacy"), "{html}");
    }

    #[test]
    fn a_privacy_the_archive_does_not_know_opens_on_a_placeholder() {
        // A `<select>` with no `selected` option shows its *first* one, so a
        // bare picker would assert "Private" for a trip whose privacy the
        // archive has no idea about — and, since only a changed value is
        // sent, would make Private the one value the owner then could not
        // choose. Both "no privacy read yet" and "Komoot reported something
        // unmappable" must therefore open on a placeholder.
        for privacy in [None, Some(KomootPrivacy::Unknown)] {
            let trip = a_trip(
                ActivityType::Hiking,
                Some(KomootLink {
                    tour_id: "111".to_string(),
                    privacy,
                }),
            );

            let html = rendered(trip);
            let picker = privacy_picker(&html);

            assert!(
                picker.contains(r#"<option value="">"#),
                "a placeholder must be offered for {privacy:?}: {picker}"
            );
            for settable in KomootPrivacy::SELECTABLE {
                assert!(
                    !picker.contains(&format!(r#"value="{}" selected"#, settable.as_str())),
                    "{settable} must not look chosen for {privacy:?}: {picker}"
                );
            }
        }
    }

    #[test]
    fn a_linked_trip_is_offered_the_settable_privacies() {
        let trip = a_trip(
            ActivityType::Hiking,
            Some(KomootLink {
                tour_id: "111".to_string(),
                privacy: Some(KomootPrivacy::Public),
            }),
        );

        let html = render(move || {
            rsx! {
                EditTripForm {
                    trip: trip.clone(),
                    on_saved: move |_| {},
                    on_cancel: move |_| {},
                }
            }
        });

        assert!(html.contains("Komoot privacy"), "{html}");
        let picker = privacy_picker(&html);
        assert!(picker.contains(KomootPrivacy::Public.label()), "{picker}");
        assert!(picker.contains(KomootPrivacy::Private.label()), "{picker}");
        // The placeholder only exists to avoid claiming a privacy that is not
        // known; it is not a value the owner can go back to.
        assert!(!picker.contains(r#"<option value="">"#), "{picker}");
        // Never offered: it is a state Komoot put the tour in, not a choice.
        assert!(
            !html.contains(&format!("value=\"{}\"", KomootPrivacy::Unknown.as_str())),
            "{html}"
        );
    }
}
