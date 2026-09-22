//! US-63 acceptance tests — "the trip list shows me where the trips are, a
//! page at a time, and works on a phone".
//!
//! The map, the paging and the phone layout are the SPA's
//! (`crates/ui-dioxus`, and the browser layer for what only a rendered page
//! shows). What stays here is the server's half: the bounding box the list's
//! rows now carry, so the map has somewhere to put each trip.
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

use crate::common::{body_string, get, import_sample, test_app};

#[tokio::test]
async fn us63_each_listed_trip_carries_its_bounding_box() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let list: serde_json::Value =
        serde_json::from_str(&body_string(get(&app, "/api/trips").await).await).unwrap();
    let detail: serde_json::Value =
        serde_json::from_str(&body_string(get(&app, &format!("/api/trips/{id}")).await).await)
            .unwrap();
    let row = &list[0];

    // The same stored box the detail screen reads, not a second computation.
    for corner in ["min_lat", "min_lon", "max_lat", "max_lon"] {
        assert!(row[corner].is_number(), "{corner}: {row}");
        assert_eq!(row[corner], detail[corner], "{corner}");
    }
}
