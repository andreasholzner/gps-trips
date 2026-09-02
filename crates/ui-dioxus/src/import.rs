//! Importing a trip from the SPA (US-43), as the two-step flow US-12 asks
//! for: choose the GPX, then confirm a name the archive has already
//! suggested — with the track's date in it, which is only knowable once the
//! file has been read.
//!
//! Step one uploads the GPX alone and gets back that suggestion
//! (`api::stage_gpx`); nothing exists in the archive yet, so walking away
//! here costs nothing. Step two confirms, which creates the trip
//! (`api::confirm_import`), and only then do the photos go up — in batches,
//! so a large import has something to watch (ADR-0004's 2026-09-01
//! amendment).
//!
//! The rules worth checking without a browser are kept out of the component:
//! [`ConfirmForm`] decides what the fields start as and what is sent, and
//! [`batches`] decides how the photos are split. What is left in the handlers
//! is sequencing.

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, ConfirmImport, StagedImport, TripKind};

use crate::api::{self, ApiClient, ApiError, PhotoUpload};
use crate::filters::Filters;
use crate::Route;

/// Photos per request. Small enough that the count moves often on a big
/// import, large enough that a hundred photos are not a hundred round trips.
const MAX_BATCH_FILES: usize = 8;

/// Bytes per request, whichever limit is reached first — eight phone photos
/// are a few megabytes, eight camera raws are not, and it is the bytes that
/// decide how long a request takes. Well under the archive's own body cap
/// (`config::server::PHOTO_IMPORT_BODY_LIMIT`), which a single batch must
/// never approach.
const MAX_BATCH_BYTES: usize = 16 * 1024 * 1024;

/// Split the chosen photos into the requests that will carry them.
///
/// The owner chose them in one dialog and sees one progress line; this is
/// only how that upload is spent. A photo bigger than the byte limit travels
/// alone rather than being refused — the limit shapes batches, it does not
/// reject files.
pub fn batches(photos: Vec<PhotoUpload>) -> Vec<Vec<PhotoUpload>> {
    let mut batches = Vec::new();
    let mut batch: Vec<PhotoUpload> = Vec::new();
    let mut bytes = 0;

    for photo in photos {
        let size = photo.bytes.len();
        let full = batch.len() >= MAX_BATCH_FILES || bytes + size > MAX_BATCH_BYTES;
        if full && !batch.is_empty() {
            batches.push(std::mem::take(&mut batch));
            bytes = 0;
        }
        bytes += size;
        batch.push(photo);
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    batches
}

/// The confirm step's fields, as strings, exactly as the inputs hold them —
/// the same shape `edit::EditForm` uses for the same reason.
#[derive(Clone, Debug, PartialEq)]
pub struct ConfirmForm {
    pub name: String,
    /// An activity's wire value, or empty for "unspecified" (`Unknown`).
    pub activity: String,
    /// A kind's wire value; never empty, because the radios always have one
    /// chosen (US-31).
    pub kind: String,
    /// An IANA name, or empty to accept whatever the archive guessed.
    pub timezone: String,
}

impl ConfirmForm {
    /// The form as the archive's suggestion fills it in.
    ///
    /// The name is US-12's whole point: it arrives with the track's date
    /// already in it, so the owner types after the prefix instead of looking
    /// one up. Activity is left unspecified — the archive has no opinion to
    /// offer yet — and kind starts on Recorded, the same default the import
    /// has always applied (US-31).
    pub fn of(staged: &StagedImport) -> Self {
        Self {
            name: staged.suggested_name.clone(),
            activity: String::new(),
            kind: TripKind::Recorded.as_str().to_string(),
            timezone: staged.timezone.clone(),
        }
    }

    /// What to send. A name left blank — or left as nothing but the
    /// suggested prefix's trailing space — asks the archive for its own
    /// fallback rather than storing whitespace; every other field means what
    /// the single-step import always took it to mean, blank included.
    pub fn to_confirm(&self) -> ConfirmImport {
        let name = self.name.trim();
        ConfirmImport {
            name: (!name.is_empty()).then(|| name.to_string()),
            activity_type: Some(self.activity.clone()),
            kind: Some(self.kind.clone()),
            timezone: Some(self.timezone.clone()),
        }
    }
}

/// How far the photos have got, once the trip itself exists.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Progress {
    done: usize,
    total: usize,
}

/// A trip that was created, whose photos did not all follow it there.
///
/// Reported rather than swallowed, and never as a failed import: the trip is
/// real and the remaining photos are one screen away (US-2), which is the
/// bargain ADR-0004's amendment struck for being able to show progress at
/// all.
#[derive(Clone, Debug, PartialEq)]
struct PartialImport {
    trip_id: i64,
    uploaded: usize,
    total: usize,
    error: String,
}

