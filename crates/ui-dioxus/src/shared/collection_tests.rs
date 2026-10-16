//! US-94 — the recipient's screens for shared collections and their tracks,
//! against a real archive (ADR-0012). Opening the link and clicking through
//! are the browser layer's (`tests/browser/share.spec.mjs`).

use super::*;
use crate::api::{add_tag, create_share, Tagged};
use crate::test_support::{
    anonymous, import_collection, render_against_archive, serve_test_archive, COLLECTION_GPX,
};
use trip_archive_types::{ActivityType, CreateShare, SharedCollection, SharedCollectionTrack};

/// Share as the owner and return a recipient's client and the token.
async fn shared(archive: &ApiClient, request: CreateShare) -> (ApiClient, String) {
    let created = create_share(archive, &request).await.expect("share");
    (
        anonymous(archive).for_share(created.token.clone()),
        created.token,
    )
}

async fn render_share(recipient: &ApiClient, token: &str, until: &'static str) -> String {
    let token = token.to_string();
    render_against_archive(
        recipient,
        move || rsx! { Shared { token: token.clone() } },
        move |html| html.contains(until),
    )
    .await
}

/// What the recipient must never be offered: a tag, a control, a file.
fn assert_read_only(html: &str) {
    for owner_only in [
        "secret",
        r#"id="add-tag""#,
        r#"id="edit-collection""#,
        r#"id="delete-collection""#,
        r#"id="share-collection""#,
        r#"id="edit-trip""#,
        r#"id="delete-trip""#,
        r#"id="look-for-links""#,
        r#"type="checkbox""#,
        "/api/collection-files/",
    ] {
        assert!(!html.contains(owner_only), "{owner_only}: {html}");
    }
}

fn a_collection(id: i64, name: &str) -> SharedCollection {
    SharedCollection {
        id,
        name: name.to_string(),
        description: String::new(),
        activity_type: None,
        track_count: 1,
        waypoint_count: 0,
    }
}

fn a_track(id: i64) -> SharedCollectionTrack {
    SharedCollectionTrack {
        id,
        name: format!("Stage {id}"),
        activity_type: ActivityType::Hiking,
        distance_m: 1_000.0,
        ascent_m: 10.0,
        descent_m: 10.0,
    }
}

#[test]
fn us94_a_share_without_a_label_is_titled_by_what_it_names() {
    let one = ShareOverview {
        collections: vec![a_collection(1, "Alps")],
        ..ShareOverview::default()
    };
    assert_eq!(title(&one), "Alps");
    let several = ShareOverview {
        collections: vec![a_collection(1, "Alps"), a_collection(2, "Tyrol")],
        ..ShareOverview::default()
    };
    assert_eq!(title(&several), "Shared collections");
    let tracks = ShareOverview {
        collection_tracks: vec![a_track(1)],
        ..ShareOverview::default()
    };
    assert_eq!(title(&tracks), "Shared tracks");
    let labelled = ShareOverview {
        label: Some("For Kari".to_string()),
        ..several
    };
    assert_eq!(title(&labelled), "For Kari");
}

#[test]
fn us94_a_tracks_page_leads_back_only_to_a_list_of_more() {
    use super::collection_track::leads_back;
    let one = ShareOverview {
        collection_tracks: vec![a_track(1)],
        ..ShareOverview::default()
    };
    assert!(!leads_back(&one));
    let two = ShareOverview {
        collection_tracks: vec![a_track(1), a_track(2)],
        ..ShareOverview::default()
    };
    assert!(leads_back(&two));
    let of_a_collection = ShareOverview {
        collections: vec![a_collection(1, "Alps")],
        ..one
    };
    assert!(leads_back(&of_a_collection));
}

