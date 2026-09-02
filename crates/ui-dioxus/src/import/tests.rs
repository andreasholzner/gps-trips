//! What the import screen decides, checked where it can be checked without a
//! browser (ADR-0012's 2026-08-26a amendment): the rules in plain functions,
//! and the confirm step rendered with a suggestion in hand.
//!
//! What is *not* here, deliberately: choosing a file. `dioxus-ssr` dispatches
//! no events, so the prefilled name — US-12's acceptance criterion, which is
//! a reaction to the picker — is asserted in the browser layer instead
//! (`tests/browser/import.spec.mjs`), and so is the upload itself.

use super::*;
use crate::test_support::{render, serve_test_archive};

fn a_suggestion(name: &str) -> StagedImport {
    StagedImport {
        staging_id: 7,
        suggested_name: name.to_string(),
        start_date: Some("2024-06-01".to_string()),
        gpx_name: Some("Oslo Hills Walk".to_string()),
        timezone: "Europe/Oslo".to_string(),
        distance_m: 1234.0,
        ascent_m: 40.0,
        duration_secs: Some(3600),
    }
}

fn a_photo(name: &str, size: usize) -> PhotoUpload {
    PhotoUpload {
        file_name: name.to_string(),
        content_type: Some("image/jpeg".to_string()),
        bytes: vec![0; size],
    }
}

// ── What the form starts as, and what it sends ───────────────────────────

#[test]
fn the_name_starts_as_the_archives_suggestion() {
    // US-12: the date is already in the field, so the owner types after it.
    let form = ConfirmForm::of(&a_suggestion("2024-06-01 Oslo Hills Walk"));

    assert_eq!(form.name, "2024-06-01 Oslo Hills Walk");
    assert_eq!(form.timezone, "Europe/Oslo");
    // US-31's default, and US-11's "not said yet".
    assert_eq!(form.kind, "recorded");
    assert_eq!(form.activity, "");
}

#[test]
fn a_bare_date_prefix_is_sent_without_its_trailing_space() {
    // The owner accepted the suggestion for a track with no name of its own.
    // Storing "2024-06-01 " would put the prefix's spacing in the archive.
    let form = ConfirmForm::of(&a_suggestion("2024-06-01 "));

    assert_eq!(form.to_confirm().name.as_deref(), Some("2024-06-01"));
}

#[test]
fn a_name_wiped_out_asks_the_archive_for_its_own_fallback() {
    // Not an empty name: `resolve_name`'s precedence — the GPX track's name,
    // then a date-prefixed default — is what should decide, exactly as it
    // did for the single-step form.
    let mut form = ConfirmForm::of(&a_suggestion("2024-06-01 Oslo Hills Walk"));
    form.name = "   ".to_string();

    assert_eq!(form.to_confirm().name, None);
}

#[test]
fn the_owners_answers_are_what_travel() {
    let mut form = ConfirmForm::of(&a_suggestion("2024-06-01 "));
    form.name = "2024-06-01 Nordmarka".to_string();
    form.activity = "hiking".to_string();
    form.kind = "planned".to_string();
    form.timezone = "Europe/Berlin".to_string();

    let confirm = form.to_confirm();
    assert_eq!(confirm.name.as_deref(), Some("2024-06-01 Nordmarka"));
    assert_eq!(confirm.activity_type.as_deref(), Some("hiking"));
    assert_eq!(confirm.kind.as_deref(), Some("planned"));
    assert_eq!(confirm.timezone.as_deref(), Some("Europe/Berlin"));
}

// ── How the photos are spent ─────────────────────────────────────────────

#[test]
fn photos_are_split_by_count_so_the_owner_sees_the_number_move() {
    let photos: Vec<_> = (0..20).map(|i| a_photo(&format!("{i}.jpg"), 1)).collect();

    let batches = batches(photos);

    assert_eq!(batches.len(), 3, "20 photos, 8 to a request");
    assert_eq!(batches.iter().map(Vec::len).sum::<usize>(), 20);
}

#[test]
fn photos_are_split_by_size_too_because_bytes_are_what_take_time() {
    let photos = vec![
        a_photo("a.jpg", 10 * 1024 * 1024),
        a_photo("b.jpg", 10 * 1024 * 1024),
    ];

    let batches = batches(photos);

    assert_eq!(batches.len(), 2, "20 MB does not travel as one request");
}

#[test]
fn one_photo_larger_than_a_batch_travels_alone_rather_than_not_at_all() {
    // The limit shapes batches; it does not refuse files. Splitting one photo
    // is not possible, so it goes on its own.
    let photos = vec![
        a_photo("huge.raw", MAX_BATCH_BYTES * 2),
        a_photo("b.jpg", 1),
    ];

    let batches = batches(photos);

    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0].len(), 1);
    assert_eq!(batches[0][0].file_name, "huge.raw");
}

