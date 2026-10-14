//! US-99 — what a page lists of its links, showing one on the map, and
//! removing one.

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
    assert!(groups[2].rows[0].icon == RowIcon::Symbol(Some("Summit".to_string())));
    assert!(groups[1].rows[0].icon == RowIcon::Activity(ActivityType::Cycling));
    assert_eq!(groups[0].rows[0].shown, Shown::Line(Kind::Planned, 2));
    assert_eq!(groups[1].rows[0].shown, Shown::Line(Kind::Collections, 3));
    assert_eq!(groups[2].rows[0].shown, Shown::Waypoint(4));
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
    let html = render(|| {
        let shown = use_signal(BTreeSet::new);
        rsx! { LinkGroups { groups: Vec::new(), shown, on_unlink: |_| {} } }
    });
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
    let html = render(move || {
        let shown = use_signal(BTreeSet::new);
        rsx! { LinkGroups { groups: groups.clone(), shown, on_unlink: |_| {} } }
    });
    assert!(html.contains("Plan"), "{html}");
    assert!(!html.contains("<button"), "{html}");
    assert!(
        html.contains("Show Plan on the map"),
        "the recipient may show it too: {html}"
    );
}

// The section against a real archive: what the trip is linked to, read and
// listed (ADR-0012).
#[tokio::test]
async fn us99_a_trips_section_lists_what_it_is_linked_to() {
    let (archive, _dir) = serve_test_archive().await;
    let monday = import_sample(&archive, &[]).await;
    let plan = import_sample(
        &archive,
        &[
            ("kind", "planned"),
            ("name", "The plan"),
            ("activity_type", "hiking"),
        ],
    )
    .await;
    let (_, tracks) = import_collection(
        &archive,
        COLLECTION_GPX,
        &[("name", "Book"), ("activity_type", "cycling")],
    )
    .await;
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
        move || {
            let overlays = use_signal(Overlays::default);
            rsx! { LinkedTo { end: LinkEnd::Trip(monday), overlays } }
        },
        |html| html.contains(r#"id="links""#),
    )
    .await;

    assert!(html.contains("<h3>Planned trips</h3>"), "{html}");
    assert!(html.contains("The plan"), "{html}");
    assert!(html.contains("<h3>Collection tracks</h3>"), "{html}");
    assert!(html.contains(" · Book"), "{html}");
    assert!(html.contains(&format!(r#"href="/trips/{plan}""#)), "{html}");
    assert_eq!(html.matches(r#"class="unlink""#).count(), 2, "{html}");
    // Each with its activity's icon, and a box for the map, not ticked.
    assert_eq!(html.matches("activity-icon").count(), 2, "{html}");
    assert_eq!(html.matches(r#"type="checkbox""#).count(), 2, "{html}");
    assert!(!html.contains("checked"), "{html}");
}

#[test]
fn us99_a_failed_removal_is_told_on_its_own_page_only() {
    let failed = Some((LinkEnd::Trip(5), "Could not remove the link".to_string()));

    assert_eq!(
        error_on(LinkEnd::Trip(5), &failed),
        Some("Could not remove the link")
    );
    assert_eq!(error_on(LinkEnd::Trip(7), &failed), None);
    assert_eq!(error_on(LinkEnd::Trip(5), &None), None);
}

#[test]
fn us99_only_a_recorded_trip_has_its_date_and_only_when_its_name_lacks_it() {
    assert_eq!(
        trip_date(TripKind::Recorded, "Monday", Some("2026-07-06")).as_deref(),
        Some("2026-07-06")
    );
    assert_eq!(
        trip_date(TripKind::Recorded, "2026-07-06 Hike", Some("2026-07-06")),
        None
    );
    assert_eq!(
        trip_date(TripKind::Planned, "Plan", Some("2026-07-06")),
        None
    );
    assert_eq!(trip_date(TripKind::Recorded, "Untimed", None), None);

    let links = Links {
        recorded_trips: vec![a_trip(1, "2026-07-06 Hike", Some("2026-07-06"))],
        planned_trips: vec![a_trip(2, "Plan", Some("2026-07-01"))],
        ..Links::default()
    };
    let groups = groups(LinkEnd::CollectionTrack(3), &links);
    assert_eq!(groups[0].rows[0].detail, None);
    assert_eq!(groups[1].rows[0].detail, None);
}

#[test]
fn us99_a_ticked_line_takes_its_activitys_color_and_its_kinds_style() {
    let row = |shown, activity| Row {
        name: "Plan".to_string(),
        detail: None,
        to: Route::TripDetail { id: 2 },
        icon: RowIcon::Activity(activity),
        shown,
        unlink: None,
    };
    let track = crate::track::Track {
        geometry: crate::track::Geometry {
            coordinates: vec![vec![9.1, 46.5, 1000.0]],
        },
        properties: Default::default(),
    };

    let planned = overlays::overlay_line(
        &row(Shown::Line(Kind::Planned, 2), ActivityType::Hiking),
        &track,
    )
    .expect("a line");
    let stage = overlays::overlay_line(
        &row(Shown::Line(Kind::Collections, 3), ActivityType::Cycling),
        &track,
    )
    .expect("a line");

    assert_eq!(planned.name, "Plan");
    assert_eq!(
        planned.color,
        crate::activity_color::color(ActivityType::Hiking)
    );
    assert_eq!(planned.dash, Kind::Planned.dash());
    assert_eq!(planned.points, vec![[46.5, 9.1]]);
    assert_eq!(
        stage.color,
        crate::activity_color::color(ActivityType::Cycling)
    );
    assert_eq!(stage.dash, Kind::Collections.dash());
    assert!(
        overlays::overlay_line(&row(Shown::Waypoint(4), ActivityType::Hiking), &track).is_none()
    );
}

// Ticking a row puts it on the page's map, read from the archive; the box's
// click itself is a real event, the browser layer's.
#[tokio::test]
async fn us99_what_is_ticked_is_read_for_the_map_in_list_order() {
    let (archive, _dir) = serve_test_archive().await;
    let plan = import_sample(&archive, &[("kind", "planned"), ("name", "The plan")]).await;
    let (collection, _) = import_collection(&archive, COLLECTION_GPX, &[]).await;
    let waypoint = api::list_collection_waypoints(&archive, collection)
        .await
        .unwrap()[0]
        .clone();
    let rows = vec![
        Row {
            name: "The plan".to_string(),
            detail: None,
            to: Route::TripDetail { id: plan },
            icon: RowIcon::Activity(ActivityType::Hiking),
            shown: Shown::Line(Kind::Planned, plan),
            unlink: None,
        },
        Row {
            name: "Pass".to_string(),
            detail: None,
            to: Route::Waypoint { id: waypoint.id },
            icon: RowIcon::Symbol(None),
            shown: Shown::Waypoint(waypoint.id),
            unlink: None,
        },
    ];

    let overlays = overlays::overlays_of(&archive, rows).await;

    assert_eq!(overlays.lines.len(), 1);
    assert_eq!(overlays.lines[0].name, "The plan");
    assert!(!overlays.lines[0].points.is_empty());
    assert_eq!(
        overlays.waypoints,
        vec![crate::interop::MapWaypoint::from(&waypoint)]
    );
}
