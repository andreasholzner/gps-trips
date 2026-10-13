//! The router's half of the URL: each screen's path parses back to the
//! screen it was written for, its query included (US-52).

use super::*;
use crate::filters::ListTab;
use std::str::FromStr;
use trip_archive_types::ActivityType;

/// The router owns the URL, and it percent-decodes the whole query string
/// before handing it to `FromQuery`. That interacts with this crate's own
/// escaping, so the round trip is asserted through `Route` itself rather
/// than reasoned about.
fn round_trip(filters: Filters) {
    let url = Route::TripList {
        filters: filters.clone(),
    }
    .to_string();
    let parsed = Route::from_str(&url).expect("the router must parse a URL it just wrote");
    let Route::TripList { filters: back } = parsed else {
        panic!("a list URL must parse back to the list screen; url was {url:?}")
    };
    assert_eq!(back, filters, "url was {url:?}");
}

#[test]
fn a_plain_filter_round_trips_through_the_url() {
    round_trip(Filters {
        tab: ListTab::Planned,
        q: "oslo".to_string(),
        activity: Some(ActivityType::Hiking),
        tags: vec!["alpine".to_string()],
        ..Default::default()
    });
}

#[test]
fn a_search_containing_url_separators_round_trips_through_the_url() {
    // `&` and `=` inside a value are the case that breaks a naive scheme.
    round_trip(Filters {
        q: "b&b = 100% fun?".to_string(),
        ..Default::default()
    });
}

#[test]
fn a_trips_url_round_trips_through_the_detail_route() {
    // The property the detail screen rests on (US-42): a row's link, a
    // bookmark and a reload are the same URL, and it names one trip.
    let url = Route::TripDetail { id: 42 }.to_string();
    assert_eq!(url, "/trips/42");

    let parsed = Route::from_str(&url).expect("the router must parse a URL it just wrote");
    let Route::TripDetail { id } = parsed else {
        panic!("a trip URL must parse back to the detail screen; url was {url:?}")
    };
    assert_eq!(id, 42);
}

#[test]
fn the_komoot_url_round_trips_and_keeps_the_path_the_page_had() {
    // The server redirects `/komoot/sync` here, so this path is part of
    // that contract rather than an internal detail (US-44).
    let url = Route::KomootSync {}.to_string();
    assert_eq!(url, "/komoot/sync");

    let parsed = Route::from_str(&url).expect("the router must parse a URL it just wrote");
    assert!(
        matches!(parsed, Route::KomootSync {}),
        "a komoot URL must parse back to the sync screen; url was {url:?}"
    );
}

#[test]
fn us77_the_statistics_view_round_trips_through_the_url() {
    let view = StatsView {
        year: Some(2024),
        measure: stats::Measure::DaysOut,
        activities: vec![
            trip_archive_types::ActivityType::Hiking,
            trip_archive_types::ActivityType::Kayaking,
        ],
    };
    let url = Route::Statistics { view: view.clone() }.to_string();

    let parsed = Route::from_str(&url).expect("the router must parse a URL it just wrote");
    let Route::Statistics { view: back } = parsed else {
        panic!("a statistics URL must parse back to the screen; url was {url:?}")
    };
    assert_eq!(back, view);
}

#[test]
fn us78_a_summary_url_round_trips_its_tags() {
    let view = SummaryView::of("alps").with("norway");
    let url = Route::Summary { view: view.clone() }.to_string();

    let parsed = Route::from_str(&url).expect("the router must parse a URL it just wrote");
    let Route::Summary { view: back } = parsed else {
        panic!("a summary URL must parse back to the screen; url was {url:?}")
    };
    assert_eq!(back, view);
}

#[test]
fn us83_the_tags_filter_round_trips_through_the_url() {
    let view = NameFilter {
        q: "alp s&".to_string(),
    };
    let url = Route::Tags { view: view.clone() }.to_string();

    let parsed = Route::from_str(&url).expect("the router must parse a URL it just wrote");
    let Route::Tags { view: back } = parsed else {
        panic!("a tags URL must parse back to the screen; url was {url:?}")
    };
    assert_eq!(back, view);
}

#[test]
fn us97_the_collections_filter_round_trips_through_the_url() {
    let view = NameFilter {
        q: "alp s&".to_string(),
    };
    let url = Route::Collections { view: view.clone() }.to_string();

    let parsed = Route::from_str(&url).expect("the router must parse a URL it just wrote");
    let Route::Collections { view: back } = parsed else {
        panic!("a collections URL must parse back to the screen; url was {url:?}")
    };
    assert_eq!(back, view);
}

#[test]
fn us97_the_collections_screen_and_a_collection_have_paths_of_their_own() {
    assert!(matches!(
        Route::from_str("/collections"),
        Ok(Route::Collections { .. })
    ));
    assert!(matches!(
        Route::from_str("/collections/3"),
        Ok(Route::Collection { id: 3 })
    ));
}

#[test]
fn us98_a_waypoint_has_a_path_of_its_own() {
    let url = Route::Waypoint { id: 7 }.to_string();

    assert_eq!(url, "/waypoints/7");
    assert!(matches!(
        Route::from_str(&url),
        Ok(Route::Waypoint { id: 7 })
    ));
}

#[test]
fn us53_a_share_link_names_its_token() {
    assert_eq!(shared_token("/app/s/abc").as_deref(), Some("abc"));
    assert_eq!(shared_token("/app/s/abc/trips/4").as_deref(), Some("abc"));
    assert_eq!(shared_token("/app/s/"), None);
    assert_eq!(shared_token("/app/trips/4"), None);
    assert_eq!(shared_token("/app/"), None);
}

#[test]
fn us53_the_share_routes_round_trip() {
    let url = Route::SharedTripDetail {
        token: "abc".to_string(),
        id: 4,
    }
    .to_string();
    assert_eq!(url, "/s/abc/trips/4");
    assert!(matches!(
        Route::from_str(&url),
        Ok(Route::SharedTripDetail { id: 4, .. })
    ));
    let url = Route::Shared {
        token: "abc".to_string(),
    }
    .to_string();
    assert_eq!(url, "/s/abc");
    // What `share_link` hands out, under the bundle's base path.
    assert_eq!(shared_token(&format!("/app{url}")).as_deref(), Some("abc"));
}

#[test]
fn us94_the_shared_collection_routes_round_trip() {
    let url = Route::SharedCollectionPage {
        token: "abc".to_string(),
        id: 3,
    }
    .to_string();
    assert_eq!(url, "/s/abc/collections/3");
    assert!(matches!(
        Route::from_str(&url),
        Ok(Route::SharedCollectionPage { id: 3, .. })
    ));
    let url = Route::SharedCollectionTrackPage {
        token: "abc".to_string(),
        id: 7,
    }
    .to_string();
    assert_eq!(url, "/s/abc/collection-tracks/7");
    assert!(matches!(
        Route::from_str(&url),
        Ok(Route::SharedCollectionTrackPage { id: 7, .. })
    ));
    assert_eq!(shared_token(&format!("/app{url}")).as_deref(), Some("abc"));
}

#[test]
fn the_bare_app_url_is_the_default_view() {
    let parsed = Route::from_str("/").expect("the bare path must parse");
    let Route::TripList { filters } = parsed else {
        panic!("the bare path must be the list screen")
    };
    assert_eq!(filters, Filters::default());
}