#[test]
fn no_photos_means_no_requests() {
    assert!(batches(Vec::new()).is_empty());
}

// ── The confirm step as it renders ───────────────────────────────────────

#[test]
fn the_confirm_step_offers_every_field_the_trip_is_stored_with() {
    let html = render(|| {
        rsx! {
            ConfirmImportStep {
                staged: a_suggestion("2024-06-01 Oslo Hills Walk"),
                on_confirm: move |_| {},
                on_start_over: move |_| {},
            }
        }
    });

    // US-12's suggestion, in the field rather than as a placeholder — the
    // owner keeps it and types, rather than retyping it.
    assert!(
        html.contains(r#"value="2024-06-01 Oslo Hills Walk""#),
        "{html}"
    );
    // US-11 and US-31.
    assert!(html.contains(r#"id="import-activity""#), "{html}");
    assert!(html.contains(r#"value="hiking""#), "{html}");
    assert!(html.contains(r#"value="recorded""#), "{html}");
    assert!(html.contains(r#"value="planned""#), "{html}");
    // US-4: the guess, offered as an override rather than hidden.
    assert!(html.contains(r#"value="Europe/Oslo""#), "{html}");
    // US-2: photos chosen in one dialog, with the track.
    assert!(html.contains(r#"id="import-photos""#), "{html}");
    assert!(html.contains("multiple"), "{html}");
}

#[test]
fn the_confirm_step_starts_on_recorded() {
    // US-31: "defaulting to Recorded".
    let html = render(|| {
        rsx! {
            ConfirmImportStep {
                staged: a_suggestion("2024-06-01 "),
                on_confirm: move |_| {},
                on_start_over: move |_| {},
            }
        }
    });

    let recorded = html
        .find(r#"value="recorded""#)
        .expect("a recorded radio: {html}");
    let planned = html
        .find(r#"value="planned""#)
        .expect("a planned radio: {html}");
    // The checked attribute belongs to the recorded input, which comes first.
    let checked = html.find("checked").expect("something is chosen");
    assert!(checked > recorded && checked < planned, "{html}");
}

#[test]
fn the_confirm_step_offers_a_way_back_to_the_picker() {
    // Without one there is no way to reach `cancel_staged_import` at all, and
    // an upload the owner thought better of sits in `import_staging` with its
    // whole GPX in it until the sweeper gets to it a day later.
    let html = render(|| {
        rsx! {
            ConfirmImportStep {
                staged: a_suggestion("2024-06-01 "),
                on_confirm: move |_| {},
                on_start_over: move |_| {},
            }
        }
    });

    assert!(html.contains(r#"id="import-start-over""#), "{html}");
    assert!(html.contains("different file"), "{html}");
}

#[test]
fn the_first_step_asks_for_the_file_and_nothing_else() {
    // Nothing to name yet: the archive has not read the track, so it has no
    // date to suggest — which is the whole reason this step exists.
    let html = render(|| rsx! { ChooseGpx { reading: false, on_choose: move |_| {} } });

    assert!(html.contains(r#"id="import-gpx""#), "{html}");
    assert!(!html.contains(r#"id="import-name""#), "{html}");
    assert!(html.contains("next step"), "{html}");
}

#[test]
fn a_gpx_the_archive_refused_is_shown_where_it_was_chosen() {
    let html = render(|| {
        rsx! {
            ChooseGpx {
                reading: false,
                error: "GPX file contains no tracks".to_string(),
                on_choose: move |_| {},
            }
        }
    });

    assert!(html.contains("GPX file contains no tracks"), "{html}");
}

#[test]
fn a_partly_uploaded_import_names_the_trip_that_does_exist() {
    // ADR-0004's amendment: this is not a failed import, and it must not read
    // as one — the trip is real and the rest of the photos are one click away.
    let html = render(|| {
        rsx! {
            PartialOutcome {
                partial: PartialImport {
                    trip_id: 42,
                    uploaded: 6,
                    total: 12,
                    error: "the archive is unreachable".to_string(),
                },
            }
        }
    });

    assert!(html.contains("6 of 12"), "{html}");
    assert!(html.contains("was created"), "{html}");
    assert!(html.contains("/trips/42"), "the trip is reachable: {html}");
}

#[test]
fn progress_counts_the_photos_the_owner_chose() {
    let html = render(|| rsx! { UploadProgress { progress: Progress { done: 6, total: 12 } } });

    assert!(html.contains("6 of 12"), "{html}");
    assert!(html.contains(r#"max="12""#), "{html}");
}

// ── Recovering an expired upload, against a real archive ─────────────────

#[tokio::test]
async fn an_expired_upload_is_re_sent_rather_than_asked_for_again() {
    // The sweeper takes a parse the owner left sitting, and a restart loses
    // one. Both answer 404 to a confirmation, and the screen still holds the
    // bytes — so the import goes through instead of sending them back to the
    // file picker.
    let (base_url, _dir) = serve_test_archive().await;
    let gpx = crate::test_support::SAMPLE_GPX.to_vec();
    let staged = api::stage_gpx(&base_url, "track.gpx", gpx.clone())
        .await
        .expect("stage");
    api::cancel_staged_import(&base_url, staged.staging_id)
        .await
        .expect("the parse is gone, as the sweeper would leave it");

    let id = confirm_or_restage(
        &base_url,
        &staged,
        &ConfirmImport {
            name: Some("Recovered".to_string()),
            ..Default::default()
        },
        Some(("track.gpx".to_string(), gpx)),
    )
    .await
    .result
    .expect("the import goes through anyway");

    assert_eq!(
        api::get_trip(&base_url, id).await.expect("trip").name,
        "Recovered"
    );
}

#[tokio::test]
async fn a_refused_retry_after_a_re_stage_hands_back_the_parse_that_is_live() {
    // The recovery path replaces the parked parse, so the screen's handle has
    // to follow it. Holding the old, already-spent id would make the owner's
    // next try 404, re-upload the whole file again, and leak another
    // `import_staging` row — once per corrected field.
    let (base_url, _dir) = serve_test_archive().await;
    let gpx = crate::test_support::SAMPLE_GPX.to_vec();
    let staged = api::stage_gpx(&base_url, "track.gpx", gpx.clone())
        .await
        .expect("stage");
    // A parse staged *after* it and left parked, so cancelling the one above
    // does not hand its id straight back to the re-staged row: SQLite gives a
    // new row `max(rowid) + 1`, so without something above it a stale handle
    // would keep working here and hide the very bug under test.
    api::stage_gpx(&base_url, "decoy.gpx", gpx.clone())
        .await
        .expect("stage a decoy");
    api::cancel_staged_import(&base_url, staged.staging_id)
        .await
        .expect("the parse is gone, as the sweeper would leave it");

    let outcome = confirm_or_restage(
        &base_url,
        &staged,
        &ConfirmImport {
            activity_type: Some("teleportation".to_string()),
            ..Default::default()
        },
        Some(("track.gpx".to_string(), gpx)),
    )
    .await;

    outcome
        .result
        .expect_err("the archive knows no such activity");
    let parked = outcome
        .parked
        .expect("the re-staged parse is still waiting to be confirmed");
    assert_ne!(
        parked.staging_id, staged.staging_id,
        "the handle must be the live one, not the spent one"
    );

    // The owner corrects the field: one more confirmation, no third upload.
    let id = api::confirm_import(
        &base_url,
        parked.staging_id,
        &ConfirmImport {
            activity_type: Some("cycling".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("the corrected retry lands");
    assert_eq!(
        api::list_trips(&base_url, String::new())
            .await
            .expect("list")
            .len(),
        1,
        "exactly one trip, however many times it took"
    );
    let _ = id;
}

#[tokio::test]
async fn a_refused_confirmation_keeps_the_parse_the_screen_already_had() {
    // The ordinary refusal: nothing expired, so nothing is re-staged and the
    // handle the screen holds is still the right one.
    let (base_url, _dir) = serve_test_archive().await;
    let staged = api::stage_gpx(
        &base_url,
        "track.gpx",
        crate::test_support::SAMPLE_GPX.to_vec(),
    )
    .await
    .expect("stage");

    let outcome = confirm_or_restage(
        &base_url,
        &staged,
        &ConfirmImport {
            kind: Some("scheduled".to_string()),
            ..Default::default()
        },
        None,
    )
    .await;

    outcome.result.expect_err("no such kind");
    assert_eq!(
        outcome.parked.expect("still parked").staging_id,
        staged.staging_id
    );
}

#[tokio::test]
async fn a_confirmed_import_leaves_nothing_parked() {
    let (base_url, _dir) = serve_test_archive().await;
    let staged = api::stage_gpx(
        &base_url,
        "track.gpx",
        crate::test_support::SAMPLE_GPX.to_vec(),
    )
    .await
    .expect("stage");

    let outcome = confirm_or_restage(&base_url, &staged, &ConfirmImport::default(), None).await;

    outcome.result.expect("confirm");
    assert!(
        outcome.parked.is_none(),
        "a spent parse must not be offered for confirmation again"
    );
}

#[tokio::test]
async fn an_expired_upload_with_no_bytes_in_hand_says_what_to_do() {
    let (base_url, _dir) = serve_test_archive().await;
    let staged = api::stage_gpx(
        &base_url,
        "track.gpx",
        crate::test_support::SAMPLE_GPX.to_vec(),
    )
    .await
    .expect("stage");
    api::cancel_staged_import(&base_url, staged.staging_id)
        .await
        .expect("cancel");

    let err = confirm_or_restage(&base_url, &staged, &ConfirmImport::default(), None)
        .await
        .result
        .expect_err("there is nothing left to send");

    assert!(err.to_string().contains("choose the file again"), "{err}");
}
