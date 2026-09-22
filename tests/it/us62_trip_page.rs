//! US-62 acceptance tests — "the trip's page shows its numbers at a glance and
//! lets me look at a photo properly".
//!
//! What this file covers is the data the screen reads its times from: the
//! offsets served with the track, the local start date on the trip, and each
//! photo's capture time. How the screen renders them is `crates/ui-dioxus`'
//! business and the browser layer's (ADR-0012).
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

use crate::common::{body_string, get, import_sample, test_app};

// ── The readout: offsets along the served track ─────────────────────────────

#[tokio::test]
async fn us62_the_served_track_carries_the_offsets_its_points_were_in() {
    // SAMPLE_GPX is a June morning in Oslo: one zone, one offset, +02:00.
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = get(&app, &format!("/api/trips/{id}/track.geojson")).await;
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();

    assert_eq!(
        json["properties"]["utc_offsets"],
        serde_json::json!([[0, 7200]]),
        "{json}"
    );
    // The timestamps the readout reads are still there, one per point.
    assert_eq!(
        json["properties"]["timestamps"].as_array().unwrap().len(),
        3
    );
}
