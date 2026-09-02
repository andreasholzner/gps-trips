//! What the sync screen decides, checked where it can be checked without a
//! browser (ADR-0012's 2026-08-26a amendment): the rules in plain functions,
//! and the review rendered with a listing in hand.
//!
//! **No browser layer for this screen, deliberately.** The Playwright suite
//! runs the real binary against a throwaway data directory, with no Komoot
//! credentials and no way to point the client at a stand-in — so the screen
//! there can only ever render "not configured". Covering the ticking and the
//! button in a browser would mean adding a production config seam that
//! exists for tests alone, which is a worse trade than the gap. The events
//! themselves are the ordinary checkbox and button wiring the trip list
//! already exercises in that layer (US-34).
//!
//! These are the assertions that let the server-rendered review page go
//! (ADR-0012's migration rule); the dedup and the pull itself are the API's,
//! and stay in `tests/us22_sync_candidates_api.rs` and
//! `tests/us25_sync_halts_on_failure.rs`.

use std::sync::Arc;

use super::*;
use crate::test_support::{render, render_against_archive, serve_test_archive_with_komoot};
use trip_archive::server::komoot::testing::{a_tour, MockKomootClient};
use trip_archive_types::{SyncPhase, TripKind};

fn a_candidate(tour_id: &str, name: &str, kind: TripKind) -> SyncCandidate {
    SyncCandidate {
        tour_id: tour_id.to_string(),
        name: name.to_string(),
        sport: "hike".to_string(),
        date: "2026-07-11T08:47:52.000Z".to_string(),
        distance_m: 12_345.0,
        kind,
    }
}

fn on_offer(candidates: Vec<SyncCandidate>) -> SyncCandidates {
    SyncCandidates {
        candidates,
        pending_edits: 0,
        pending_deletes: 0,
    }
}

