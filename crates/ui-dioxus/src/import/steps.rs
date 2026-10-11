//! The import screen's steps (US-43), as components of their own so each
//! can be rendered — and asserted on — without a server behind it.

use dioxus::prelude::*;
use trip_archive_types::{StagedImport, TripKind};

use super::guide_book::GUIDE_BOOK;
use super::{ConfirmForm, PartialImport, Progress};
use crate::activity_select::{choices, ActivitySelect, Choice};
use crate::api::PhotoUpload;
use crate::suggested::MapCredits;
use crate::Route;

/// Step one: what the file is — a trip recorded or planned (US-31), or a
/// guide book (US-86) — and the file itself.
#[component]
pub(super) fn ChooseGpx(
    reading: bool,
    #[props(default)] error: Option<String>,
    #[props(default = TripKind::Recorded.as_str().to_string())] kind: String,
    #[props(default)] on_kind: EventHandler<String>,
    on_choose: EventHandler<FormEvent>,
) -> Element {
    let options = TripKind::ALL
        .iter()
        .map(|kind| (kind.as_str(), kind.label()))
        .chain([(GUIDE_BOOK, "Guide book")]);
    rsx! {
        fieldset {
            legend { "Import as" }
            for (value, label) in options {
                label {
                    input {
                        r#type: "radio",
                        name: "kind",
                        value,
                        checked: kind == value,
                        disabled: reading,
                        oninput: move |_| on_kind.call(value.to_string()),
                    }
                    {label}
                }
            }
        }
        p { "Choose the GPX file to import. You can name it in the next step." }
        input {
            id: "import-gpx",
            r#type: "file",
            accept: ".gpx,application/gpx+xml",
            disabled: reading,
            onchange: move |event| on_choose.call(event),
        }
        if reading {
            p { "Reading the file…" }
        }
        if let Some(error) = error {
            p { class: "error", "{error}" }
        }
    }
}

/// Step two: the name the archive suggested, and everything else the trip is
/// stored with.
///
/// A component of its own so it can be rendered — and asserted on — with a
/// suggestion in hand and no server behind it, the way `edit::EditTripForm`
/// is.
#[component]
pub(super) fn ConfirmImportStep(
    staged: StagedImport,
    /// The trip kind chosen with the file (US-31).
    #[props(default = TripKind::Recorded.as_str().to_string())]
    kind: String,
    #[props(default)] error: Option<String>,
    /// Whether a confirmation is already on its way; the button says so and
    /// declines to start a second.
    #[props(default)]
    submitting: bool,
    on_confirm: EventHandler<(ConfirmForm, Vec<PhotoUpload>)>,
    on_start_over: EventHandler<()>,
) -> Element {
    let mut form = use_signal(|| ConfirmForm::of(&staged, &kind));
    let mut photos = use_signal(Vec::<PhotoUpload>::new);
    let mut reading_photos = use_signal(|| false);
    let mut photo_error = use_signal(|| None::<String>);

    rsx! {
        form {
            id: "confirm-import",
            onsubmit: move |event| async move {
                event.prevent_default();
                on_confirm.call((form.read().clone(), photos.read().clone()));
            },

            p {
                label { r#for: "import-name", "Trip name" }
                input {
                    id: "import-name",
                    r#type: "text",
                    value: "{form.read().name}",
                    oninput: move |event| form.write().name = event.value(),
                }
            }

            div {
                ActivitySelect {
                    id: "import-activity",
                    label: "Activity",
                    value: form.read().activity.clone(),
                    choices: choices(&[Choice::unspecified("")]),
                    onchange: move |value| form.write().activity = value,
                }
                // US-74/US-76: the name and the activity may come from the
                // places and the ways the track passes.
                MapCredits {}
            }

            p {
                label { r#for: "import-photos", "Photos (optional)" }
                input {
                    id: "import-photos",
                    r#type: "file",
                    accept: "image/*",
                    multiple: true,
                    // Read here rather than at submit, so the upload starts
                    // the moment the owner asks for it.
                    onchange: move |event: FormEvent| async move {
                        // Whatever was staged belongs to the previous
                        // selection; it must not survive a pick that then
                        // fails to read (the rule `photos::AddPhotos`
                        // already follows).
                        photos.take();
                        photo_error.set(None);
                        reading_photos.set(true);
                        let mut chosen = Vec::new();
                        for file in event.files() {
                            match file.read_bytes().await {
                                Ok(bytes) => chosen.push(PhotoUpload {
                                    file_name: file.name(),
                                    content_type: file.content_type(),
                                    bytes: bytes.to_vec(),
                                }),
                                Err(err) => {
                                    photo_error
                                        .set(Some(format!("Could not read {}: {err}", file.name())));
                                    reading_photos.set(false);
                                    return;
                                }
                            }
                        }
                        photos.set(chosen);
                        reading_photos.set(false);
                    },
                }
                if !photos.read().is_empty() {
                    small { "{photos.read().len()} photo(s) will be uploaded after the trip is created." }
                }
            }

            button {
                id: "import-confirm",
                r#type: "submit",
                disabled: reading_photos() || submitting,
                if submitting { "Importing…" } else { "Import" }
            }
            button {
                id: "import-start-over",
                r#type: "button",
                disabled: submitting,
                onclick: move |_| on_start_over.call(()),
                "Choose a different file"
            }
        }

        if let Some(message) = photo_error() {
            p { class: "error", "{message}" }
        }
        if let Some(error) = error {
            p { class: "error", "{error}" }
        }
    }
}

/// The photos going up, once the trip itself is safely stored.
#[component]
pub(super) fn UploadProgress(progress: Progress) -> Element {
    rsx! {
        p { "Trip created. Uploading photos…" }
        progress {
            id: "import-progress",
            value: "{progress.done}",
            max: "{progress.total}",
        }
        p { "{progress.done} of {progress.total} photos uploaded." }
    }
}

/// A trip that arrived without all of its photos.
#[component]
pub(super) fn PartialOutcome(partial: PartialImport) -> Element {
    rsx! {
        p {
            "The trip was created, but only {partial.uploaded} of {partial.total} photos "
            "were uploaded: {partial.error}"
        }
        p { "You can add the rest from the trip itself." }
        Link {
            id: "partial-import-trip",
            to: Route::TripDetail { id: partial.trip_id },
            "Open the trip"
        }
    }
}