#[tokio::test]
async fn us94_one_shared_collection_opens_its_page_read_only() {
    let (archive, _dir) = serve_test_archive().await;
    let (id, tracks) = import_collection(&archive, COLLECTION_GPX, &[("name", "Alps")]).await;
    add_tag(&archive, Tagged::Collection(id), "secret")
        .await
        .expect("tag");
    let (recipient, token) = shared(
        &archive,
        CreateShare {
            collection_ids: vec![id],
            ..CreateShare::default()
        },
    )
    .await;

    let html = render_share(&recipient, &token, "collection-waypoints").await;

    assert!(
        html.contains(r#"<h1 id="collection-name">Alps</h1>"#),
        "{html}"
    );
    assert!(html.contains("3 tracks · 2 waypoints"), "{html}");
    assert!(html.contains(r#"id="overview-map""#), "{html}");
    for track in tracks {
        assert!(
            html.contains(&format!(r#"href="/s/{token}/collection-tracks/{track}""#)),
            "{track}: {html}"
        );
    }
    assert!(html.contains("San Bernardino Pass"), "{html}");
    assert!(!html.contains("/waypoints/"), "no waypoint pages: {html}");
    assert!(
        !html.contains("share-title"),
        "no label, no title above: {html}"
    );
    assert_read_only(&html);
}

#[tokio::test]
async fn us94_several_shared_collections_are_listed_under_one_map() {
    let (archive, _dir) = serve_test_archive().await;
    let (alps, _) = import_collection(&archive, COLLECTION_GPX, &[("name", "Alps")]).await;
    let (tyrol, _) = import_collection(&archive, COLLECTION_GPX, &[("name", "Tyrol")]).await;
    let (recipient, token) = shared(
        &archive,
        CreateShare {
            collection_ids: vec![tyrol, alps],
            ..CreateShare::default()
        },
    )
    .await;

    let html = render_share(&recipient, &token, "shared-collections").await;

    assert!(
        html.contains(r#"<h1 id="share-title">Shared collections</h1>"#),
        "{html}"
    );
    assert!(html.contains(r#"id="overview-map""#), "{html}");
    let alps_at = html
        .find(&format!(r#"href="/s/{token}/collections/{alps}""#))
        .expect("Alps listed");
    let tyrol_at = html
        .find(&format!(r#"href="/s/{token}/collections/{tyrol}""#))
        .expect("Tyrol listed");
    assert!(alps_at < tyrol_at, "alphabetically: {html}");
    assert_read_only(&html);

    // One of them, reached from the list, leads back to it.
    let page_token = token.clone();
    let html = render_against_archive(
        &recipient,
        move || rsx! { SharedCollectionPage { token: page_token.clone(), id: tyrol } },
        |html| html.contains("collection-name"),
    )
    .await;
    assert!(html.contains(">Tyrol</h1>"), "{html}");
    assert!(
        html.contains(&format!(r#"href="/s/{token}">Shared collections</a>"#)),
        "{html}"
    );
}

#[tokio::test]
async fn us94_shared_tracks_are_listed_and_one_opens_without_its_collection() {
    let (archive, _dir) = serve_test_archive().await;
    let (_, tracks) = import_collection(&archive, COLLECTION_GPX, &[("name", "Alps")]).await;
    add_tag(&archive, Tagged::CollectionTrack(tracks[0]), "secret")
        .await
        .expect("tag");
    let (recipient, token) = shared(
        &archive,
        CreateShare {
            collection_track_ids: vec![tracks[0], tracks[2]],
            label: Some("Two stages".to_string()),
            ..CreateShare::default()
        },
    )
    .await;

    let html = render_share(&recipient, &token, "collection-tracks").await;
    assert!(
        html.contains(r#"<h1 id="share-title">Two stages</h1>"#),
        "{html}"
    );
    assert!(html.contains(r#"id="overview-map""#), "{html}");
    assert!(html.contains(&format!("/s/{token}/collection-tracks/{}", tracks[0])));
    assert!(
        !html.contains(&format!("/collection-tracks/{}\"", tracks[1])),
        "{html}"
    );
    assert_read_only(&html);

    let page_token = token.clone();
    let track = tracks[0];
    let html = render_against_archive(
        &recipient,
        move || rsx! { SharedCollectionTrackPage { token: page_token.clone(), id: track } },
        |html| html.contains("Download GPX"),
    )
    .await;
    assert!(html.contains(r#"id="trip-name""#), "{html}");
    assert!(html.contains("Distance"), "{html}");
    assert!(!html.contains("collection-of-track"), "{html}");
    assert!(
        !html.contains("Alps"),
        "its collection stays unnamed: {html}"
    );
    assert!(
        html.contains(&format!(r#"href="/s/{token}">Two stages</a>"#)),
        "{html}"
    );
    assert_read_only(&html);
}

#[tokio::test]
async fn us94_a_track_of_a_shared_collection_names_and_leads_to_it() {
    let (archive, _dir) = serve_test_archive().await;
    let (id, tracks) = import_collection(&archive, COLLECTION_GPX, &[("name", "Alps")]).await;
    let (recipient, token) = shared(
        &archive,
        CreateShare {
            collection_ids: vec![id],
            ..CreateShare::default()
        },
    )
    .await;

    let page_token = token.clone();
    let track = tracks[1];
    let html = render_against_archive(
        &recipient,
        move || rsx! { SharedCollectionTrackPage { token: page_token.clone(), id: track } },
        |html| html.contains("collection-of-track"),
    )
    .await;
    assert!(
        html.contains(&format!(r#"href="/s/{token}/collections/{id}">Alps</a>"#)),
        "{html}"
    );
}

#[tokio::test]
async fn us94_one_shared_track_opens_its_page() {
    let (archive, _dir) = serve_test_archive().await;
    let (_, tracks) = import_collection(&archive, COLLECTION_GPX, &[("name", "Alps")]).await;
    let (recipient, token) = shared(
        &archive,
        CreateShare {
            collection_track_ids: vec![tracks[1]],
            ..CreateShare::default()
        },
    )
    .await;

    let html = render_share(&recipient, &token, "Download GPX").await;
    assert!(html.contains(r#"id="trip-name""#), "{html}");
    assert!(
        !html.contains("share-title"),
        "nothing to lead back to: {html}"
    );
    assert!(
        html.contains(&format!(
            "/s/{token}/api/collection-tracks/{}/gpx",
            tracks[1]
        )),
        "{html}"
    );
}

#[test]
fn us104_a_recipients_waypoint_is_named_on_the_map_but_opens_nothing() {
    // There is no shared waypoint page to open.
    let read = Ok(vec![trip_archive_types::SharedWaypoint {
        id: 4,
        name: Some("Pass".to_string()),
        description: None,
        lat: 46.5,
        lon: 9.8,
        ele: None,
        symbol: None,
    }]);

    let drawn = crate::shared::collection::drawn(read);

    assert_eq!(drawn.len(), 1);
    assert_eq!(drawn[0].name.as_deref(), Some("Pass"));
    assert!(!drawn[0].opens);
}
