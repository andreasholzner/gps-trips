//! US-90 — trips and collections in one list, chosen by kind: the screen
//! rendered against a real server, and the rules it combines what it read
//! by (ADR-0012). Toggling a kind and folding a group out are the browser
//! layer's (`tests/browser/trip_list.spec.mjs`).

use super::*;
use crate::test_support::{
    import_collection, import_sample, render_against_archive, serve_test_archive,
    set_track_activity, COLLECTION_GPX,
};

fn showing(kinds: &[Kind]) -> Filters {
    let kinds = kinds
        .iter()
        .skip(1)
        .fold(Kinds::only(kinds[0]), |all, kind| all.toggled(*kind));
    Filters {
        kinds,
        ..Default::default()
    }
}

// ── What is read, combined ───────────────────────────────────────────────────

#[test]
fn us90_the_list_waits_for_every_kind_on() {
    let both = showing(&[Kind::Recorded, Kind::Collections]).kinds;

    assert_eq!(listed(both, Some(Ok(Vec::new())), None), None);
    assert_eq!(listed(both, None, Some(Ok(Vec::new()))), None);
    assert_eq!(
        listed(both, Some(Ok(Vec::new())), Some(Ok(Vec::new()))),
        Some(Ok(Listed::default()))
    );
    assert_eq!(
        listed(Kinds::default(), Some(Ok(Vec::new())), None),
        Some(Ok(Listed::default())),
        "collections off are not waited for"
    );
}

#[test]
fn us90_a_kind_that_could_not_be_read_fails_the_list() {
    let both = showing(&[Kind::Recorded, Kind::Collections]).kinds;
    let failed = api::ApiError::new("unreachable");

    assert_eq!(
        listed(both, Some(Ok(Vec::new())), Some(Err(failed.clone()))),
        Some(Err(failed))
    );
}

#[test]
fn us90_the_trips_are_counted_by_kind_from_one_read() {
    let trip = |trip_kind| TripSummary {
        id: 1,
        name: "Trip".to_string(),
        activity_type: trip_archive_types::ActivityType::Hiking,
        start_time: None,
        start_date: None,
        distance_m: 1_000.0,
        ascent_m: None,
        duration_secs: None,
        trip_kind,
        privacy_status: None,
        min_lat: None,
        min_lon: None,
        max_lat: None,
        max_lon: None,
        has_recorded_trip: false,
    };

    assert_eq!(
        trip_totals(&[
            trip(TripKind::Recorded),
            trip(TripKind::Planned),
            trip(TripKind::Recorded)
        ]),
        TripTotals {
            recorded: 2,
            planned: 1
        }
    );
}

#[test]
fn us90_the_totals_wait_for_every_kind_on_and_name_only_those() {
    let trips = TripTotals {
        recorded: 2,
        planned: 1,
    };
    let both = showing(&[Kind::Planned, Kind::Collections]).kinds;

    assert_eq!(totals(both, Some(trips), None), None);
    assert_eq!(totals(both, None, Some(4)), None);
    assert_eq!(
        totals(both, Some(trips), Some(4)),
        Some(Totals(vec![(Kind::Planned, 1), (Kind::Collections, 4)]))
    );
    assert_eq!(
        totals(Kinds::default(), Some(trips), None),
        Some(Totals(vec![(Kind::Recorded, 2)])),
        "collections off are not waited for"
    );
}

#[test]
fn us90_a_read_is_used_only_for_what_it_was_read_for() {
    let recorded = Kinds::default().trips_only();
    let read = (recorded, 312);

    assert_eq!(read_for(Some(&read), &recorded), Some(&312));
    assert_eq!(
        read_for(Some(&read), &Kinds::only(Kind::Planned).trips_only()),
        None
    );
    assert_eq!(read_for::<Option<Kinds>, usize>(None, &recorded), None);
}

// ── Against a real server ────────────────────────────────────────────────────

#[tokio::test]
async fn us90_recorded_and_planned_trips_are_listed_together() {
    let (archive, _dir) = serve_test_archive().await;
    import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
    import_sample(&archive, &[("name", "Dream Route"), ("kind", "planned")]).await;

    let html = render_against_archive(
        &archive,
        || rsx! { TripList { filters: showing(&[Kind::Recorded, Kind::Planned]) } },
        |html| html.contains("planned trip"),
    )
    .await;

    assert!(html.contains("Oslo Hills Walk"), "{html}");
    assert!(html.contains("Dream Route"), "{html}");
    assert!(html.contains("1 recorded trip · 1 planned trip"), "{html}");
}

