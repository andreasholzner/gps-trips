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
//! A guide book's file takes the third choice beside Recorded and Planned,
//! *Collection* (US-86, US-88): step one then asks the archive what the file
//! holds, and step two creates a collection rather than a trip. The choice
//! is made before the file is read, because a book of waypoints alone is no
//! trip the archive could stage. Several files, or files for an existing
//! collection, are parked and reviewed instead (US-88, `review`).
//!
//! The rules worth checking without a browser are kept out of the component:
//! [`ConfirmForm`] decides what the fields start as and what is sent, and
//! [`batches`] decides how the photos are split. What is left in the handlers
//! is sequencing.

use dioxus::prelude::*;
use trip_archive_types::{CollectionImportReview, ConfirmImport, StagedImport, TripKind};

use crate::api::{self, ApiClient, ApiError, PhotoUpload};
use crate::collection::CollectionForm;
use crate::photos::{upload_in_batches, PartialUpload};
use crate::Route;

mod collection;
mod review;
mod steps;

use collection::{ChosenBook, ConfirmGuideBookStep, COLLECTION};
use review::ReviewFlow;
use steps::{ChooseGpx, ConfirmImportStep, PartialOutcome, UploadProgress};

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
    /// A kind's wire value, as chosen with the file (US-31).
    pub kind: String,
}

impl ConfirmForm {
    /// The form as the archive's suggestion fills it in.
    ///
    /// The name is US-12's whole point: it arrives with the track's date
    /// already in it, so the owner types after the prefix instead of looking
    /// one up. The activity starts on the one the track looks like (US-76),
    /// or unspecified when the archive cannot tell; the kind is the one
    /// chosen with the file.
    pub fn of(staged: &StagedImport, kind: &str) -> Self {
        Self {
            name: staged.suggested_name.clone(),
            activity: staged
                .suggested_activity
                .map_or_else(String::new, |activity| activity.as_str().to_string()),
            kind: kind.to_string(),
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
        }
    }
}

/// Which way chosen files go.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Path {
    /// One file, read as a trip or as US-86's new collection.
    Single,
    /// Parked and reviewed first (US-88).
    Review,
}

/// Several files, or files for an existing collection, are reviewed; any
/// other file goes the single way; a picker closed without one, nowhere.
fn path_of(kind: &str, files: usize, target: Option<i64>) -> Option<Path> {
    match files {
        0 => None,
        _ if kind == COLLECTION && (files > 1 || target.is_some()) => Some(Path::Review),
        _ => Some(Path::Single),
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
    // A trip kind's wire value, or `COLLECTION` (US-86): Recorded until the
    // owner says otherwise, the default the import has always had (US-31).
    let mut kind = use_signal(|| TripKind::Recorded.as_str().to_string());
    let mut book = use_signal(|| None::<ChosenBook>);
    // US-88: the collection the files are added to, `None` for a new one,
    // and the upload parked for review.
    let mut target = use_signal(|| None::<i64>);
    let mut upload = use_signal(|| None::<CollectionImportReview>);
    let collections = use_resource(move || async move {
        if kind() != COLLECTION {
            return Vec::new();
        }
        match api::list_collections(&archive(), String::new()).await {
            Ok(listed) => listed.into_iter().map(|c| (c.id, c.name)).collect(),
            Err(err) => {
                dioxus::logger::tracing::error!("could not list the collections: {err}");
                Vec::new()
            }
        }
    });
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
        let Some(path) = path_of(&kind(), event.files().len(), target()) else {
            return;
        };
        // US-88: several files, or files for an existing collection, are
        // parked and reviewed; a single one for a new collection is US-86's.
        if path == Path::Review {
            reading.set(true);
            error.set(None);
            let mut files = Vec::new();
            for file in event.files() {
                match file.read_bytes().await {
                    Ok(bytes) => files.push((file.name(), bytes.to_vec())),
                    Err(err) => {
                        reading.set(false);
                        error.set(Some(format!("Could not read {}: {err}", file.name())));
                        return;
                    }
                }
            }
            match api::upload_collection_files(&archive(), files, target()).await {
                Ok(review) => upload.set(Some(review)),
                Err(err) => error.set(Some(format!("Could not read those GPX files: {err}"))),
            }
            reading.set(false);
            return;
        }
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
        if kind() == COLLECTION {
            match api::preview_collection(&archive(), &name, bytes.clone()).await {
                Ok(preview) => book.set(Some(ChosenBook {
                    file_name: name,
                    bytes,
                    preview,
                })),
                Err(err) => error.set(Some(format!("Could not read that GPX file: {err}"))),
            }
            reading.set(false);
            return;
        }
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
        progress.set(Some(Progress { done: 0, total }));
        let uploaded = upload_in_batches(&archive(), id, photos, |done| {
            progress.set(Some(Progress { done, total }));
        })
        .await;
        progress.set(None);
        if let Err(PartialUpload { uploaded, error }) = uploaded {
            partial.set(Some(PartialImport {
                trip_id: id,
                uploaded,
                total,
                error: error.to_string(),
            }));
            submitting.set(false);
            return;
        }
        submitting.set(false);
        navigator().push(Route::TripDetail { id });
    };

    // A guide book is created in one request (US-86): nothing was parked, so
    // there is no parse to confirm or to lose.
    let confirm_book = move |form: CollectionForm| async move {
        if submitting() {
            return;
        }
        let Some(chosen) = book() else { return };
        submitting.set(true);
        error.set(None);
        let created = api::create_collection(
            &archive(),
            &chosen.file_name,
            chosen.bytes,
            form.name.trim(),
            form.description.trim(),
            &form.activity,
        )
        .await;
        submitting.set(false);
        match created {
            Ok(id) => {
                navigator().push(Route::Collection { id });
            }
            Err(err) => error.set(Some(format!("Could not import this collection: {err}"))),
        }
    };

    // The only way the archive is ever told about an upload the owner thought
    // better of: picking a different file is reachable from step one alone,
    // which shows only while nothing is parked.
    let start_over = move |_| async move {
        if let Some(current) = staged.take() {
            let _ = api::cancel_staged_import(&archive(), current.staging_id).await;
        }
        gpx.set(None);
        book.set(None);
        error.set(None);
    };

    rsx! {
        h1 { "Import a trip" }

        if let Some(review) = upload() {
            ReviewFlow {
                key: "{review.staging_id}",
                initial: review,
                on_leave: move |message| {
                    upload.set(None);
                    error.set(message);
                },
            }
        // Only the preview: the file itself is megabytes for a whole book,
        // and this runs on every keystroke in the form.
        } else if let Some(preview) = book.read().as_ref().map(|chosen| chosen.preview.clone()) {
            ConfirmGuideBookStep {
                preview,
                error: error(),
                submitting: submitting(),
                on_confirm: confirm_book,
                on_start_over: start_over,
            }
        } else if let Some(progress) = progress() {
            UploadProgress { progress }
        } else if let Some(partial) = partial() {
            PartialOutcome { partial }
        } else {
            match staged() {
                None => rsx! {
                    ChooseGpx {
                        reading: reading(),
                        error: error(),
                        kind: kind(),
                        on_kind: move |chosen| kind.set(chosen),
                        collections: collections().unwrap_or_default(),
                        target: target(),
                        on_target: move |chosen| target.set(chosen),
                        on_choose: choose,
                    }
                },
                Some(suggestion) => rsx! {
                    ConfirmImportStep {
                        staged: suggestion,
                        kind: kind(),
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

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
