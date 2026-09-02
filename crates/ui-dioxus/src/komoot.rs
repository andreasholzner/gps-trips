//! The Komoot "Sync now" review, in the SPA (US-44).
//!
//! One screen over two calls: the archive says what a run would do
//! (`api::list_sync_candidates`), the owner narrows it, and the button runs
//! it (`api::sync_now`). The narrowing is the point — a sync pulls whole
//! tours with their photos through the import pipeline, so which ones is a
//! decision, not a default (US-22). The historical bulk import has its own
//! binary (US-23) and does not go through here.
//!
//! A run is push-then-pull (ADR-0021), and the push half belongs to stories
//! with no screen of their own: US-20's queued edits and US-24's queued
//! deletes go out before the first tour is fetched, whether or not a tour is
//! ticked. So the screen says what is queued before the run and what was sent
//! after it, and neither number is this screen's own work.
//!
//! What is kept out of the component, so it can be checked without a browser:
//! [`selection_request`] decides what is asked for, [`run_report`] and
//! [`pending_note`] decide what is said, and [`refusal`] decides whether a
//! refusal reads as a failure or as a "not now" (US-26).

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::{SelectedTour, SyncCandidate, SyncCandidates, SyncRequest, SyncResponse};

use crate::api::{self, ApiClient, ApiError};
use crate::filters::Filters;
use crate::format;
use crate::Route;

/// What the ticked boxes ask for.
///
/// The **listing** decides, not the tick set: a halted run (US-25) leaves the
/// tours it did pull ticked but no longer on offer, and asking for those
/// again would import nothing and misreport what the retry was for. Each
/// selection carries the kind the screen already knew it was, so the pull
/// pages only the endpoint(s) it actually spans (US-29).
pub fn selection_request(candidates: &[SyncCandidate], selected: &BTreeSet<String>) -> SyncRequest {
    SyncRequest {
        tours: candidates
            .iter()
            .filter(|candidate| selected.contains(&candidate.tour_id))
            .map(|candidate| SelectedTour {
                tour_id: candidate.tour_id.clone(),
                kind: candidate.kind,
            })
            .collect(),
    }
}

/// What a finished run says, in one line.
///
/// Every phase that did something is reported, and a halt is added to that
/// rather than replacing it: the tours pulled before the failure are real,
/// and a line that only named the failure would hide them. The failing
/// trip/tour is named, which is US-25's acceptance criterion, and so is the
/// phase it belongs to — that is what tells the owner whether the archive
/// could not reach Komoot or the other way round.
pub fn run_report(result: &SyncResponse) -> String {
    let mut report = String::new();
    if result.pushed > 0 {
        report.push_str(&format!("Pushed {} edit(s). ", result.pushed));
    }
    if result.deleted > 0 {
        report.push_str(&format!("Deleted {} tour(s) on Komoot. ", result.deleted));
    }
    report.push_str(&format!("Synced {} tour(s).", result.imported));
    if let Some(tour_id) = &result.failed_tour {
        report.push_str(&format!(
            " Failed to {} tour {tour_id}: {}",
            // A run that reported a failure without saying where is a bug,
            // but the owner still gets the tour and the reason.
            result.failed_phase.map_or("pull", |phase| phase.verb()),
            result.failed_msg.as_deref().unwrap_or("unknown error"),
        ));
    }
    report
}

/// What the run will send before it pulls anything (US-20/US-24), or `None`
/// when the queues are empty and there is nothing to warn about.
///
/// The counts are the only place these two queues are ever visible: an edit
/// or a delete is queued by a screen that says nothing about Komoot, and
/// leaves no trace until a sync sends it.
pub fn pending_note(edits: i64, deletes: i64) -> Option<String> {
    let mut queued = Vec::new();
    if edits > 0 {
        queued.push(format!("{edits} edit(s)"));
    }
    if deletes > 0 {
        queued.push(format!("{deletes} deletion(s)"));
    }
    (!queued.is_empty()).then(|| {
        format!(
            "This run will also push {} to Komoot.",
            queued.join(" and ")
        )
    })
}

/// How a refused run reads.
///
/// A `409` is the archive declining to start a second sync while one is in
/// flight (US-26) — a "not now", and its own sentence already says what to
/// do. Anything else is a failure and is framed as one.
pub fn refusal(err: &ApiError) -> String {
    if err.is_conflict() {
        err.to_string()
    } else {
        format!("Could not sync with Komoot: {err}")
    }
}

