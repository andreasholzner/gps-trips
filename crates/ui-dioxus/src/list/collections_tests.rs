//! US-87 — the trip list's Collections tab, rendered against a real server
//! (ADR-0012). Switching tabs and typing are the browser layer's
//! (`tests/browser/collections_tab.spec.mjs`).

use super::*;
use crate::test_support::{
    import_collection, import_sample, render, render_against_archive, serve_test_archive,
    COLLECTION_GPX,
};

fn on_the_collections_tab(q: &str) -> Filters {
    Filters {
        tab: ListTab::Collections,
        q: q.to_string(),
        ..Default::default()
    }
}

#[tokio::test]
async fn us87_the_tab_lists_every_collection_alphabetically_and_no_trips() {
    let (archive, _dir) = serve_test_archive().await;
    import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
    for name in ["Lakes", "alpencross"] {
        import_collection(&archive, COLLECTION_GPX, &[("name", name)]).await;
    }

    let html = render_against_archive(
        &archive,
        || rsx! { TripList { filters: on_the_collections_tab("") } },
        |html| html.contains("2 collections"),
    )
    .await;

    let alpencross = html.find("alpencross").expect("listed");
    let lakes = html.find("Lakes").expect("listed");
    assert!(alpencross < lakes, "alphabetically: {html}");
    assert!(!html.contains("Oslo Hills Walk"), "{html}");
    // Collections are selected to share them (US-94), apart from the trips'
    // selection; nothing is selected yet.
    assert_eq!(html.matches(r#"aria-label="Select""#).count(), 2, "{html}");
    assert!(html.contains(r#"aria-label="Select all""#), "{html}");
    assert!(!html.contains("Share selected"), "{html}");
}

#[tokio::test]
async fn us87_the_counts_say_how_many_match_and_how_many_there_are() {
    let (archive, _dir) = serve_test_archive().await;
    for name in ["Lakes", "Passes", "Lakes North"] {
        import_collection(&archive, COLLECTION_GPX, &[("name", name)]).await;
    }

    let html = render_against_archive(
        &archive,
        || rsx! { TripList { filters: on_the_collections_tab("lakes") } },
        |html| html.contains("3 collections"),
    )
    .await;

    assert!(html.contains("2 of 3 collections"), "{html}");
    assert!(!html.contains("Passes"), "{html}");
}

#[tokio::test]
async fn us87_an_archive_without_collections_says_so() {
    let (archive, _dir) = serve_test_archive().await;
    import_sample(&archive, &[]).await;

    let html = render_against_archive(
        &archive,
        || rsx! { TripList { filters: on_the_collections_tab("") } },
        |html| !html.contains("Loading"),
    )
    .await;

    assert!(html.contains("No collections yet."), "{html}");
    assert!(
        html.contains("Import a GPX file as a collection."),
        "{html}"
    );
}

#[tokio::test]
async fn us87_a_search_matching_no_collection_says_so() {
    let (archive, _dir) = serve_test_archive().await;
    import_collection(&archive, COLLECTION_GPX, &[("name", "Lakes")]).await;

    let html = render_against_archive(
        &archive,
        || rsx! { TripList { filters: on_the_collections_tab("nothing") } },
        |html| !html.contains("Loading"),
    )
    .await;

    assert!(
        html.contains("No collections match your filters."),
        "{html}"
    );
}

#[test]
fn us87_a_long_list_of_collections_says_which_rows_are_shown() {
    let html = render(|| {
        rsx! { ListCounts { shown: 60, total: 60, tab: ListTab::Collections, page: 1 } }
    });

    assert!(html.contains("60 collections · showing 51–60"), "{html}");
}

#[tokio::test]
async fn us87_a_long_list_of_collections_is_shown_a_page_at_a_time() {
    let (archive, _dir) = serve_test_archive().await;
    for n in 0..51 {
        import_collection(
            &archive,
            COLLECTION_GPX,
            &[("name", &format!("Book {n:02}"))],
        )
        .await;
    }

    let html = render_against_archive(
        &archive,
        || rsx! { TripList { filters: on_the_collections_tab("") } },
        |html| html.contains("51 collections"),
    )
    .await;

    let table = html.split_once("<tbody").expect("a table").1;
    assert_eq!(table.matches("<tr").count(), 50, "{html}");
    assert!(
        table.contains("Book 49") && !table.contains("Book 50"),
        "{html}"
    );
    assert!(html.contains("51 collections · showing 1–50"), "{html}");
    assert!(html.contains("Page 1 of 2"), "{html}");
}

// What was read for another tab is not shown, nor counted against this
// one's total: right after a switch, "3 of 312 collections" would pair the
// new list with the old tab's total until the new one arrives.
#[test]
fn us87_only_what_was_read_for_the_tab_showing_is_used() {
    let read = (ListTab::Recorded, 312);

    assert_eq!(for_tab(Some(&read), ListTab::Recorded), Some(&312));
    assert_eq!(for_tab(Some(&read), ListTab::Collections), None);
    assert_eq!(for_tab(Some(&read), ListTab::Planned), None);
    assert_eq!(for_tab::<usize>(None, ListTab::Recorded), None);
}
