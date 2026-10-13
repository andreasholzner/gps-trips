//! US-102 acceptance tests (ADR-0022): a collection track's item carries
//! the track's own tags (US-93) as its keywords, as a trip's does, and
//! follows them when they change.

use axum::http::{Method, StatusCode};
use trip_archive::models::Tag;
use trip_archive::server::qmapshack::{collections::track_keyqms, decode};

use crate::common::{body_string, delete, import_collection, json_request, send, COLLECTION_GPX};
use crate::us89_qmapshack_collections::{archive, item, Archive};

async fn tag(archive: &Archive, track: i64, name: &str) -> Tag {
    let body = serde_json::json!({ "name": name }).to_string();
    let uri = format!("/api/collection-tracks/{track}/tags");
    let response = send(&archive.app, json_request(Method::POST, &uri, &body)).await;
    assert_eq!(response.status(), StatusCode::CREATED, "{uri}");
    serde_json::from_str(&body_string(response).await).unwrap()
}

async fn keywords(archive: &Archive, track: i64) -> Vec<String> {
    let target = archive.target().await;
    let (_, _, data, _) = item(&target, &track_keyqms(track)).await;
    target.close().await;
    decode::decode_track_item(&data)
        .expect("track blob decodes")
        .keywords
}

#[tokio::test]
async fn us102_a_tracks_own_tags_are_its_keywords() {
    let archive = archive().await;
    let collection = import_collection(&archive.app, COLLECTION_GPX, &[]).await;
    let tracks = archive.track_ids(collection).await;
    tag(&archive, tracks[0], "telt").await;
    tag(&archive, tracks[0], "fjell").await;
    let body = serde_json::json!({ "name": "bok" }).to_string();
    let uri = format!("/api/collections/{collection}/tags");
    send(&archive.app, json_request(Method::POST, &uri, &body)).await;

    archive.export().await;

    assert_eq!(keywords(&archive, tracks[0]).await, ["fjell", "telt"]);
    assert!(
        keywords(&archive, tracks[1]).await.is_empty(),
        "its collection's tags are not its own"
    );
}

#[tokio::test]
async fn us102_keywords_follow_tagging_renaming_and_deleting_on_the_next_run() {
    let archive = archive().await;
    let collection = import_collection(&archive.app, COLLECTION_GPX, &[]).await;
    let tracks = archive.track_ids(collection).await;
    let fjell = tag(&archive, tracks[0], "fjell").await;
    let telt = tag(&archive, tracks[0], "telt").await;
    archive.export().await;

    // Untag one track, tag another, and rename a tag.
    let uri = format!("/api/collection-tracks/{}/tags/{}", tracks[0], telt.id);
    assert_eq!(
        delete(&archive.app, &uri).await.status(),
        StatusCode::NO_CONTENT
    );
    tag(&archive, tracks[1], "bre").await;
    let body = serde_json::json!({ "name": "berg" }).to_string();
    let uri = format!("/api/tags/{}", fjell.id);
    let response = send(&archive.app, json_request(Method::PATCH, &uri, &body)).await;
    assert!(
        response.status().is_success(),
        "{uri}: {}",
        response.status()
    );

    let second = archive.export().await;
    assert_eq!(second.updated, 2);
    assert_eq!(keywords(&archive, tracks[0]).await, ["berg"]);
    assert_eq!(keywords(&archive, tracks[1]).await, ["bre"]);

    let uri = format!("/api/tags/{}", fjell.id);
    assert!(delete(&archive.app, &uri).await.status().is_success());

    let third = archive.export().await;
    assert_eq!(third.updated, 1);
    assert!(keywords(&archive, tracks[0]).await.is_empty());
}
