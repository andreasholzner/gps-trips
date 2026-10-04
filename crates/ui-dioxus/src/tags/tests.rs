//! The Tags screen (US-83). What the rows, the filter, the paging and the
//! confirmation say is asserted on rendered HTML; one test reads the tags
//! from a real server. Typing in the filter, deleting and creating are the
//! browser layer's (`tests/browser/tags.spec.mjs`).

use super::*;
use crate::test_support::{
    import_sample, render, render_against_archive, serve_test_archive, tag_trip,
};
use trip_archive_types::TagShare;

fn a_tag(id: i64, name: &str, trips: i64, recorded: i64) -> TagOverview {
    TagOverview {
        id,
        name: name.to_string(),
        trip_count: trips,
        recorded_trip_count: recorded,
        shares: Vec::new(),
    }
}

fn a_share(label: Option<&str>, tags: &[&str]) -> TagShare {
    TagShare {
        id: 7,
        label: label.map(str::to_string),
        tags: tags.iter().map(|tag| tag.to_string()).collect(),
    }
}

fn table(tags: Vec<TagOverview>) -> String {
    render(
        move || rsx! { TagTable { tags: tags.clone(), query: String::new(), on_changed: |_| {} } },
    )
}

// ── Rows ─────────────────────────────────────────────────────────────────────

#[test]
fn us83_a_tag_with_a_recorded_trip_links_to_its_summary_with_its_count() {
    let html = table(vec![a_tag(1, "alps", 3, 1)]);

    let summary = Route::Summary {
        view: SummaryView::of("alps"),
    };
    assert!(html.contains(&format!(r#"href="{summary}""#)), "{html}");
    assert!(html.contains(">alps</a> (3)"), "{html}");
}

#[test]
fn us83_a_tag_without_a_recorded_trip_is_plain_text() {
    let html = table(vec![a_tag(1, "plans", 2, 0), a_tag(2, "unused", 0, 0)]);

    assert!(!html.contains("href"), "{html}");
    assert!(html.contains("plans (2)"), "{html}");
    assert!(html.contains("unused (0)"), "{html}");
}

#[test]
fn us83_a_shared_tag_carries_the_share_icon_naming_its_shares() {
    let mut alps = a_tag(1, "alps", 1, 1);
    alps.shares = vec![
        a_share(Some("Summer"), &["alps"]),
        a_share(None, &["alps", "norway"]),
    ];
    let html = table(vec![alps, a_tag(2, "norway", 1, 1)]);

    assert_eq!(html.matches("🔗").count(), 1, "{html}");
    let shares = Route::Shares {}.to_string();
    assert!(html.contains(&format!(r#"href="{shares}""#)), "{html}");
    assert!(
        html.contains(r#"title="Shared in “Summer”; alps, norway""#),
        "{html}"
    );
}

#[test]
fn us83_every_row_offers_deleting_but_nothing_is_armed() {
    let html = table(vec![a_tag(1, "alps", 1, 1), a_tag(2, "norway", 1, 1)]);

    assert_eq!(html.matches(">Delete<").count(), 2, "{html}");
    assert!(!html.contains("cannot be undone"), "{html}");
}

#[test]
fn us83_the_table_shows_fifty_tags_a_page() {
    let tags: Vec<TagOverview> = (0..51)
        .map(|id| a_tag(id, &format!("tag{id:02}"), 0, 0))
        .collect();

    let html = table(tags);

    assert!(html.contains("tag49 (0)"), "{html}");
    assert!(!html.contains("tag50 (0)"), "{html}");
    assert!(html.contains("Page 1 of 2"), "{html}");
}

#[test]
fn us83_an_empty_archive_and_an_unmatched_filter_say_so() {
    let empty = table(Vec::new());
    let unmatched = render(|| {
        rsx! { TagTable { tags: Vec::new(), query: "xyz".to_string(), on_changed: |_| {} } }
    });

    assert!(empty.contains("No tags yet."), "{empty}");
    assert!(unmatched.contains("No tag contains “xyz”."), "{unmatched}");
}

// ── Finding a tag ────────────────────────────────────────────────────────────

#[test]
fn us83_the_filter_keeps_the_tags_containing_it_ignoring_case() {
    let tags = vec![
        a_tag(1, "alps", 0, 0),
        a_tag(2, "norway", 0, 0),
        a_tag(3, "salpeter", 0, 0),
    ];

    let names = |query: &str| -> Vec<String> {
        matching(&tags, query)
            .into_iter()
            .map(|tag| tag.name)
            .collect()
    };
    assert_eq!(names(" ALP "), vec!["alps", "salpeter"]);
    assert_eq!(names(""), vec!["alps", "norway", "salpeter"]);
}

// ── Deleting a tag ───────────────────────────────────────────────────────────

fn confirmation(tag: TagOverview) -> String {
    render(move || {
        rsx! {
            ConfirmDeleteTag {
                tag: tag.clone(),
                on_confirm: |_| {},
                on_cancel: |_| {},
            }
        }
    })
}

#[test]
fn us83_the_confirmation_names_the_tag_and_says_no_trip_goes() {
    let html = confirmation(a_tag(1, "alps", 3, 3));

    assert!(
        html.contains("Delete the tag “alps”? It comes off its 3 trips; no trip is deleted. This cannot be undone."),
        "{html}"
    );
    assert!(html.contains("Cancel"), "{html}");
}

#[test]
fn us83_the_confirmation_says_what_happens_to_each_share() {
    let mut alps = a_tag(1, "alps", 1, 1);
    alps.shares = vec![
        a_share(Some("Summer"), &["norway", "alps"]),
        a_share(None, &["alps"]),
    ];

    let html = confirmation(alps);

    assert!(
        html.contains("The share “Summer” is narrowed to norway."),
        "{html}"
    );
    assert!(html.contains("The share alps is stopped."), "{html}");
}

#[test]
fn us83_the_trips_are_counted_in_words_the_owner_reads() {
    assert!(delete_question(&a_tag(1, "a", 0, 0)).contains("No trip carries it."));
    assert!(delete_question(&a_tag(1, "a", 1, 0)).contains("It comes off its one trip;"));
}

// ── Against the archive ──────────────────────────────────────────────────────

#[tokio::test]
async fn us83_the_screen_lists_the_archives_tags_narrowed_by_the_url() {
    let (archive, _dir) = serve_test_archive().await;
    let trip = import_sample(&archive, &[]).await;
    tag_trip(&archive, trip, "norway").await;
    tag_trip(&archive, trip, "alps").await;
    tag_trip(&archive, trip, "oslo").await;

    let html = render_against_archive(
        &archive,
        || rsx! { Tags { view: TagsView { q: "o".to_string() } } },
        |html| html.contains("norway"),
    )
    .await;

    assert!(html.contains(r#"value="o""#), "{html}");
    let norway = html.find(">norway<").unwrap();
    let oslo = html.find(">oslo<").unwrap();
    assert!(norway < oslo, "{html}");
    assert!(!html.contains(">alps<"), "{html}");
    // Creating sits below the table.
    let create = html.find(r#"id="new-tag-name""#).expect("the create field");
    assert!(oslo < create, "{html}");
}
