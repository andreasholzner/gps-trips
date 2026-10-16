//! US-103 — a proposed link in a page's list of links: marked as proposed,
//! with its coverage, confirmed or dismissed there.

use super::*;
use crate::test_support::{import_sample, render, render_against_archive, serve_test_archive};
use trip_archive_types::{LinkOther, LinkedTrack, LinkedTrip, LinkedWaypoint, Proposal};

const COVERED: Proposal = Proposal {
    plan_covered: Some(0.92),
    trip_on_plan: Some(0.854),
    points_reached: None,
};

fn a_trip(id: i64, proposal: Option<Proposal>) -> LinkedTrip {
    LinkedTrip {
        id,
        name: "Plan".to_string(),
        activity_type: ActivityType::Hiking,
        start_date: None,
        proposal,
    }
}

#[test]
fn us103_a_proposal_is_listed_beside_the_links_with_what_it_was_proposed_on() {
    let links = Links {
        planned_trips: vec![a_trip(2, Some(COVERED))],
        collection_tracks: vec![LinkedTrack {
            id: 3,
            name: "Stage 1".to_string(),
            activity_type: ActivityType::Cycling,
            collection_id: 7,
            collection_name: "Book".to_string(),
            proposal: None,
        }],
        waypoints: vec![LinkedWaypoint {
            id: 4,
            name: Some("Pass".to_string()),
            symbol: None,
            collection_id: 7,
            collection_name: "Book".to_string(),
            proposal: Some(Proposal::default()),
        }],
        ..Links::default()
    };

    let groups = groups(LinkEnd::Trip(1), &links);

    assert_eq!(groups[0].rows[0].proposal, Some(COVERED));
    assert_eq!(groups[1].rows[0].proposal, None);
    assert_eq!(groups[2].rows[0].proposal, Some(Proposal::default()));
}

#[test]
fn us103_coverage_reads_as_shares_of_the_plan_and_the_trip() {
    assert_eq!(
        coverage(&COVERED).as_deref(),
        Some("92 % of the plan · 85 % of the trip")
    );
    let by_points = Proposal {
        points_reached: Some(0.8),
        ..Proposal::default()
    };
    assert_eq!(
        coverage(&by_points).as_deref(),
        Some("80 % of the plan's points reached")
    );
    assert_eq!(coverage(&Proposal::default()), None);
}

#[test]
fn us103_a_proposed_row_is_confirmed_or_dismissed_and_a_link_only_removed() {
    let row = |id, proposal| Row {
        name: format!("Plan {id}"),
        detail: None,
        to: Route::TripDetail { id },
        icon: RowIcon::Activity(ActivityType::Hiking),
        shown: Shown::Line(Kind::Planned, id),
        unlink: Some((1, LinkEnd::Trip(id))),
        proposal,
    };
    let groups = vec![Group {
        title: "Planned trips",
        rows: vec![row(2, Some(COVERED)), row(3, None)],
    }];

    let html = render(move || {
        let shown = use_signal(BTreeSet::new);
        rsx! { LinkGroups { groups: groups.clone(), shown, on_unlink: |_| {}, on_confirm: |_| {} } }
    });

    assert_eq!(html.matches(r#"class="proposed""#).count(), 1, "{html}");
    assert!(
        html.contains("92 % of the plan · 85 % of the trip"),
        "{html}"
    );
    assert_eq!(html.matches(r#"class="confirm""#).count(), 1, "{html}");
    assert!(html.contains("Confirm the link to Plan 2"), "{html}");
    assert!(
        html.contains("Dismiss the proposed link to Plan 2"),
        "{html}"
    );
    assert!(html.contains("Remove the link to Plan 3"), "{html}");
}

#[tokio::test]
async fn us103_a_trips_section_lists_what_the_archive_proposed() {
    let (archive, _dir) = serve_test_archive().await;
    import_sample(&archive, &[("kind", "planned"), ("name", "The plan")]).await;
    let monday = import_sample(&archive, &[]).await;

    let html = render_against_archive(
        &archive,
        move || {
            let overlays = use_signal(Overlays::default);
            rsx! { LinkedTo { end: LinkEnd::Trip(monday), overlays } }
        },
        |html| html.contains(r#"id="links""#),
    )
    .await;

    assert!(html.contains("The plan"), "{html}");
    assert!(html.contains(r#"class="proposed""#), "{html}");
    assert!(
        html.contains("100 % of the plan · 100 % of the trip"),
        "{html}"
    );
    assert!(html.contains(r#"class="confirm""#), "{html}");
}

#[test]
fn us103_the_item_a_page_opens_with_is_found_among_its_rows() {
    let links = Links {
        planned_trips: vec![a_trip(2, Some(COVERED))],
        waypoints: vec![LinkedWaypoint {
            id: 4,
            name: Some("Pass".to_string()),
            symbol: None,
            collection_id: 7,
            collection_name: "Book".to_string(),
            proposal: Some(Proposal::default()),
        }],
        ..Links::default()
    };
    let groups = groups(LinkEnd::Trip(1), &links);

    assert_eq!(
        to_tick(LinkOther::Trip(2), &groups),
        Some(Shown::Line(Kind::Planned, 2))
    );
    assert_eq!(
        to_tick(LinkOther::Waypoint(4), &groups),
        Some(Shown::Waypoint(4))
    );
    assert_eq!(to_tick(LinkOther::CollectionTrack(2), &groups), None);
    assert_eq!(to_tick(LinkOther::Trip(3), &groups), None);
}

#[tokio::test]
async fn us103_a_trip_page_opens_with_the_other_end_ticked() {
    let (archive, _dir) = serve_test_archive().await;
    let plan = import_sample(&archive, &[("kind", "planned"), ("name", "The plan")]).await;
    let monday = import_sample(&archive, &[]).await;

    let html = render_against_archive(
        &archive,
        move || {
            let overlays = use_signal(Overlays::default);
            rsx! { LinkedTo { end: LinkEnd::Trip(monday), overlays, show: Some(LinkOther::Trip(plan)) } }
        },
        |html| html.contains("checked"),
    )
    .await;

    assert_eq!(html.matches("checked").count(), 1, "{html}");
}