fn ticked(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

// ── What a selection asks for ────────────────────────────────────────────

#[test]
fn a_selection_asks_for_the_tours_it_ticked_carrying_their_kind() {
    // US-29: the kind travels with the id, so the pull lists only the
    // endpoint(s) the selection actually spans instead of always paging both.
    let candidates = vec![
        a_candidate("111", "Fjord Loop", TripKind::Recorded),
        a_candidate("222", "Ridge Traverse", TripKind::Planned),
    ];

    let request = selection_request(&candidates, &ticked(&["222"]));

    assert_eq!(
        request.tours,
        vec![SelectedTour {
            tour_id: "222".to_string(),
            kind: TripKind::Planned,
        }]
    );
}

#[test]
fn a_selection_only_ever_asks_for_tours_still_on_offer() {
    // What makes retrying a halted run safe: the tours that were pulled
    // before the halt are gone from the refreshed listing, but their ids are
    // still ticked. Asking for them again would import nothing (they are
    // linked now) and would misreport what the run was for — so the listing,
    // not the tick set, decides.
    let candidates = vec![a_candidate("222", "Ridge Traverse", TripKind::Recorded)];

    let request = selection_request(&candidates, &ticked(&["111", "222"]));

    assert_eq!(request.tours.len(), 1);
    assert_eq!(request.tours[0].tour_id, "222");
}

#[test]
fn nothing_ticked_still_makes_a_run() {
    // A sync with no tour ticked is not a no-op: the push phases go first
    // and send the pending edits (US-20) and deletes (US-24) regardless.
    let candidates = vec![a_candidate("111", "Fjord Loop", TripKind::Recorded)];

    let request = selection_request(&candidates, &BTreeSet::new());

    assert!(request.tours.is_empty());
}

// ── What a finished run says ─────────────────────────────────────────────

fn a_run() -> SyncResponse {
    SyncResponse::default()
}

#[test]
fn a_clean_run_reports_what_it_pulled() {
    let report = run_report(&SyncResponse {
        imported: 3,
        ..a_run()
    });

    assert!(report.contains("Synced 3 tour(s)"), "{report}");
}

#[test]
fn a_run_that_pushed_first_reports_that_half_too() {
    // US-20 and US-24 need no screen of their own, but their work happens
    // inside this run and the owner is owed the outcome.
    let report = run_report(&SyncResponse {
        pushed: 2,
        deleted: 1,
        imported: 0,
        ..a_run()
    });

    assert!(report.contains("Pushed 2 edit(s)"), "{report}");
    assert!(report.contains("Deleted 1 tour(s) on Komoot"), "{report}");
    assert!(report.contains("Synced 0 tour(s)"), "{report}");
}

#[test]
fn a_push_that_sent_nothing_is_not_mentioned() {
    // Nothing was pending, so saying "Pushed 0 edit(s)" would be noise.
    let report = run_report(&SyncResponse {
        imported: 1,
        ..a_run()
    });

    assert!(!report.contains("Pushed"), "{report}");
    assert!(!report.contains("Deleted"), "{report}");
}

#[test]
fn a_halted_pull_names_the_tour_that_stopped_it() {
    // US-25's acceptance criterion, on the screen: "a visible error names the
    // specific trip/tour that failed".
    let report = run_report(&SyncResponse {
        imported: 1,
        failed_tour: Some("222".to_string()),
        failed_msg: Some("komoot returned 500".to_string()),
        failed_phase: Some(SyncPhase::Pull),
        ..a_run()
    });

    assert!(report.contains("222"), "the tour must be named: {report}");
    assert!(report.contains("komoot returned 500"), "{report}");
    assert!(report.contains("pull"), "{report}");
    // The work done before the halt is still reported: the run is not
    // all-or-nothing, and the owner needs both halves.
    assert!(report.contains("Synced 1 tour(s)"), "{report}");
}

#[test]
fn a_halted_push_says_push_rather_than_pull() {
    // Which side failed is what tells the owner whether their archive could
    // not reach Komoot or the other way round (US-25).
    let report = run_report(&SyncResponse {
        failed_tour: Some("111".to_string()),
        failed_msg: Some("delete tour: komoot returned 500".to_string()),
        failed_phase: Some(SyncPhase::Push),
        ..a_run()
    });

    assert!(report.contains("push"), "{report}");
    assert!(!report.contains("pull"), "{report}");
}

#[test]
fn a_failure_with_no_words_still_says_something() {
    let report = run_report(&SyncResponse {
        failed_tour: Some("111".to_string()),
        failed_phase: Some(SyncPhase::Pull),
        ..a_run()
    });

    assert!(report.contains("111"), "{report}");
    assert!(report.contains("unknown error"), "{report}");
}

// ── What the run will also push ──────────────────────────────────────────

#[test]
fn a_quiet_queue_is_not_mentioned_at_all() {
    assert_eq!(pending_note(0, 0), None);
}

#[test]
fn a_pending_edit_is_named_before_the_run_not_after() {
    // The PoC page said this, and it is the only place US-20's queue is ever
    // visible: without it the owner cannot tell a run that will push from one
    // that will not.
    let note = pending_note(2, 0).expect("a note");

    assert!(note.contains("2 edit(s)"), "{note}");
    assert!(!note.contains("deletion"), "{note}");
}

#[test]
fn a_pending_delete_is_named_alongside_it() {
    // US-24's queue, which the PoC page never showed at all.
    let note = pending_note(2, 1).expect("a note");

    assert!(note.contains("2 edit(s)"), "{note}");
    assert!(note.contains("1 deletion(s)"), "{note}");
}

// ── The review, as it renders ────────────────────────────────────────────

fn review(candidates: SyncCandidates) -> String {
    render(move || {
        rsx! {
            SyncReview {
                listing: candidates.clone(),
                selected: Signal::new(BTreeSet::new()),
                on_sync: move |_| {},
            }
        }
    })
}

#[test]
fn every_candidate_is_listed_with_its_kind() {
    // US-29: the owner must be able to tell a planned route from a recorded
    // tour before pulling it — the assertion that moves off the server's
    // `render_sync_candidates`.
    let html = review(on_offer(vec![
        a_candidate("111", "Fjord Loop", TripKind::Recorded),
        a_candidate("222", "Ridge Traverse", TripKind::Planned),
    ]));

    assert!(html.contains("Fjord Loop"), "{html}");
    assert!(html.contains("Ridge Traverse"), "{html}");
    assert!(html.contains("Recorded"), "{html}");
    assert!(html.contains("Planned"), "{html}");
    // US-22: enough of the tour to decide by, without a per-tour Komoot call.
    assert!(html.contains("2026-07-11"), "{html}");
    assert!(html.contains("12.35 km"), "{html}");
}

#[test]
fn nothing_is_ticked_until_the_owner_ticks_it() {
    // US-22, as built: the owner opts in per tour, rather than a stray press
    // of the button pulling in everything new at once. A full pull is one
    // click on the select-all box, and the historical bulk import is US-23's
    // own binary.
    let html = review(on_offer(vec![
        a_candidate("111", "Fjord Loop", TripKind::Recorded),
        a_candidate("222", "Ridge Traverse", TripKind::Planned),
    ]));

    assert_eq!(
        html.matches("checked").count(),
        0,
        "no box starts ticked: {html}"
    );
}

#[test]
fn a_caught_up_archive_says_so_and_still_offers_the_run() {
    // Nothing to pull does not mean nothing to do: the push phases still run,
    // so the button has to be there (the PoC page's own rule).
    let html = review(on_offer(vec![]));

    assert!(html.contains("already in the archive"), "{html}");
    assert!(html.contains(r#"id="komoot-sync-now""#), "{html}");
}

#[test]
fn the_review_says_what_the_run_will_also_push() {
    let html = review(SyncCandidates {
        candidates: vec![],
        pending_edits: 2,
        pending_deletes: 1,
    });

    assert!(html.contains("2 edit(s)"), "{html}");
    assert!(html.contains("1 deletion(s)"), "{html}");
}

#[test]
fn a_run_in_flight_declines_to_start_another() {
    // A sync takes as long as Komoot takes; a second press would only earn a
    // 409 (US-26) against the owner's own first run.
    let listing = on_offer(vec![a_candidate("111", "Fjord Loop", TripKind::Recorded)]);
    let html = render(move || {
        rsx! {
            SyncReview {
                listing: listing.clone(),
                selected: Signal::new(BTreeSet::new()),
                running: true,
                on_sync: move |_| {},
            }
        }
    });

    assert!(html.contains("disabled"), "{html}");
}

// ── The screen, against a real archive ───────────────────────────────────

fn a_mock() -> Arc<MockKomootClient> {
    Arc::new(MockKomootClient {
        tours: vec![a_tour("111", "Fjord Loop", "hike")],
        planned_tours: vec![a_tour("222", "Ridge Traverse", "touringbicycle")],
        ..Default::default()
    })
}

#[tokio::test]
async fn the_screen_offers_what_komoot_has_that_the_archive_does_not() {
    let (base_url, _state, _dir) = serve_test_archive_with_komoot(a_mock()).await;

    let html = render_against_archive(
        &base_url,
        || rsx! { KomootSync {} },
        |html| html.contains("Fjord Loop"),
    )
    .await;

    // Both listings, each labeled — the same facts the page rendered, now
    // fetched over the API (US-22/US-29).
    assert!(html.contains("Ridge Traverse"), "{html}");
    assert!(html.contains("Planned"), "{html}");
}

#[tokio::test]
async fn an_archive_without_komoot_credentials_says_so_rather_than_looking_empty() {
    // `main.rs` treats Komoot as optional, so "cannot sync" must not read as
    // "nothing to sync".
    let (base_url, _dir) = crate::test_support::serve_test_archive().await;

    let html = render_against_archive(
        &base_url,
        || rsx! { KomootSync {} },
        |html| html.contains("KOMOOT_EMAIL"),
    )
    .await;

    assert!(!html.contains("already in the archive"), "{html}");
}

#[tokio::test]
async fn a_run_refused_because_one_is_already_going_reads_as_try_again() {
    // US-26 through the real router: the archive answers 409, and the screen
    // has to say "not now" rather than reporting the owner's sync as broken.
    let (base_url, state, _dir) = serve_test_archive_with_komoot(a_mock()).await;
    state.set_sync_in_progress_for_test(true);

    let err = api::sync_now(&base_url, &SyncRequest::default())
        .await
        .expect_err("a second sync must be refused");

    assert!(err.is_conflict(), "{err}");
    let message = refusal(&err);
    assert!(message.contains("try again shortly"), "{message}");
    assert!(
        !message.contains("Could not"),
        "a 409 is a not-now, not a failure: {message}"
    );
}

#[tokio::test]
async fn a_run_that_could_not_be_made_at_all_is_reported_as_a_failure() {
    // The other side of the branch above: a refusal that is not a 409 reads
    // as what it is.
    let (base_url, _dir) = crate::test_support::serve_test_archive().await;

    let err = api::sync_now(&base_url, &SyncRequest::default())
        .await
        .expect_err("an archive without credentials cannot sync");

    assert!(!err.is_conflict(), "{err}");
    assert!(refusal(&err).contains("Could not sync"), "{err}");
}
