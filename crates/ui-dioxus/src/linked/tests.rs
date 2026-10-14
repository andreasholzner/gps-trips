//! US-99 — what a page lists of its links, and removing one from it.

use super::*;
use crate::test_support::{
    import_collection, import_sample, render, render_against_archive, serve_test_archive,
    COLLECTION_GPX,
};
use trip_archive_types::{
    ActivityType, CreateLinks, LinkedTrack, LinkedTrip, LinkedWaypoint, SharedLinkedTrip,
};

fn a_trip(id: i64, name: &str, date: Option<&str>) -> LinkedTrip {
    LinkedTrip {
        id,
        name: name.to_string(),
        activity_type: ActivityType::Hiking,
        start_date: date.map(str::to_string),
    }
}

fn a_track(id: i64) -> LinkedTrack {
    LinkedTrack {
        id,
        name: "Stage 1".to_string(),
        activity_type: ActivityType::Cycling,
        collection_id: 7,
        collection_name: "Book".to_string(),
    }
}

fn a_waypoint(id: i64, name: Option<&str>) -> LinkedWaypoint {
    LinkedWaypoint {
        id,
        name: name.map(str::to_string),
        symbol: Some("Summit".to_string()),
        collection_id: 7,
        collection_name: "Book".to_string(),
    }
}

#[test]
fn us99_a_trips_page_names_every_link_from_the_trip() {
    let links = Links {
        planned_trips: vec![a_trip(2, "Plan", None)],
        collection_tracks: vec![a_track(3)],
        waypoints: vec![a_waypoint(4, None)],
        ..Links::default()
    };

    let groups = groups(LinkEnd::Trip(1), &links);

    let titles: Vec<&str> = groups.iter().map(|group| group.title).collect();
    assert_eq!(titles, ["Planned trips", "Collection tracks", "Waypoints"]);
    assert_eq!(groups[0].rows[0].unlink, Some((1, LinkEnd::Trip(2))));
    assert_eq!(groups[0].rows[0].to.to_string(), "/trips/2");
    assert_eq!(
        groups[1].rows[0].unlink,
        Some((1, LinkEnd::CollectionTrack(3)))
    );
    assert_eq!(groups[1].rows[0].detail.as_deref(), Some("Book"));
    assert_eq!(groups[2].rows[0].name, UNNAMED);
    assert_eq!(groups[2].rows[0].symbol, Some(Some("Summit".to_string())));
    assert_eq!(groups[2].rows[0].to.to_string(), "/waypoints/4");
}

#[test]
fn us99_a_tracks_page_names_each_link_from_the_trip_it_lists() {
    let links = Links {
        recorded_trips: vec![a_trip(1, "Monday", Some("2026-07-06"))],
        planned_trips: vec![a_trip(2, "Plan", None)],
        ..Links::default()
    };

    let groups = groups(LinkEnd::CollectionTrack(3), &links);

    assert_eq!(groups[0].title, "Recorded trips");
    assert_eq!(groups[0].rows[0].detail.as_deref(), Some("2026-07-06"));
    assert_eq!(
        groups[0].rows[0].unlink,
        Some((1, LinkEnd::CollectionTrack(3)))
    );
    assert_eq!(
        groups[1].rows[0].unlink,
        Some((2, LinkEnd::CollectionTrack(3)))
    );
}

#[test]
fn us99_nothing_linked_lists_nothing() {
    assert!(groups(LinkEnd::Waypoint(1), &Links::default()).is_empty());
    let html = render(|| rsx! { LinkGroups { groups: Vec::new(), on_unlink: |_| {} } });
    assert!(!html.contains(r#"id="links""#), "{html}");
}

#[test]
fn us99_a_recipient_reaches_the_shares_own_pages_and_removes_nothing() {
    let links = SharedLinks {
        planned_trips: vec![SharedLinkedTrip {
            id: 2,
            name: "Plan".to_string(),
            activity_type: ActivityType::Hiking,
            start_date: None,
        }],
        ..SharedLinks::default()
    };

    let groups = shared_groups("abc", &links);

    assert_eq!(groups[0].rows[0].to.to_string(), "/s/abc/trips/2");
    assert_eq!(groups[0].rows[0].unlink, None);
    let html = render(move || rsx! { LinkGroups { groups: groups.clone(), on_unlink: |_| {} } });
    assert!(html.contains("Plan"), "{html}");
    assert!(!html.contains("<button"), "{html}");
}

// The section against a real archive: what the trip is linked to, read and
// listed (ADR-0012).
#[tokio::test]
async fn us99_a_trips_section_lists_what_it_is_linked_to() {
    let (archive, _dir) = serve_test_archive().await;
    let monday = import_sample(&archive, &[]).await;
    let plan = import_sample(&archive, &[("kind", "planned"), ("name", "The plan")]).await;
    let (_, tracks) = import_collection(&archive, COLLECTION_GPX, &[("name", "Book")]).await;
    api::create_links(
        &archive,
        &CreateLinks {
            trip_ids: vec![monday, plan],
            collection_track_ids: vec![tracks[0]],
        },
    )
    .await
    .unwrap();

    let html = render_against_archive(
        &archive,
        move || rsx! { LinkedTo { end: LinkEnd::Trip(monday) } },
        |html| html.contains(r#"id="links""#),
    )
    .await;

    assert!(html.contains("<h3>Planned trips</h3>"), "{html}");
    assert!(html.contains("The plan"), "{html}");
    assert!(html.contains("<h3>Collection tracks</h3>"), "{html}");
    assert!(html.contains(" · Book"), "{html}");
    assert!(html.contains(&format!(r#"href="/trips/{plan}""#)), "{html}");
    assert_eq!(html.matches(r#"class="unlink""#).count(), 2, "{html}");
}
