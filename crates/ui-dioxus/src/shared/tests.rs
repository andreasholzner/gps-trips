//! US-53 — the recipient's screens, against a real archive (ADR-0012).

use super::*;
use crate::api::create_share;
use crate::test_support::{
    anonymous, import_gpx, import_sample, render_against_archive, serve_test_archive,
};
use trip_archive_types::{ActivityType, CreateShare, ShareExpiry};

/// Share `trip_ids` as the owner and return a recipient's client for it.
async fn shared(
    archive: &ApiClient,
    trip_ids: Vec<i64>,
    label: Option<&str>,
) -> (ApiClient, String) {
    let created = create_share(
        archive,
        &CreateShare {
            trip_ids,
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
async fn us53_a_share_of_several_lists_them_under_its_title_and_a_map() {
    let (archive, _dir) = serve_test_archive().await;
    let first = import_sample(&archive, &[("name", "Day one")]).await;
    let second = import_sample(&archive, &[("name", "Day two")]).await;
    let (recipient, token) = shared(&archive, vec![first, second], Some("Lofoten 2026")).await;

    let html = render_against_archive(
        &recipient,
        move || rsx! { Shared { token: token.clone() } },
        |html| html.contains("shared-trips"),
    )
    .await;

    assert!(
        html.contains(r#"<h1 id="share-title">Lofoten 2026</h1>"#),
        "{html}"
    );
    assert!(html.contains(r#"id="overview-map""#), "{html}");
    for (id, name) in [(first, "Day one"), (second, "Day two")] {
        assert!(
            html.contains(&format!("/s/{}/trips/{id}", recipient_token(&html))),
            "{html}"
        );
        assert!(html.contains(name), "{html}");
    }
}

#[tokio::test]
async fn us53_a_shared_trip_is_dated_where_it_started() {
    // Starts 22:30 UTC near Oslo: half past midnight the next day there.
    const LATE_EVENING_GPX: &[u8] = include_bytes!("../../../../tests/fixtures/late_evening.gpx");
    let (archive, _dir) = serve_test_archive().await;
    let first = import_gpx(&archive, LATE_EVENING_GPX, &[("name", "Night ride")]).await;
    // A second, so the share opens on its list rather than on the trip.
    let second = import_gpx(&archive, LATE_EVENING_GPX, &[("name", "Night two")]).await;
    let (recipient, token) = shared(&archive, vec![first, second], None).await;
    let trip = crate::api::get_shared_trip(&recipient, first)
        .await
        .unwrap();
    let local = trip.start_date.expect("a dated trip");
    let utc = &trip.start_time.expect("a timed trip")[..10];
    assert_ne!(local, utc, "the fixture must straddle midnight");

    let html = render_against_archive(
        &recipient,
        move || rsx! { Shared { token: token.clone() } },
        |html| html.contains("shared-trips"),
    )
    .await;

    assert!(html.contains(&local), "{html}");
    assert!(!html.contains(utc), "{html}");
}

/// The token as it appears in the rendered links.
fn recipient_token(html: &str) -> String {
    html.split("/s/")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .expect("a share link")
        .to_string()
}

#[tokio::test]
async fn us53_a_share_of_one_trip_opens_on_it_read_only() {
    let (archive, _dir) = serve_test_archive().await;
    let id = import_sample(
        &archive,
        &[("name", "Oslo Hills Walk"), ("activity_type", "hiking")],
    )
    .await;
    let (recipient, token) = shared(&archive, vec![id], Some("For Kari")).await;

    let html = render_against_archive(
        &recipient,
        move || rsx! { Shared { token: token.clone() } },
        |html| html.contains("track-map") && html.contains("No photos yet."),
    )
    .await;

    assert!(html.contains("For Kari"), "{html}");
    assert!(html.contains(ActivityType::Hiking.label()), "{html}");
    assert!(html.contains(r#"id="track-map""#), "{html}");
    assert!(
        html.contains(&format!("/s/{}/api/trips/{id}/gpx", recipient_token(&html))),
        "the GPX download goes through the share: {html}"
    );
    // Nothing that changes the trip, and nothing that organises it.
    for owners_only in [
        r#"id="edit-trip""#,
        r#"id="add-photos""#,
        r#"id="delete-trip""#,
        "trip-tags",
        "Tags",
    ] {
        assert!(!html.contains(owners_only), "{owners_only}: {html}");
    }
}

#[tokio::test]
async fn us53_a_dead_link_says_so_in_the_recipients_terms() {
    let (archive, _dir) = serve_test_archive().await;
    let recipient = anonymous(&archive).for_share("not-a-token");

    let html = render_against_archive(
        &recipient,
        move || rsx! { Shared { token: "not-a-token".to_string() } },
        |html| html.contains("class=\"error\""),
    )
    .await;
    assert!(html.contains(DEAD_LINK), "{html}");
}

#[test]
fn us53_each_trip_whose_track_was_read_gets_a_line() {
    let trip = |id: i64, name: &str| SharedTripSummary {
        id,
        name: name.to_string(),
        activity_type: ActivityType::Hiking,
        start_time: None,
        start_date: None,
        distance_m: 0.0,
        ascent_m: None,
        duration_secs: None,
    };
    let track: Track = serde_json::from_value(serde_json::json!({
        "geometry": { "coordinates": [[10.7, 59.9, 0.0], [10.8, 60.0, 0.0]] }
    }))
    .unwrap();

    let lines = overview_lines(&[trip(1, "One"), trip(2, "Two")], &[(2, track)]);

    assert_eq!(
        lines,
        [OverviewLine {
            id: 2,
            name: "Two".to_string(),
            points: vec![[59.9, 10.7], [60.0, 10.8]],
        }]
    );
}
