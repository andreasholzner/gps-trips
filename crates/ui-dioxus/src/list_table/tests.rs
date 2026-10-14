//! The list's table rendered with props in hand (ADR-0012): its rows, their
//! kinds, the groups and the selection. Clicking — folding a group out,
//! ticking a row — is a real event, the browser layer's
//! (`tests/browser/trip_list.spec.mjs`).

use super::*;
use crate::test_support::render;
use trip_archive_types::KomootPrivacy;

fn a_trip(id: i64, name: &str) -> TripSummary {
    TripSummary {
        id,
        name: name.to_string(),
        activity_type: ActivityType::Hiking,
        // Half past midnight on the 11th where the trip was; UTC still on
        // the 10th.
        start_time: Some("2026-07-10T22:30:00Z".to_string()),
        start_date: Some("2026-07-11".to_string()),
        distance_m: 12_345.0,
        ascent_m: Some(410.0),
        duration_secs: Some(3_725),
        trip_kind: TripKind::Recorded,
        privacy_status: None,
        min_lat: Some(59.9),
        min_lon: Some(10.7),
        max_lat: Some(60.0),
        max_lon: Some(10.9),
        has_recorded_trip: false,
    }
}

fn a_track(id: i64, name: &str) -> CollectionTrackMark {
    CollectionTrackMark {
        id,
        name: name.to_string(),
        activity_type: ActivityType::Cycling,
        distance_m: 51_000.0,
        ascent_m: 680.0,
        min_lat: 46.0,
        min_lon: 9.0,
        max_lat: 46.5,
        max_lon: 9.5,
        has_recorded_trip: false,
    }
}

fn a_collection(id: i64, name: &str, tracks: Vec<CollectionTrackMark>) -> CollectionSummary {
    CollectionSummary {
        id,
        name: name.to_string(),
        activity_type: Some(ActivityType::Cycling),
        waypoint_count: 46,
        track_count: 125,
        tracks,
        waypoint_bounds: None,
        tracks_with_recorded_trip: 0,
    }
}

fn table(
    trips: Vec<TripSummary>,
    collections: Vec<CollectionSummary>,
    picked: &[Picked],
    expanded: &[i64],
) -> String {
    let picked: BTreeSet<Picked> = picked.iter().copied().collect();
    let expanded: BTreeSet<i64> = expanded.iter().copied().collect();
    render(move || {
        let picked = use_signal(|| picked.clone());
        let expanded = use_signal(|| expanded.clone());
        rsx! { ListTable { trips: trips.clone(), collections: collections.clone(), picked, expanded } }
    })
}

fn trips_only(trips: Vec<TripSummary>) -> String {
    table(trips, Vec::new(), &[], &[])
}

fn row<'a>(html: &'a str, name: &str) -> &'a str {
    html.split("<tr")
        .find(|row| row.contains(name))
        .unwrap_or_else(|| panic!("no row for {name}: {html}"))
}

// ── A trip's row (US-6, US-35) ───────────────────────────────────────────────

