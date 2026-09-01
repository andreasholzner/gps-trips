//! The API client against a real server, no mocks: these carry the
//! acceptance criteria that live in the *request* rather than in a
//! screen. Driving the same call from a click belongs to the browser
//! layer (ADR-0012's 2026-08-26b amendment).
//!
//! Split out of `api.rs` to keep that file under the repo's line cap,
//! the way `server/repo/trip.rs` already splits its own.

use super::*;
use crate::test_support::{import_sample, serve_test_archive};
use trip_archive_types::ActivityType;

#[tokio::test]
async fn bulk_tagging_applies_every_tag_to_every_selected_trip() {
    let (base_url, _dir) = serve_test_archive().await;
    let first = import_sample(&base_url, &[("name", "Oslo Hills Walk")]).await;
    let second = import_sample(&base_url, &[("name", "Inn Valley Ride")]).await;

    bulk_add_tags(
        &base_url,
        &[first, second],
        &["alpine".to_string(), "summer".to_string()],
    )
    .await
    .expect("bulk tag");

    let names: Vec<String> = list_tags(&base_url)
        .await
        .expect("tags")
        .into_iter()
        .map(|tag| tag.name)
        .collect();
    assert!(names.contains(&"alpine".to_string()), "{names:?}");
    assert!(names.contains(&"summer".to_string()), "{names:?}");
    // Both trips carry both tags: filtering on both lists both.
    let both = list_trips(&base_url, "tags=alpine,summer".to_string())
        .await
        .expect("filtered list");
    assert_eq!(both.len(), 2, "{both:?}");
}

/// JPEG magic plus padding: enough for the archive to store and serve,
/// and undecodable, which the thumbnail step already tolerates (US-5).
const FAKE_JPEG: &[u8] = b"\xFF\xD8\xFF-fake-jpeg";

// ── US-15: editing a trip's name and activity type ───────────────────
//
// The acceptance criterion is that the new values are saved, so these
// read them back over the API. Which fields a form sends is the screen's
// business, and tested there.

