use super::*;
use crate::test_support::render;
use trip_archive_types::ReviewMatch;

use ImportAction::{Add, Replace, Skip};

fn item(file: usize, index: usize, name: &str, proposed: ImportAction) -> ReviewItem {
    ReviewItem {
        file,
        index,
        name: Some(name.to_string()),
        matched: (proposed != Add).then(|| ReviewMatch {
            name: Some(name.to_string()),
            file: None,
        }),
        proposed,
        action: proposed,
        choices: if proposed == Add {
            vec![Add, Skip]
        } else {
            vec![Skip, Replace, Add]
        },
    }
}

fn a_review(collection_id: Option<i64>) -> CollectionImportReview {
    CollectionImportReview {
        staging_id: 3,
        collection_id,
        suggested_name: "Lakes & Passes".to_string(),
        suggested_description: "Bruckmann".to_string(),
        files: vec!["part-1.gpx".to_string(), "part-2.gpx".to_string()],
        replace_all: false,
        tracks: vec![
            item(0, 0, "Stage 1", Skip),
            item(0, 1, "Stage 2", Replace),
            item(1, 0, "Stage 4", Add),
        ],
        waypoints: vec![item(1, 0, "Hut", Skip)],
        held_tracks: vec![HeldItem {
            id: 9,
            name: Some("Stage 3".to_string()),
            remove: false,
        }],
        held_waypoints: vec![],
        held_digest: "abc".to_string(),
    }
}

fn rendered(review: CollectionImportReview) -> String {
    render(move || {
        rsx! {
            ReviewStep {
                review: review.clone(),
                decisions: ImportDecisions::default(),
                on_decide: move |_| {},
                on_confirm: move |_| {},
                on_cancel: move |_| {},
            }
        }
    })
}

// ── The decisions ────────────────────────────────────────────────────────────

#[test]
fn us88_a_later_choice_for_an_item_replaces_the_earlier_one() {
    let once = decide(&ImportDecisions::default(), Items::Tracks, 0, 1, Skip);
    let twice = decide(&once, Items::Tracks, 0, 1, Add);
    let other = decide(&twice, Items::Waypoints, 0, 1, Replace);

    assert_eq!(
        other.tracks,
        [ItemDecision {
            file: 0,
            index: 1,
            action: Add,
        }]
    );
    assert_eq!(other.waypoints.len(), 1);
}

#[test]
fn us88_a_held_item_is_removed_and_kept_again() {
    let removed = set_removed(&ImportDecisions::default(), Items::Tracks, 9, true);
    assert_eq!(removed.remove_tracks, [9]);
    assert_eq!(
        set_removed(&removed, Items::Tracks, 9, false).remove_tracks,
        Vec::<i64>::new()
    );
}

#[test]
fn us88_a_confirmation_sends_a_new_collections_details_only() {
    let form = CollectionForm {
        name: "  Lakes  ".to_string(),
        description: " Bruckmann ".to_string(),
        activity: "hiking".to_string(),
    };
    let decisions = decide(&ImportDecisions::default(), Items::Tracks, 0, 0, Add);

    let new = confirmation(&a_review(None), &decisions, &form);
    let existing = confirmation(&a_review(Some(5)), &decisions, &form);

    assert_eq!(new.held_digest, "abc");
    assert_eq!(new.decisions, decisions);
    assert_eq!(new.name.as_deref(), Some("Lakes"));
    assert_eq!(new.description.as_deref(), Some("Bruckmann"));
    assert_eq!(new.activity_type.as_deref(), Some("hiking"));
    assert_eq!(existing.name, None);
    assert_eq!(existing.activity_type, None);
}

#[test]
fn us88_a_blank_name_asks_for_the_archives_own() {
    let form = CollectionForm {
        name: "  ".to_string(),
        description: String::new(),
        activity: String::new(),
    };

    assert_eq!(
        confirmation(&a_review(None), &ImportDecisions::default(), &form).name,
        None
    );
}

#[test]
fn us88_a_match_says_whether_the_collection_or_an_earlier_file_holds_it() {
    let files = ["a.gpx".to_string(), "b.gpx".to_string()];
    let mut from_file = item(1, 0, "Hut", Skip);
    from_file.matched = Some(ReviewMatch {
        name: Some("Hut".to_string()),
        file: Some(0),
    });

    assert_eq!(
        matched_text(&item(0, 0, "Stage 1", Skip), &files),
        "Stage 1 in the collection"
    );
    assert_eq!(matched_text(&from_file, &files), "Hut from a.gpx");
    assert_eq!(matched_text(&item(0, 0, "New", Add), &files), "—");
}

// ── The step ─────────────────────────────────────────────────────────────────

#[test]
fn us88_every_item_is_listed_with_its_file_and_its_choices() {
    let html = rendered(a_review(Some(5)));

    assert!(html.contains(r#"id="review-tracks""#), "{html}");
    assert!(html.contains(r#"id="review-waypoints""#), "{html}");
    assert!(html.contains(r#"id="track-1-0""#), "{html}");
    assert!(html.contains(r#"id="waypoint-1-0""#), "{html}");
    assert!(html.contains("part-2.gpx"), "{html}");
    assert!(html.contains("Stage 2 in the collection"), "{html}");
    assert!(html.contains("Replace (proposed)"), "{html}");
    assert!(html.contains("From part-1.gpx, part-2.gpx."), "{html}");
}

#[test]
fn us88_what_no_file_holds_is_offered_to_keep_or_remove() {
    let html = rendered(a_review(Some(5)));

    assert!(html.contains(r#"id="held-track-9""#), "{html}");
    assert!(html.contains("Keep (proposed)"), "{html}");
    assert!(html.contains("Stage 3"), "{html}");
}

#[test]
fn us88_an_existing_collection_can_be_replaced_whole_and_keeps_its_details() {
    let html = rendered(a_review(Some(5)));

    assert!(html.contains(r#"id="import-replace-all""#), "{html}");
    assert!(!html.contains(r#"id="collection-name-field""#), "{html}");
    assert!(html.contains("Add to collection"), "{html}");
}

#[test]
fn us88_a_new_collection_of_several_files_takes_its_details_from_the_first() {
    let html = rendered(a_review(None));

    assert!(!html.contains(r#"id="import-replace-all""#), "{html}");
    assert!(html.contains(r#"value="Lakes &#38; Passes""#), "{html}");
    assert!(html.contains("Import collection"), "{html}");
    assert!(html.contains(r#"id="import-cancel""#), "{html}");
}