/// The screen (US-44).
#[component]
pub fn KomootSync() -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    // Slow: the archive logs into Komoot and pages both listings to answer
    // it. Fetched on arrival and again after a run, never on a keystroke.
    let mut listing =
        use_resource(move || async move { api::list_sync_candidates(&archive()).await });
    let selected = use_signal(BTreeSet::<String>::new);
    let mut running = use_signal(|| false);
    let mut report = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);

    rsx! {
        nav { class: "elsewhere",
            Link { to: Route::TripList { filters: Filters::default() }, "← All trips" }
        }
        h1 { "Sync with Komoot" }

        match &*listing.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            // An archive that booted without Komoot credentials answers here,
            // in its own words — "cannot sync" must not read as "nothing to
            // sync", which an empty table would.
            Some(Err(err)) => rsx! {
                p { class: "error", "Could not ask Komoot what is new: {err}" }
            },
            Some(Ok(data)) => {
                let candidates = data.candidates.clone();
                rsx! {
                    SyncReview {
                        listing: data.clone(),
                        selected,
                        running: running(),
                        report: report(),
                        error: error(),
                        on_sync: move |_| {
                            let candidates = candidates.clone();
                            async move {
                                // One run at a time from this screen: a
                                // second would only earn a 409 (US-26)
                                // against the owner's own first run.
                                if running() {
                                    return;
                                }
                                running.set(true);
                                error.set(None);
                                report.set(None);
                                // Built before the first await, so no borrow
                                // of the tick set is held across it.
                                let request = selection_request(&candidates, &selected.read());
                                match api::sync_now(&archive(), &request).await {
                                    Ok(result) => {
                                        report.set(Some(run_report(&result)));
                                        // What was pulled is no longer on
                                        // offer and the queues have moved:
                                        // ask again rather than guess. What
                                        // stayed keeps its tick, so a halted
                                        // run is retried by pressing again.
                                        listing.restart();
                                    }
                                    Err(err) => error.set(Some(refusal(&err))),
                                }
                                running.set(false);
                            }
                        },
                    }
                }
            }
        }
    }
}

/// The review itself: what a run would do, and the button that does it.
///
/// The button is rendered even with nothing to pull — a run with no tours
/// ticked still pushes the queued edits and deletes, and that is the only way
/// to trigger them.
#[component]
fn SyncReview(
    listing: SyncCandidates,
    selected: Signal<BTreeSet<String>>,
    /// Whether a run is already on its way; the button says so and declines
    /// to start a second.
    #[props(default)]
    running: bool,
    #[props(default)] report: Option<String>,
    #[props(default)] error: Option<String>,
    on_sync: EventHandler<()>,
) -> Element {
    rsx! {
        if let Some(message) = report {
            p { strong { "{message}" } }
        }
        if let Some(message) = error {
            p { class: "error", "{message}" }
        }
        if let Some(note) = pending_note(listing.pending_edits, listing.pending_deletes) {
            p { "{note}" }
        }

        if listing.candidates.is_empty() {
            p { "No new tours to sync — everything on Komoot is already in the archive." }
        } else {
            SyncCandidateTable { candidates: listing.candidates.clone(), selected }
        }

        p {
            button {
                id: "komoot-sync-now",
                r#type: "button",
                disabled: running,
                onclick: move |_| on_sync.call(()),
                if running { "Syncing…" } else { "Sync now" }
            }
        }
    }
}

/// One row per tour Komoot has that the archive does not, each tickable, with
/// a select-all box for the common "pull everything new" case.
///
/// Nothing starts ticked (US-22): a sync pulls whole tours with their photos,
/// so it is opted into per tour rather than being what a stray press of the
/// button does.
///
/// Every column is read straight off Komoot's tour listing, so the screen
/// costs no per-tour call (`docs/komoot-api.md`). The kind is US-29's: it
/// says which of the list's tabs the imported trip will land on.
#[component]
fn SyncCandidateTable(
    candidates: Vec<SyncCandidate>,
    selected: Signal<BTreeSet<String>>,
) -> Element {
    let listed: Vec<String> = candidates
        .iter()
        .map(|candidate| candidate.tour_id.clone())
        .collect();
    let all_listed_selected =
        !listed.is_empty() && listed.iter().all(|id| selected.read().contains(id));

    rsx! {
        // Six columns don't fit a phone: the table scrolls inside this box so
        // the page itself never scrolls sideways (US-41).
        div { class: "table-scroll",
            table {
                thead {
                    tr {
                        th {
                            input {
                                r#type: "checkbox",
                                checked: all_listed_selected,
                                onchange: move |event: FormEvent| {
                                    let mut selected = selected.write();
                                    for id in &listed {
                                        if event.checked() {
                                            selected.insert(id.clone());
                                        } else {
                                            selected.remove(id);
                                        }
                                    }
                                },
                            }
                        }
                        th { "Tour" }
                        th { "Kind" }
                        th { "Activity" }
                        th { "Date" }
                        th { "Distance" }
                    }
                }
                tbody {
                    for candidate in candidates {
                        tr { key: "{candidate.tour_id}",
                            td {
                                input {
                                    r#type: "checkbox",
                                    checked: selected.read().contains(&candidate.tour_id),
                                    onchange: {
                                        let tour_id = candidate.tour_id.clone();
                                        move |event: FormEvent| {
                                            if event.checked() {
                                                selected.write().insert(tour_id.clone());
                                            } else {
                                                selected.write().remove(&tour_id);
                                            }
                                        }
                                    },
                                }
                            }
                            td { "{candidate.name}" }
                            td { "{candidate.kind.label()}" }
                            // Komoot's own sport name, unmapped: the archive's
                            // activity type is decided by the import, and
                            // showing a guess here would be a second answer.
                            td { "{candidate.sport}" }
                            td { {format::date(Some(&candidate.date))} }
                            td { {format::km(candidate.distance_m)} }
                        }
                    }
                }
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
