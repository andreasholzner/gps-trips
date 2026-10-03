//! US-82 — a shared summary, as its recipient sees it, against a real
//! archive (ADR-0012). Clicking a line on its map is the browser layer's
//! (`tests/browser/share.spec.mjs`).

use super::*;
use crate::api::{create_share, ApiClient};
use crate::shared::{Shared, SharedTripDetail};
use crate::test_support::{
    anonymous, import_sample, render_against_archive, serve_test_archive, tag_trip,
};
use trip_archive_types::{CreateShare, ShareExpiry};

/// Share the summary of `tags` as the owner; a recipient's client and the
/// token.
async fn shared(archive: &ApiClient, tags: &[&str], label: Option<&str>) -> (ApiClient, String) {
    let created = create_share(
        archive,
        &CreateShare {
            trip_ids: Vec::new(),
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
            label: label.map(str::to_string),
            expiry: ShareExpiry::Never,
        },
    )
    .await
    .expect("share");
    (
        anonymous(archive).for_share(created.token.clone()),
        created.token,
    )
}

#[tokio::test]
async fn us82_the_recipient_sees_the_summary_read_only_under_its_title() {
    let (archive, _dir) = serve_test_archive().await;
    // One trip: a shared summary is the summary even then, not the trip.
    let id = import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
    tag_trip(&archive, id, "alps").await;
    let (recipient, token) = shared(&archive, &["alps"], Some("Summer 2026")).await;

    let html = render_against_archive(
        &recipient,
        {
            let token = token.clone();
            move || rsx! { Shared { token: token.clone() } }
        },
        |html| html.contains("summary-figures"),
    )
    .await;

    assert!(
        html.contains(r#"<h1 id="share-title">Summer 2026</h1>"#),
        "{html}"
    );
    assert!(html.contains(r#"id="summary-chosen""#), "{html}");
    assert!(html.contains(r#"id="overview-map""#), "{html}");
    assert!(html.contains(r#"id="summary-trips""#), "{html}");
    assert!(html.contains("Oslo Hills Walk"), "{html}");
    // Every trip opens as a shared trip, never on the owner's page.
    assert!(
        html.contains(&format!(r#"href="/s/{token}/trips/{id}""#)),
        "{html}"
    );
    assert!(!html.contains(r#"href="/trips/"#), "{html}");
    // Neither widened nor narrowed, nor shared on.
    assert!(!html.contains("summary-tag-input"), "{html}");
    assert!(!html.contains("Remove alps"), "{html}");
    assert!(!html.contains("share-summary"), "{html}");
    assert!(!html.contains("track-map"), "{html}");
}

#[tokio::test]
async fn us82_without_a_label_the_tag_names_are_the_title_each_in_its_color() {
    let (archive, _dir) = serve_test_archive().await;
    let walk = import_sample(&archive, &[("name", "Walk")]).await;
    let ride = import_sample(&archive, &[("name", "Ride")]).await;
    tag_trip(&archive, walk, "alps").await;
    tag_trip(&archive, ride, "norway").await;
    let (recipient, token) = shared(&archive, &["norway", "alps"], None).await;

    let html = render_against_archive(
        &recipient,
        move || rsx! { Shared { token: token.clone() } },
        |html| html.contains("summary-figures"),
    )
    .await;

    assert!(
        html.contains(r#"<h1 id="share-title">norway, alps</h1>"#),
        "{html}"
    );
    let chosen = html
        .split(r#"id="summary-chosen""#)
        .nth(1)
        .expect("the shared tags");
    assert!(chosen.contains(r#"class="swatch""#), "{chosen}");
    assert!(chosen.find("norway").unwrap() < chosen.find("alps").unwrap());
}

#[tokio::test]
async fn us82_a_shared_summary_without_trips_says_so() {
    let (archive, _dir) = serve_test_archive().await;
    let planned = import_sample(&archive, &[("kind", "planned")]).await;
    tag_trip(&archive, planned, "someday").await;
    let (recipient, token) = shared(&archive, &["someday"], None).await;

    let html = render_against_archive(
        &recipient,
        move || rsx! { Shared { token: token.clone() } },
        |html| html.contains("summary-empty"),
    )
    .await;

    assert!(
        html.contains("No recorded trips are tagged someday."),
        "{html}"
    );
}

#[tokio::test]
async fn us82_a_trip_of_a_shared_summary_leads_back_to_it() {
    let (archive, _dir) = serve_test_archive().await;
    let id = import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
    tag_trip(&archive, id, "alps").await;
    let (recipient, token) = shared(&archive, &["alps"], None).await;

    let html = render_against_archive(
        &recipient,
        {
            let token = token.clone();
            move || rsx! { SharedTripDetail { token: token.clone(), id } }
        },
        |html| html.contains("track-map"),
    )
    .await;

    assert!(
        html.contains(&format!(r#"href="/s/{token}">alps</a>"#)),
        "{html}"
    );
}