#[tokio::test]
async fn an_edited_name_and_activity_type_are_saved() {
    let (base_url, _dir) = serve_test_archive().await;
    let id = import_sample(&base_url, &[]).await;

    edit_trip(
        &base_url,
        id,
        &TripEdit {
            name: Some("Renamed Trip".to_string()),
            activity_type: Some("cycling".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("edit");

    let trip = get_trip(&base_url, id).await.expect("trip");
    assert_eq!(trip.name, "Renamed Trip");
    assert_eq!(trip.activity_type, ActivityType::Cycling);
}

#[tokio::test]
async fn editing_one_field_leaves_the_other_alone() {
    // The reason only changed fields are sent: an omitted field must not
    // be written back from what this screen happened to load with.
    let (base_url, _dir) = serve_test_archive().await;
    let id = import_sample(&base_url, &[("activity_type", "hiking")]).await;

    edit_trip(
        &base_url,
        id,
        &TripEdit {
            name: Some("Only Name Changed".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("edit");

    let trip = get_trip(&base_url, id).await.expect("trip");
    assert_eq!(trip.name, "Only Name Changed");
    assert_eq!(trip.activity_type, ActivityType::Hiking);
}

#[tokio::test]
async fn a_blank_name_is_refused_in_the_archives_own_words() {
    let (base_url, _dir) = serve_test_archive().await;
    let id = import_sample(&base_url, &[]).await;

    let err = edit_trip(
        &base_url,
        id,
        &TripEdit {
            name: Some("   ".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect_err("a blank name is not a name");

    assert!(err.to_string().contains("name"), "{err}");
    assert_eq!(
        get_trip(&base_url, id).await.expect("trip").name,
        "Oslo Hills Walk",
        "a refused edit changes nothing"
    );
}

#[tokio::test]
async fn a_privacy_on_a_trip_that_never_came_from_komoot_is_refused() {
    // US-35: privacy belongs to the linked tour, so an unlinked trip has
    // none to change — and the screen offers no picker for one.
    let (base_url, _dir) = serve_test_archive().await;
    let id = import_sample(&base_url, &[]).await;

    let err = edit_trip(
        &base_url,
        id,
        &TripEdit {
            privacy_status: Some("public".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect_err("there is no tour to make public");

    assert!(!err.to_string().is_empty(), "the archive says why");
}

// ── US-33: tagging a trip from its detail screen ─────────────────────

#[tokio::test]
async fn a_new_tag_is_created_by_using_it_and_normalized_as_it_is_stored() {
    // "Tag names are normalized (trimmed, lowercased) so casing doesn't
    // create duplicates."
    let (base_url, _dir) = serve_test_archive().await;
    let id = import_sample(&base_url, &[]).await;

    let created = add_trip_tag(&base_url, id, "  Alpine  ")
        .await
        .expect("tag");
    assert_eq!(created.name, "alpine");

    // The same name in another casing joins the tag that exists rather
    // than making a second one.
    add_trip_tag(&base_url, id, "ALPINE").await.expect("tag");
    let names: Vec<String> = list_tags(&base_url)
        .await
        .expect("tags")
        .into_iter()
        .map(|tag| tag.name)
        .collect();
    assert_eq!(names, vec!["alpine".to_string()]);
}

#[tokio::test]
async fn untagging_a_trip_keeps_the_tag_for_next_time() {
    // "Untagging a trip keeps the now-unused tag row around, suggestible
    // again later."
    let (base_url, _dir) = serve_test_archive().await;
    let id = import_sample(&base_url, &[]).await;
    let tag = add_trip_tag(&base_url, id, "alpine").await.expect("tag");

    remove_trip_tag(&base_url, id, tag.id).await.expect("untag");

    assert!(
        list_trip_tags(&base_url, id)
            .await
            .expect("trip tags")
            .is_empty(),
        "the trip is untagged"
    );
    assert_eq!(
        list_tags(&base_url).await.expect("tags").len(),
        1,
        "the tag itself outlives the trip that used it"
    );
}

#[tokio::test]
async fn a_name_the_archive_refuses_is_reported_in_its_own_words() {
    let (base_url, _dir) = serve_test_archive().await;
    let id = import_sample(&base_url, &[]).await;

    let err = add_trip_tag(&base_url, id, "day trip")
        .await
        .expect_err("a name with a space is not a tag");

    assert!(err.to_string().contains("cannot contain spaces"), "{err}");
    assert!(list_tags(&base_url).await.expect("tags").is_empty());
}

// ── US-9: deleting a trip ────────────────────────────────────────────

#[tokio::test]
async fn a_deleted_trip_is_gone_from_the_archive() {
    let (base_url, _dir) = serve_test_archive().await;
    let id = import_sample(&base_url, &[]).await;

    delete_trip(&base_url, id).await.expect("delete");

    let err = get_trip(&base_url, id)
        .await
        .expect_err("the trip must be gone");
    assert!(err.is_not_found(), "{err}");
    assert!(list_trips(&base_url, String::new())
        .await
        .expect("list")
        .is_empty());
}

#[tokio::test]
async fn deleting_a_trip_that_is_already_gone_says_so() {
    // Two windows, or the Back button onto a trip just deleted.
    let (base_url, _dir) = serve_test_archive().await;

    let err = delete_trip(&base_url, 9_999)
        .await
        .expect_err("there is nothing to delete");

    assert!(err.is_not_found(), "{err}");
}

#[test]
fn the_original_gpx_is_downloaded_from_the_archive_that_stored_it() {
    // US-21, and US-16's reason for it being absolute: on Android the app
    // is not served from the archive at all.
    assert_eq!(
        original_gpx_url("http://archive.test", 7),
        "http://archive.test/api/trips/7/gpx"
    );
}

#[test]
fn only_a_content_type_worth_attaching_is_attached() {
    assert_eq!(usable_content_type(Some("image/jpeg")), Some("image/jpeg"));
    assert_eq!(
        usable_content_type(Some("image/svg+xml")),
        Some("image/svg+xml")
    );
    // What a web picker reports for a file it cannot classify, and what
    // another platform's might.
    assert_eq!(usable_content_type(Some("")), None);
    assert_eq!(usable_content_type(Some("image")), None);
    assert_eq!(usable_content_type(Some("image/")), None);
    assert_eq!(usable_content_type(Some("image/jpeg; charset=utf-8")), None);
    assert_eq!(usable_content_type(None), None);
}

#[test]
fn only_a_servers_own_words_reach_the_owner() {
    assert_eq!(
        readable_body("tag names cannot contain spaces".to_string()),
        Some("tag names cannot contain spaces".to_string())
    );
    assert_eq!(readable_body("   ".to_string()), None);
    // An error page, not a message: the caller falls back to naming the
    // request and its status.
    assert_eq!(readable_body("<!DOCTYPE html><h1>oh no".to_string()), None);
}

// US-2's "photos can be added at a later time", from the SPA. Uploading
// is a request, not a screen behaviour, so it belongs here — only the
// file picker itself needs a browser (ADR-0012).
#[tokio::test]
async fn a_photo_added_after_the_import_joins_the_trips_photos() {
    let (base_url, _dir) = serve_test_archive().await;
    let id = import_sample(&base_url, &[]).await;

    add_photos(
        &base_url,
        id,
        vec![PhotoUpload {
            file_name: "later.jpg".to_string(),
            content_type: Some("image/jpeg".to_string()),
            bytes: FAKE_JPEG.to_vec(),
        }],
    )
    .await
    .expect("upload");

    let photos = list_photos(&base_url, id).await.expect("photos");
    assert_eq!(photos.len(), 1, "{photos:?}");
    assert_eq!(photos[0].original_name, "later.jpg");
    // US-5: the gallery is handed a thumbnail URL either way — the
    // full-size one stands in when no thumbnail could be made, as here.
    assert!(!photos[0].thumbnail_url.is_empty(), "{photos:?}");
}

#[tokio::test]
async fn photos_added_later_accumulate_rather_than_replace() {
    let (base_url, _dir) = serve_test_archive().await;
    let id = import_sample(&base_url, &[]).await;

    for name in ["a.jpg", "b.jpg"] {
        add_photos(
            &base_url,
            id,
            vec![PhotoUpload {
                file_name: name.to_string(),
                content_type: Some("image/jpeg".to_string()),
                bytes: FAKE_JPEG.to_vec(),
            }],
        )
        .await
        .expect("upload");
    }

    assert_eq!(list_photos(&base_url, id).await.expect("photos").len(), 2);
}

#[tokio::test]
async fn adding_a_photo_to_a_trip_that_is_gone_says_so() {
    let (base_url, _dir) = serve_test_archive().await;

    let err = add_photos(
        &base_url,
        9_999,
        vec![PhotoUpload {
            file_name: "later.jpg".to_string(),
            content_type: Some("image/jpeg".to_string()),
            bytes: FAKE_JPEG.to_vec(),
        }],
    )
    .await
    .expect_err("there is no such trip to add to");

    assert!(err.is_not_found(), "{err}");
}

#[tokio::test]
async fn a_selection_holding_a_vanished_trip_tags_nothing_at_all() {
    // US-34's all-or-nothing rule: the whole request 404s and no tag is
    // created or linked, so a stale selection can't half-apply.
    let (base_url, _dir) = serve_test_archive().await;
    let existing = import_sample(&base_url, &[]).await;

    let err = bulk_add_tags(&base_url, &[existing, 9_999], &["alpine".to_string()])
        .await
        .expect_err("a vanished trip must fail the request");

    assert!(
        err.to_string().contains("no longer exist"),
        "the message must say nothing was tagged: {err}"
    );
    assert_eq!(list_tags(&base_url).await.expect("tags"), Vec::new());
}

#[tokio::test]
async fn an_invalid_tag_name_is_reported_readably_and_tags_nothing() {
    let (base_url, _dir) = serve_test_archive().await;
    let trip = import_sample(&base_url, &[]).await;

    let err = bulk_add_tags(&base_url, &[trip], &["day trip".to_string()])
        .await
        .expect_err("a name with a space must be rejected");

    // The server's own wording, not a status code the owner must decode.
    assert!(err.to_string().contains("cannot contain spaces"), "{err}");
    assert_eq!(list_tags(&base_url).await.expect("tags"), Vec::new());
}

// ── US-12: the two-phase import, from the client that drives it ──────────
//
// Uploading and confirming are requests, not screen behaviour, so they
// belong here; only the file picker itself needs a browser (ADR-0012).

const SAMPLE_GPX: &[u8] = include_bytes!("../../../../tests/fixtures/sample.gpx");

async fn stage_sample(base_url: &str) -> StagedImport {
    stage_gpx(base_url, "track.gpx", SAMPLE_GPX.to_vec())
        .await
        .expect("stage")
}

#[tokio::test]
async fn staging_a_gpx_suggests_a_name_that_leads_with_the_tracks_date() {
    // US-12's point, over the wire: the screen has something to prefill
    // before the owner has typed anything.
    let (base_url, _dir) = serve_test_archive().await;

    let staged = stage_sample(&base_url).await;

    assert_eq!(staged.suggested_name, "2024-06-01 Oslo Hills Walk");
    assert_eq!(staged.timezone, "Europe/Oslo");
    assert!(staged.staging_id > 0);
}

#[tokio::test]
async fn confirming_creates_the_trip_the_owner_described() {
    let (base_url, _dir) = serve_test_archive().await;
    let staged = stage_sample(&base_url).await;

    let id = confirm_import(
        &base_url,
        staged.staging_id,
        &ConfirmImport {
            name: Some("2024-06-01 Nordmarka".to_string()),
            activity_type: Some("hiking".to_string()),
            kind: Some("planned".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("confirm");

    let trip = get_trip(&base_url, id).await.expect("trip");
    assert_eq!(trip.name, "2024-06-01 Nordmarka");
    assert_eq!(trip.activity_type, ActivityType::Hiking);
}

#[tokio::test]
async fn a_gpx_the_archive_cannot_use_is_refused_in_its_own_words() {
    let (base_url, _dir) = serve_test_archive().await;

    let err = stage_gpx(&base_url, "broken.gpx", b"not xml at all".to_vec())
        .await
        .expect_err("the archive cannot read that");

    // The server's sentence, not a status code the owner must decode — and
    // not an error page either (`readable_body`).
    assert!(!err.to_string().is_empty(), "{err}");
    assert!(!err.to_string().starts_with('<'), "{err}");
}

#[tokio::test]
async fn a_staged_import_that_is_already_spent_says_so() {
    // A double submit, or the Back button onto a confirmed form. The screen
    // reads this as "gone" rather than as a fault (`is_not_found`).
    let (base_url, _dir) = serve_test_archive().await;
    let staged = stage_sample(&base_url).await;
    confirm_import(&base_url, staged.staging_id, &ConfirmImport::default())
        .await
        .expect("confirm");

    let err = confirm_import(&base_url, staged.staging_id, &ConfirmImport::default())
        .await
        .expect_err("the parse is spent");

    assert!(err.is_not_found(), "{err}");
    assert_eq!(
        list_trips(&base_url, String::new())
            .await
            .expect("list")
            .len(),
        1
    );
}

#[tokio::test]
async fn a_refused_confirmation_is_readable_and_leaves_the_parse_for_a_retry() {
    // What makes the screen able to keep the owner on step two: the upload
    // is not lost when a field is wrong.
    let (base_url, _dir) = serve_test_archive().await;
    let staged = stage_sample(&base_url).await;

    let err = confirm_import(
        &base_url,
        staged.staging_id,
        &ConfirmImport {
            activity_type: Some("teleportation".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect_err("the archive knows no such activity");
    assert!(!err.to_string().is_empty(), "{err}");
    assert!(!err.is_not_found(), "the parse is still there: {err}");

    let id = confirm_import(
        &base_url,
        staged.staging_id,
        &ConfirmImport {
            activity_type: Some("cycling".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("the retry lands");
    assert_eq!(
        get_trip(&base_url, id).await.expect("trip").activity_type,
        ActivityType::Cycling
    );
}

#[tokio::test]
async fn cancelling_a_staged_import_takes_it_back() {
    // The screen cancels when the owner picks a different file or leaves.
    let (base_url, _dir) = serve_test_archive().await;
    let staged = stage_sample(&base_url).await;

    cancel_staged_import(&base_url, staged.staging_id)
        .await
        .expect("cancel");

    let err = confirm_import(&base_url, staged.staging_id, &ConfirmImport::default())
        .await
        .expect_err("there is nothing left to confirm");
    assert!(err.is_not_found(), "{err}");
    assert!(list_trips(&base_url, String::new())
        .await
        .expect("list")
        .is_empty());
}