#[tokio::test]
async fn us90_trips_and_collections_share_one_table_trips_first() {
    let (archive, _dir) = serve_test_archive().await;
    import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
    import_collection(&archive, COLLECTION_GPX, &[("name", "Lakes")]).await;

    let html = render_against_archive(
        &archive,
        || rsx! { TripList { filters: showing(&[Kind::Recorded, Kind::Collections]) } },
        |html| html.contains("1 collection"),
    )
    .await;

    assert_eq!(html.matches("<table").count(), 1, "{html}");
    let trip = html.find("Oslo Hills Walk").expect("listed");
    let collection = html.find("Lakes").expect("listed");
    assert!(trip < collection, "{html}");
    assert!(html.contains("1 recorded trip · 1 collection"), "{html}");
    assert!(html.contains("3 tracks · 2 waypoints"), "{html}");
    assert!(!html.contains("Stage 3"), "the group is folded: {html}");
}

#[tokio::test]
async fn us90_the_toggles_say_how_many_of_each_kind_on_match() {
    let (archive, _dir) = serve_test_archive().await;
    import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
    import_sample(&archive, &[("name", "Inn Valley Ride")]).await;
    import_collection(&archive, COLLECTION_GPX, &[("name", "Lakes")]).await;

    let html = render_against_archive(
        &archive,
        || {
            rsx! { TripList { filters: Filters { q: "inn".to_string(), ..showing(&[Kind::Recorded, Kind::Collections]) } } }
        },
        |html| html.contains("of 2 recorded trips"),
    )
    .await;

    let toggles = html.split_once("kind-toggles").expect("toggles").1;
    let toggles = toggles.split_once("</div>").expect("closed").0;
    let count = |label: &str| {
        toggles
            .split("<button")
            .find(|toggle| toggle.contains(label))
            .and_then(|toggle| toggle.split_once(r#"class="kind-count">"#))
            .map(|(_, rest)| rest.split('<').next().unwrap_or_default().to_string())
    };
    assert_eq!(count("Recorded").as_deref(), Some("1"), "{html}");
    assert_eq!(count("Collections").as_deref(), Some("0"), "{html}");
    assert_eq!(count("Planned"), None, "off: {html}");
}

#[tokio::test]
async fn us90_the_activity_narrows_a_collection_to_its_tracks_of_it() {
    let (archive, _dir) = serve_test_archive().await;
    let (_, tracks) = import_collection(&archive, COLLECTION_GPX, &[("name", "Lakes")]).await;
    set_track_activity(&archive, tracks[2], "hiking").await;

    let html = render_against_archive(
        &archive,
        || {
            rsx! { TripList { filters: Filters { activity: Some(trip_archive_types::ActivityType::Hiking), ..showing(&[Kind::Collections]) } } }
        },
        |html| html.contains("1 collection"),
    )
    .await;

    assert!(html.contains("1 of 3 tracks"), "{html}");
}

#[tokio::test]
async fn us90_nothing_matching_any_kind_on_says_so() {
    let (archive, _dir) = serve_test_archive().await;
    import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
    import_collection(&archive, COLLECTION_GPX, &[("name", "Lakes")]).await;

    let html = render_against_archive(
        &archive,
        || {
            rsx! { TripList { filters: Filters { q: "nomatch".to_string(), ..showing(&[Kind::Recorded, Kind::Collections]) } } }
        },
        |html| !html.contains("Loading"),
    )
    .await;

    assert!(html.contains("Nothing matches your filters."), "{html}");
}

#[tokio::test]
async fn us90_nothing_is_selected_and_so_nothing_offered_to_begin_with() {
    let (archive, _dir) = serve_test_archive().await;
    import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;

    let html = render_against_archive(
        &archive,
        || rsx! { TripList {} },
        |html| html.contains("Oslo Hills Walk"),
    )
    .await;

    assert!(!html.contains("selection-bar"), "{html}");
    assert!(!html.contains("Tag…"), "{html}");
}
