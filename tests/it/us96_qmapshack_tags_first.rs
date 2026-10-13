//! US-96 acceptance tests (ADR-0022): an exported item's comment opens with
//! its tags — a trip's, and a collection track's own (US-93) — and one
//! without tags opens as before. The new layout rewrites each item once.

use axum::http::Method;
use sqlx::SqlitePool;
use trip_archive::server::qmapshack::{self, collections::track_keyqms};

use crate::common::{import_collection, import_sample, json_request, send, COLLECTION_GPX};
use crate::us89_qmapshack_collections::{archive, Archive};

async fn tag(archive: &Archive, uri: &str, name: &str) {
    let body = serde_json::json!({ "name": name }).to_string();
    let response = send(&archive.app, json_request(Method::POST, uri, &body)).await;
    assert!(
        response.status().is_success(),
        "{uri}: {}",
        response.status()
    );
}

async fn comment(target: &SqlitePool, keyqms: &str) -> String {
    sqlx::query_scalar("SELECT comment FROM items WHERE keyqms = ?")
        .bind(keyqms)
        .fetch_one(target)
        .await
        .unwrap_or_else(|e| panic!("no item {keyqms}: {e}"))
}

async fn set_comment(target: &SqlitePool, keyqms: &str, comment: &str) {
    sqlx::query("UPDATE items SET comment = ? WHERE keyqms = ?")
        .bind(comment)
        .bind(keyqms)
        .execute(target)
        .await
        .unwrap();
}

#[tokio::test]
async fn us96_tags_open_the_comment_and_an_untagged_item_opens_as_before() {
    let archive = archive().await;
    let trip = import_sample(&archive.app).await;
    let collection = import_collection(&archive.app, COLLECTION_GPX, &[]).await;
    let tracks = archive.track_ids(collection).await;
    tag(&archive, &format!("/api/trips/{trip}/tags"), "telt").await;
    tag(&archive, &format!("/api/trips/{trip}/tags"), "fjell").await;
    tag(
        &archive,
        &format!("/api/collection-tracks/{}/tags", tracks[0]),
        "pass",
    )
    .await;
    tag(
        &archive,
        &format!("/api/collections/{collection}/tags"),
        "bok",
    )
    .await;

    archive.export().await;

    let target = archive.target().await;
    let trip_comment = comment(&target, &qmapshack::keyqms(trip)).await;
    assert!(
        trip_comment.starts_with("Tags: fjell, telt\n"),
        "{trip_comment}"
    );
    let track_comment = comment(&target, &track_keyqms(tracks[0])).await;
    assert!(track_comment.starts_with("Tags: pass\n"), "{track_comment}");
    // Its collection's tags are not its own (US-93): it opens as before.
    let untagged = comment(&target, &track_keyqms(tracks[1])).await;
    assert!(untagged.starts_with("Lakes & Passes · "), "{untagged}");
    assert!(!untagged.contains("Tags:"), "{untagged}");
}

#[tokio::test]
async fn us96_an_item_exported_before_is_rewritten_once() {
    let archive = archive().await;
    let trip = import_sample(&archive.app).await;
    let collection = import_collection(&archive.app, COLLECTION_GPX, &[]).await;
    let tracks = archive.track_ids(collection).await;
    tag(&archive, &format!("/api/trips/{trip}/tags"), "fjell").await;
    tag(
        &archive,
        &format!("/api/collection-tracks/{}/tags", tracks[0]),
        "pass",
    )
    .await;
    archive.export().await;

    // Put back the comments an export before US-96 wrote: a trip's tags as
    // its third line, a track's not at all.
    let target = archive.target().await;
    let trip_key = qmapshack::keyqms(trip);
    let mut lines: Vec<String> = comment(&target, &trip_key)
        .await
        .lines()
        .map(String::from)
        .collect();
    let tags = lines.remove(0);
    lines.insert(2, tags);
    set_comment(&target, &trip_key, &lines.join("\n")).await;
    let track_key = track_keyqms(tracks[0]);
    let current = comment(&target, &track_key).await;
    let (_, rest) = current.split_once('\n').unwrap();
    set_comment(&target, &track_key, rest).await;
    target.close().await;

    let second = archive.export().await;
    assert_eq!(second.updated, 2, "the tagged trip and track");
    assert_eq!(second.failed, 0);
    let third = archive.export().await;
    assert_eq!(third.updated, 0, "rewritten once");
    assert_eq!(third.inserted, 0);
}