/// The screen (US-43). Two steps in one component: which one shows is
/// whether the archive has a parse waiting.
#[component]
pub fn ImportTrip() -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    // The chosen file, kept after staging: the archive holds these bytes too,
    // but its copy expires, and re-sending them silently beats asking the
    // owner to find the file again.
    let mut gpx = use_signal(|| None::<(String, Vec<u8>)>);
    let mut staged = use_signal(|| None::<StagedImport>);
    let mut reading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut progress = use_signal(|| None::<Progress>);
    let mut partial = use_signal(|| None::<PartialImport>);
    // One confirmation at a time, set before the first `await`.
    //
    // The hazard it closes is in this file's own logic rather than in any
    // path reachable today: `staged` stays set across the confirm request, so
    // two overlapping confirmations would both post the same parse — and the
    // second, answered 404 because the first spent it, would be taken for an
    // expired upload by `confirm_or_restage` and re-imported as a *second
    // trip*. Nothing produces two of them at present (the button disables
    // itself, and neither a double click nor two synchronous `requestSubmit`
    // calls reach the handler twice), which is precisely why this is written
    // down rather than left to hold by accident.
    let mut submitting = use_signal(|| false);

    // This step shows only while nothing is parked, so there is never a
    // previous parse to hand back here — `start_over` below is what does
    // that, and what makes it reachable at all.
    let choose = move |event: FormEvent| async move {
        let Some(file) = event.files().into_iter().next() else {
            return;
        };
        reading.set(true);
        error.set(None);
        partial.set(None);
        let name = file.name();
        let bytes = match file.read_bytes().await {
            Ok(bytes) => bytes.to_vec(),
            Err(err) => {
                reading.set(false);
                error.set(Some(format!("Could not read {name}: {err}")));
                return;
            }
        };
        match api::stage_gpx(&archive(), &name, bytes.clone()).await {
            Ok(suggestion) => {
                gpx.set(Some((name, bytes)));
                staged.set(Some(suggestion));
            }
            // The archive read the file and would not take it (US-1): its
            // own words, and the owner stays on the picker.
            Err(err) => {
                gpx.set(None);
                error.set(Some(format!("Could not read that GPX file: {err}")));
            }
        }
        reading.set(false);
    };

    let confirm = move |(form, photos): (ConfirmForm, Vec<PhotoUpload>)| async move {
        if submitting() {
            return;
        }
        let Some(current) = staged() else { return };
        submitting.set(true);
        error.set(None);
        partial.set(None);

        let outcome = confirm_or_restage(&archive(), &current, &form.to_confirm(), gpx()).await;
        // Whichever parse is live now — the one we came in with, a re-staged
        // one, or none at all — is what the screen holds from here.
        staged.set(outcome.parked);
        let id = match outcome.result {
            Ok(id) => id,
            Err(err) => {
                error.set(Some(format!("Could not import this trip: {err}")));
                submitting.set(false);
                return;
            }
        };

        let total = photos.len();
        let mut uploaded = 0;
        progress.set(Some(Progress { done: 0, total }));
        for batch in batches(photos) {
            let sending = batch.len();
            if let Err(err) = api::add_photos(&archive(), id, batch).await {
                progress.set(None);
                partial.set(Some(PartialImport {
                    trip_id: id,
                    uploaded,
                    total,
                    error: err.to_string(),
                }));
                submitting.set(false);
                return;
            }
            uploaded += sending;
            progress.set(Some(Progress {
                done: uploaded,
                total,
            }));
        }
        progress.set(None);
        submitting.set(false);
        navigator().push(Route::TripDetail { id });
    };

    // The only way the archive is ever told about an upload the owner thought
    // better of: picking a different file is reachable from step one alone,
    // which shows only while nothing is parked.
    let start_over = move |_| async move {
        if let Some(current) = staged.take() {
            let _ = api::cancel_staged_import(&archive(), current.staging_id).await;
        }
        gpx.set(None);
        error.set(None);
    };

    rsx! {
        nav { class: "elsewhere",
            Link { to: Route::TripList { filters: Filters::default() }, "← All trips" }
        }
        h1 { "Import a trip" }

        if let Some(progress) = progress() {
            UploadProgress { progress }
        } else if let Some(partial) = partial() {
            PartialOutcome { partial }
        } else {
            match staged() {
                None => rsx! {
                    ChooseGpx { reading: reading(), error: error(), on_choose: choose }
                },
                Some(suggestion) => rsx! {
                    ConfirmImportStep {
                        staged: suggestion,
                        error: error(),
                        submitting: submitting(),
                        on_confirm: confirm,
                        on_start_over: start_over,
                    }
                },
            }
        }
    }
}