// Exemplar for the component-render layer (ADR-0012, 2026-08-26a): props in,
// HTML string out, no server. US-6: each trip's name, date, distance, ascent
// and duration — asserting the formatting the screen is responsible for, not
// the raw values.
#[test]
fn the_table_shows_a_row_per_trip() {
    let html = trips_only(vec![
        a_trip(1, "Oslo Hills Walk"),
        a_trip(2, "Inn Valley Ride"),
    ]);

    assert!(html.contains("Oslo Hills Walk"), "{html}");
    assert!(html.contains("Inn Valley Ride"), "{html}");
    assert!(html.contains("2026-07-11"), "{html}");
    assert!(
        !html.contains("2026-07-10"),
        "dated where it started: {html}"
    );
    assert!(html.contains("12.35 km"), "{html}");
    assert!(html.contains("410 m"), "{html}");
    assert!(html.contains("01:02:05"), "{html}");
    // The activity as its icon, named on hover.
    assert!(html.contains(r#"class="activity-icon""#), "{html}");
    assert!(html.contains(r#"title="Hiking""#), "{html}");
    assert!(!html.contains(">Hiking<"), "{html}");
}

// US-42 made the detail screen part of the SPA: a row must reach it through
// the router rather than by leaving the app for a page load.
#[test]
fn a_rows_name_links_to_the_trips_detail_screen() {
    let html = trips_only(vec![a_trip(7, "Oslo Hills Walk")]);

    assert!(html.contains("href=\"/trips/7\""), "{html}");
    assert!(
        !html.contains("noopener"),
        "an in-app route, not an external link out of the SPA: {html}"
    );
}

// US-35's Privacy column. A linked trip's privacy is a stored value the row
// displays, so the component layer covers it (ADR-0012).
#[test]
fn the_privacy_column_shows_a_linked_trips_privacy() {
    let html = trips_only(vec![TripSummary {
        privacy_status: Some(KomootPrivacy::Public),
        ..a_trip(1, "Komoot Trip")
    }]);

    assert!(html.contains("Privacy"), "{html}");
    assert!(html.contains(KomootPrivacy::Public.label()), "{html}");
}

#[test]
fn a_privacy_komoot_reported_unmappably_shows_as_unknown() {
    // The archive never pretends to know a privacy it couldn't map
    // (ADR-0021); it shows it, and the push side leaves it alone.
    let html = trips_only(vec![TripSummary {
        privacy_status: Some(KomootPrivacy::Unknown),
        ..a_trip(1, "Odd Privacy")
    }]);

    assert!(html.contains(KomootPrivacy::Unknown.label()), "{html}");
}

#[test]
fn a_trip_that_never_came_from_komoot_shows_no_privacy() {
    let html = trips_only(vec![a_trip(1, "Local Trip")]);

    assert!(
        html.contains("Privacy"),
        "the column is always there: {html}"
    );
    assert!(!html.contains(KomootPrivacy::Public.label()), "{html}");
    assert!(!html.contains(KomootPrivacy::Private.label()), "{html}");
}

#[test]
fn a_trip_missing_optional_stats_shows_dashes_not_blanks() {
    let html = trips_only(vec![TripSummary {
        start_date: None,
        ascent_m: None,
        duration_secs: None,
        ..a_trip(1, "Bare Trip")
    }]);

    assert!(row(&html, "Bare Trip").contains("—"), "{html}");
}

// ── Kinds as icons (US-90) ───────────────────────────────────────────────────

#[test]
fn us90_each_row_names_its_kind_by_an_icon_in_its_line_style() {
    let planned = TripSummary {
        trip_kind: TripKind::Planned,
        ..a_trip(2, "Dream Route")
    };
    let html = table(
        vec![a_trip(1, "Oslo Hills Walk"), planned],
        vec![a_collection(3, "Alpencross", vec![a_track(4, "Etappe 7")])],
        &[],
        &[3],
    );

    let recorded = row(&html, "Oslo Hills Walk");
    assert!(recorded.contains(r#"aria-label="Recorded trip""#), "{html}");
    assert!(
        recorded.contains(r##"stroke="#b2182b""##),
        "in its activity's color: {html}"
    );
    assert!(recorded.contains(r#"stroke-dasharray="none""#), "{html}");
    let planned = row(&html, "Dream Route");
    assert!(planned.contains(r#"title="Planned trip""#), "{html}");
    assert!(planned.contains(r#"stroke-dasharray="8 6""#), "{html}");
    assert!(row(&html, "Alpencross").contains(r#"title="Collection""#));
    let track = row(&html, "Etappe 7");
    assert!(track.contains(r#"title="Collection track""#), "{html}");
    assert!(track.contains(r#"stroke-dasharray="1 7""#), "{html}");
    assert!(!html.contains(">Recorded<"), "an icon, not a word: {html}");
}

// ── Collections as groups (US-90) ────────────────────────────────────────────

#[test]
fn us90_trips_come_first_then_the_collections() {
    let html = table(
        vec![a_trip(1, "Oslo Hills Walk")],
        vec![a_collection(2, "Alpencross", Vec::new())],
        &[],
        &[],
    );

    let trip = html.find("Oslo Hills Walk").expect("listed");
    let collection = html.find("Alpencross").expect("listed");
    assert!(trip < collection, "{html}");
}

#[test]
fn us90_a_collection_is_a_group_row_folded_by_default() {
    let collection = a_collection(
        3,
        "Alpencross",
        vec![a_track(4, "Etappe 6"), a_track(5, "Etappe 7")],
    );

    let html = table(Vec::new(), vec![collection], &[], &[]);

    let group = row(&html, "Alpencross");
    assert!(group.contains(r#"class="collection-group""#), "{html}");
    assert!(group.contains("href=\"/collections/3?\""), "{html}");
    assert!(group.contains("2 of 125 tracks · 46 waypoints"), "{html}");
    assert!(group.contains(r#"aria-expanded="false""#), "{html}");
    assert!(!html.contains("Etappe 7"), "its tracks wait: {html}");
}

#[test]
fn us90_a_folded_out_group_lists_its_tracks_under_it() {
    let collection = a_collection(
        3,
        "Alpencross",
        vec![a_track(4, "Etappe 6"), a_track(5, "Etappe 7")],
    );

    let html = table(Vec::new(), vec![collection], &[], &[3]);

    assert!(
        row(&html, "Alpencross").contains(r#"aria-expanded="true""#),
        "{html}"
    );
    let six = html.find("Etappe 6").expect("listed");
    let seven = html.find("Etappe 7").expect("listed");
    assert!(six < seven, "in file order: {html}");
    let track = row(&html, "Etappe 7");
    assert!(track.contains("href=\"/collection-tracks/5\""), "{html}");
    assert!(track.contains("51.00 km"), "{html}");
    assert!(track.contains("680 m"), "{html}");
    assert_eq!(track.matches("—").count(), 2, "no date or duration: {html}");
}

#[test]
fn us90_a_group_with_no_matching_track_has_nothing_to_fold_out() {
    let html = table(
        Vec::new(),
        vec![a_collection(3, "Passes", Vec::new())],
        &[],
        &[],
    );

    let group = row(&html, "Passes");
    assert!(group.contains("disabled"), "{html}");
    assert!(group.contains("0 of 125 tracks"), "{html}");
}

#[test]
fn us90_a_group_whose_tracks_all_match_says_how_many_it_holds() {
    let collection = CollectionSummary {
        track_count: 1,
        waypoint_count: 0,
        ..a_collection(3, "Single", vec![a_track(4, "Tour")])
    };

    assert_eq!(group_counts(&collection), "1 track");
}

// ── Selecting (US-34, US-90) ─────────────────────────────────────────────────

#[test]
fn us90_every_row_is_selectable_and_so_are_all_the_page_lists() {
    let html = table(
        vec![a_trip(1, "Oslo Hills Walk")],
        vec![a_collection(3, "Alpencross", vec![a_track(4, "Etappe 7")])],
        &[Picked::Track(4)],
        &[3],
    );

    assert_eq!(
        html.matches(r#"aria-label="Select""#).count(),
        3,
        "a trip, a collection, a track: {html}"
    );
    assert!(html.contains(r#"aria-label="Select all""#), "{html}");
    assert!(row(&html, "Etappe 7").contains("checked"), "{html}");
    assert!(!row(&html, "Oslo Hills Walk").contains("checked"), "{html}");
}

#[test]
fn us90_select_all_is_ticked_when_every_row_listed_is_selected() {
    let html = table(
        vec![a_trip(1, "Oslo Hills Walk")],
        vec![a_collection(3, "Alpencross", Vec::new())],
        &[Picked::Trip(Kind::Recorded, 1), Picked::Collection(3)],
        &[],
    );

    let header = html.split_once("</thead>").expect("a header").0;
    assert!(header.contains("checked"), "{html}");
}

// ── Paging (US-63, US-90) ────────────────────────────────────────────────────

#[test]
fn us90_a_page_holds_trips_then_collections_fifty_rows_at_a_time() {
    assert_eq!(page_rows(0, 30, 30), (0..30, 0..20));
    assert_eq!(page_rows(1, 30, 30), (30..30, 20..30));
    assert_eq!(page_rows(0, 70, 5), (0..50, 0..0));
    assert_eq!(page_rows(1, 70, 5), (50..70, 0..5));
    assert_eq!(page_rows(0, 0, 3), (0..0, 0..3));
}

// ── US-99: what has a recorded trip ──────────────────────────────────────────

#[test]
fn us99_a_planned_trip_or_a_track_with_a_recorded_trip_is_marked() {
    let planned = TripSummary {
        trip_kind: TripKind::Planned,
        has_recorded_trip: true,
        ..a_trip(1, "Been there")
    };
    let open = TripSummary {
        trip_kind: TripKind::Planned,
        ..a_trip(2, "Not yet")
    };
    let collection = a_collection(
        3,
        "Alpencross",
        vec![
            CollectionTrackMark {
                has_recorded_trip: true,
                ..a_track(4, "Etappe 6")
            },
            a_track(5, "Etappe 7"),
        ],
    );

    let html = table(vec![planned, open], vec![collection], &[], &[3]);

    for marked in ["Been there", "Etappe 6"] {
        assert!(row(&html, marked).contains("Has a recorded trip"), "{html}");
    }
    for unmarked in ["Not yet", "Etappe 7"] {
        assert!(
            !row(&html, unmarked).contains("Has a recorded trip"),
            "{html}"
        );
    }
}
