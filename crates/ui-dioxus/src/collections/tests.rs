//! The Collections screen (US-97). What the table, the filter's matching,
//! the paging and the empty states say is asserted on rendered HTML; one
//! test reads the collections from a real server. Typing in the filter and
//! reloading are the browser layer's (`tests/browser/collections_page.spec.mjs`).

use super::*;
use crate::test_support::{
    import_collection, render, render_against_archive, serve_test_archive, COLLECTION_GPX,
};

fn a_collection(id: i64, name: &str) -> CollectionSummary {
    CollectionSummary {
        id,
        name: name.to_string(),
        activity_type: None,
        waypoint_count: 0,
        track_count: 0,
        tracks: Vec::new(),
        waypoint_bounds: None,
        tracks_with_recorded_trip: 0,
    }
}

fn names<'a>(collections: &[&'a CollectionSummary]) -> Vec<&'a str> {
    collections.iter().map(|c| c.name.as_str()).collect()
}

fn list(collections: Vec<CollectionSummary>, query: &str) -> String {
    let query = query.to_string();
    render(move || {
        let selected = use_signal(BTreeSet::new);
        rsx! { CollectionList { collections: collections.clone(), query: query.clone(), selected } }
    })
}

// ── Finding a collection ─────────────────────────────────────────────────────

#[test]
fn us97_the_filter_keeps_the_names_containing_it_ignoring_case_and_spaces() {
    let collections = vec![
        a_collection(1, "Alpencross"),
        a_collection(2, "Lakes of the ALPS"),
        a_collection(3, "Norway"),
    ];

    assert_eq!(
        names(&matching(&collections, " alp ")),
        ["Alpencross", "Lakes of the ALPS"]
    );
    assert_eq!(names(&matching(&collections, "")).len(), 3);
    assert!(matching(&collections, "oslo").is_empty());
}

// ── The table ────────────────────────────────────────────────────────────────

#[test]
fn us97_each_collection_links_to_its_page() {
    let html = list(vec![a_collection(3, "Alpencross")], "");

    assert!(html.contains(r#"href="/collections/3""#), "{html}");
    // Exactly the Collections tab's table (US-87).
    assert!(html.contains("<th>Waypoints</th>"), "{html}");
}

#[test]
fn us97_the_list_shows_only_the_collections_the_filter_keeps() {
    let html = list(
        vec![a_collection(1, "Alpencross"), a_collection(2, "Norway")],
        "NOR",
    );

    assert!(html.contains(">Norway<"), "{html}");
    assert!(!html.contains("Alpencross"), "{html}");
}

#[test]
fn us97_fifty_collections_make_a_page() {
    let collections = (0..51)
        .map(|n| a_collection(n, &format!("Book {n:02}")))
        .collect();

    let html = list(collections, "");

    let table = html.split_once("<tbody").expect("a table").1;
    assert_eq!(table.matches("<tr").count(), 50, "{html}");
    assert!(!table.contains("Book 50"), "{html}");
    assert!(html.contains("Page 1 of 2"), "{html}");
}

#[test]
fn us97_without_collections_the_screen_points_to_the_import() {
    let html = list(Vec::new(), "");

    assert!(html.contains(NO_COLLECTIONS), "{html}");
}

#[test]
fn us97_a_filter_matching_no_collection_says_so() {
    let html = list(Vec::new(), " oslo ");

    assert!(html.contains("No collection contains “oslo”."), "{html}");
    assert!(!html.contains("<table"), "{html}");
}

// ── The screen ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn us97_the_screen_lists_every_collection_alphabetically_narrowed_by_the_url() {
    let (archive, _dir) = serve_test_archive().await;
    for name in ["Lakes", "alpencross", "Lakes North", "Passes"] {
        import_collection(&archive, COLLECTION_GPX, &[("name", name)]).await;
    }

    let html = render_against_archive(
        &archive,
        || rsx! { Collections {} },
        |html| html.contains("<tbody"),
    )
    .await;

    let position = |name: &str| html.find(&format!(">{name}<")).expect("listed");
    assert!(position("alpencross") < position("Lakes"), "{html}");
    assert!(position("Lakes North") < position("Passes"), "{html}");
    // No map, and none of the trip list's other filters.
    assert!(!html.contains("region-map"), "{html}");
    assert!(!html.contains("filter-toolbar"), "{html}");
    // Collections are selected to share (US-94) or tag (US-100) them;
    // nothing is yet.
    assert_eq!(html.matches(r#"type="checkbox""#).count(), 5, "{html}");
    assert!(!html.contains("Share selected collections"), "{html}");
    assert!(!html.contains("Tag selected"), "{html}");

    let html = render_against_archive(
        &archive,
        || rsx! { Collections { view: NameFilter { q: "LAKES".to_string() } } },
        |html| html.contains("<tbody"),
    )
    .await;

    assert!(html.contains(">Lakes North<"), "{html}");
    assert!(!html.contains(">Passes<"), "{html}");
    assert!(
        html.contains(r#"value="LAKES""#),
        "the field shows the filter: {html}"
    );
}