/// What a confirmation left behind: the new trip, or why there isn't one,
/// and which parked parse the screen should be holding afterwards.
///
/// The handle can change under the screen, so it is returned rather than
/// assumed. Holding a spent one would make the owner's next try 404, re-send
/// the whole file again, and leave another `import_staging` row behind —
/// once per corrected field.
struct Confirmation {
    result: Result<i64, ApiError>,
    /// The parse still waiting to be confirmed: the one the screen came in
    /// with when a field was refused, the re-staged one when the original had
    /// expired and the retry was refused too, and `None` both when a trip was
    /// made and when there is nothing left to retry with.
    parked: Option<StagedImport>,
}

/// Confirm the parse the screen has, re-staging first if the archive no
/// longer has it.
///
/// A handle goes stale two ways the owner did nothing to cause: the sweeper
/// took it after a long pause on the form, or a restart lost it. Both answer
/// 404, and both are recoverable without them, because the bytes are still
/// in hand — so this re-sends the file and confirms against the new parse
/// rather than reporting a failure the owner can only fix by picking the
/// same file again.
async fn confirm_or_restage(
    archive: &ApiClient,
    staged: &StagedImport,
    confirm: &ConfirmImport,
    gpx: Option<(String, Vec<u8>)>,
) -> Confirmation {
    match api::confirm_import(archive, staged.staging_id, confirm).await {
        Ok(id) => Confirmation {
            result: Ok(id),
            parked: None,
        },
        // Refused on its merits: the parse is untouched and still the one to
        // retry against once the owner has fixed the field.
        Err(err) if !err.is_not_found() => Confirmation {
            result: Err(err),
            parked: Some(staged.clone()),
        },
        // Gone: swept after a long pause on the form, or lost to a restart.
        Err(_) => match gpx {
            None => Confirmation {
                result: Err(ApiError::new(
                    "that upload has expired — choose the file again",
                )),
                parked: None,
            },
            Some((name, bytes)) => restage_and_confirm(archive, &name, bytes, confirm).await,
        },
    }
}

/// Send the file again and confirm against the parse that makes, keeping
/// whichever handle is live afterwards.
async fn restage_and_confirm(
    archive: &ApiClient,
    name: &str,
    bytes: Vec<u8>,
    confirm: &ConfirmImport,
) -> Confirmation {
    let restaged = match api::stage_gpx(archive, name, bytes).await {
        Ok(restaged) => restaged,
        Err(err) => {
            return Confirmation {
                result: Err(err),
                parked: None,
            }
        }
    };
    match api::confirm_import(archive, restaged.staging_id, confirm).await {
        Ok(id) => Confirmation {
            result: Ok(id),
            parked: None,
        },
        Err(err) => Confirmation {
            result: Err(err),
            parked: Some(restaged),
        },
    }
}

/// Step one: the file, and nothing else to fill in yet.
#[component]
fn ChooseGpx(
    reading: bool,
    #[props(default)] error: Option<String>,
    on_choose: EventHandler<FormEvent>,
) -> Element {
    rsx! {
        p { "Choose the GPX file to import. You can name the trip in the next step." }
        input {
            id: "import-gpx",
            r#type: "file",
            accept: ".gpx,application/gpx+xml",
            disabled: reading,
            onchange: move |event| on_choose.call(event),
        }
        if reading {
            p { "Reading the track…" }
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
fn ConfirmImportStep(
    staged: StagedImport,
    #[props(default)] error: Option<String>,
    /// Whether a confirmation is already on its way; the button says so and
    /// declines to start a second.
    #[props(default)]
    submitting: bool,
    on_confirm: EventHandler<(ConfirmForm, Vec<PhotoUpload>)>,
    on_start_over: EventHandler<()>,
) -> Element {
    let mut form = use_signal(|| ConfirmForm::of(&staged));
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

            p {
                label { r#for: "import-activity", "Activity" }
                select {
                    id: "import-activity",
                    value: "{form.read().activity}",
                    oninput: move |event| form.write().activity = event.value(),
                    option { value: "", {ActivityType::Unknown.label()} }
                    for activity in ActivityType::SELECTABLE {
                        option { value: activity.as_str(), {activity.label()} }
                    }
                }
            }

            fieldset {
                legend { "Trip kind" }
                for kind in TripKind::ALL {
                    label {
                        input {
                            r#type: "radio",
                            name: "kind",
                            value: kind.as_str(),
                            checked: form.read().kind == kind.as_str(),
                            oninput: move |_| form.write().kind = kind.as_str().to_string(),
                        }
                        {kind.label()}
                    }
                }
            }

            p {
                label { r#for: "import-timezone", "Photo timezone" }
                input {
                    id: "import-timezone",
                    r#type: "text",
                    value: "{form.read().timezone}",
                    oninput: move |event| form.write().timezone = event.value(),
                }
                small { "Guessed from where the track starts; used to place photos by time." }
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
fn UploadProgress(progress: Progress) -> Element {
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
fn PartialOutcome(partial: PartialImport) -> Element {
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

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
