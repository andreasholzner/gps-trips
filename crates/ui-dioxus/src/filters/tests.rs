//! `Filters` unit tests, split out to keep `filters.rs` under the repo's
//! file-length cap.

use super::*;

#[test]
fn an_untouched_filter_asks_only_for_its_tab() {
    assert_eq!(Filters::default().to_query(), "kind=recorded");
    assert!(!Filters::default().any_set());
}

#[test]
fn set_fields_become_the_servers_own_parameter_names() {
    let filters = Filters {
        tab: ListTab::Planned,
        q: "oslo".to_string(),
        activity: Some(ActivityType::Hiking),
        from: "2026-01-01".to_string(),
        to: "2026-12-31".to_string(),
        min_dist: "5".to_string(),
        max_dist: "40".to_string(),
        tags: Vec::new(),
        bbox: String::new(),
        unnamed: false,
        unplaced: false,
    };

    assert_eq!(
        filters.to_query(),
        "kind=planned&activity=hiking&q=oslo&from=2026-01-01&to=2026-12-31&min_dist=5&max_dist=40"
    );
    assert!(filters.any_set());
}

#[test]
fn blank_and_whitespace_only_fields_are_left_out() {
    let filters = Filters {
        q: "   ".to_string(),
        ..Default::default()
    };

    assert_eq!(filters.to_query(), "kind=recorded");
    assert!(!filters.any_set());
}

#[test]
fn chosen_tags_travel_as_one_comma_separated_parameter() {
    // US-38: comma-joined is unambiguous because a tag name can never
    // contain a comma (US-33).
    let filters = Filters {
        tags: vec!["alpine".to_string(), "summer".to_string()],
        ..Default::default()
    };

    assert_eq!(filters.to_query(), "kind=recorded&tags=alpine%2Csummer");
    assert!(filters.any_set());
}

#[test]
fn every_filter_survives_a_round_trip_through_the_query_string() {
    // The SPA's URL is this same string (US-52), so a filtered list is
    // bookmarkable: what goes into the address bar must come back out.
    let filters = Filters {
        tab: ListTab::Planned,
        q: "oslo".to_string(),
        activity: Some(ActivityType::SkiTouring),
        from: "2026-01-01".to_string(),
        to: "2026-12-31".to_string(),
        min_dist: "5".to_string(),
        max_dist: "40".to_string(),
        tags: vec!["alpine".to_string(), "summer".to_string()],
        bbox: String::new(),
        unnamed: true,
        unplaced: true,
    };

    assert_eq!(Filters::from_query(&filters.to_query()), filters);
}

#[test]
fn a_search_full_of_awkward_characters_round_trips_too() {
    let filters = Filters {
        q: "oslo & bergen? 100% + å".to_string(),
        ..Default::default()
    };

    assert_eq!(Filters::from_query(&filters.to_query()), filters);
}

#[test]
fn a_chosen_region_travels_as_the_apis_bbox_parameter() {
    let filters = Filters {
        bbox: "10.75,59.91,11.25,60.12".to_string(),
        ..Default::default()
    };

    assert_eq!(
        filters.to_query(),
        "kind=recorded&bbox=10.75%2C59.91%2C11.25%2C60.12"
    );
    assert_eq!(Filters::from_query(&filters.to_query()), filters);
}

#[test]
fn a_chosen_region_counts_as_a_filter() {
    // Otherwise an empty result reads "no trips yet" instead of "nothing
    // matches your filters" (US-14).
    let filters = Filters {
        bbox: "10.75,59.91,11.25,60.12".to_string(),
        ..Default::default()
    };

    assert!(filters.any_set());
    assert!(!Filters::default().any_set());
}

#[test]
fn the_tidy_up_filters_travel_as_true_flags() {
    // US-66: the server takes `true` or nothing.
    let filters = Filters {
        unnamed: true,
        unplaced: true,
        ..Default::default()
    };

    assert_eq!(
        filters.to_query(),
        "kind=recorded&unnamed=true&unplaced=true"
    );
    assert!(filters.any_set());
}

#[test]
fn each_tidy_up_filter_counts_as_a_filter_on_its_own() {
    // Otherwise an empty result reads "no trips yet" instead of "nothing
    // matches your filters" (US-66).
    for filters in [
        Filters {
            unnamed: true,
            ..Default::default()
        },
        Filters {
            unplaced: true,
            ..Default::default()
        },
    ] {
        assert!(filters.any_set(), "{filters:?}");
    }
}

#[test]
fn a_tidy_up_flag_with_another_value_is_not_applied() {
    // A hand-edited URL yields a working list rather than the 400 the
    // server would answer `unnamed=yes` with.
    let filters = Filters::from_query("unnamed=yes&unplaced=true");

    assert!(!filters.unnamed);
    assert!(filters.unplaced);
}

#[test]
fn an_empty_query_is_the_default_view() {
    assert_eq!(Filters::from_query(""), Filters::default());
}

#[test]
fn a_hand_edited_query_keeps_what_it_can() {
    // A truncated or mistyped URL should still show a working list, the
    // way the server's own parser treats a blank value.
    let filters = Filters::from_query("kind=planned&activity=nonsense&q=oslo&bogus=1&novalue");

    assert_eq!(filters.tab, ListTab::Planned);
    assert_eq!(filters.activity, None);
    assert_eq!(filters.q, "oslo");
}

#[test]
fn a_free_text_search_is_encoded_so_it_cannot_break_the_query_string() {
    let filters = Filters {
        q: "oslo & bergen?".to_string(),
        ..Default::default()
    };

    assert_eq!(filters.to_query(), "kind=recorded&q=oslo+%26+bergen%3F");
}

// ── US-87: the Collections tab ──────────────────────────────────────────

fn every_trip_filter_set(tab: ListTab) -> Filters {
    Filters {
        tab,
        q: "alp".to_string(),
        activity: Some(ActivityType::Hiking),
        from: "2026-01-01".to_string(),
        to: "2026-12-31".to_string(),
        min_dist: "5".to_string(),
        max_dist: "40".to_string(),
        tags: vec!["alpine".to_string()],
        bbox: "10,59,11,60".to_string(),
        unnamed: true,
        unplaced: true,
    }
}

#[test]
fn us87_the_collections_tab_lives_in_the_url() {
    let filters = Filters {
        tab: ListTab::Collections,
        ..Default::default()
    };

    assert_eq!(filters.to_query(), "kind=collections");
    assert_eq!(Filters::from_query(&filters.to_query()), filters);
}

#[test]
fn us87_switching_tabs_keeps_every_trip_filter_in_the_url() {
    let filters = every_trip_filter_set(ListTab::Collections);

    assert_eq!(Filters::from_query(&filters.to_query()), filters);
}

#[test]
fn us87_the_tabs_come_in_order_with_their_labels() {
    let labels: Vec<_> = ListTab::ALL.iter().map(|tab| tab.label()).collect();

    assert_eq!(labels, ["Recorded", "Planned", "Collections"]);
}

#[test]
fn us87_only_the_trip_tabs_ask_for_trips() {
    assert_eq!(
        Filters::default().trip_query().as_deref(),
        Some("kind=recorded")
    );
    let collections = Filters {
        tab: ListTab::Collections,
        ..Default::default()
    };
    assert_eq!(collections.trip_query(), None);
    assert_eq!(ListTab::Planned.trip_kind(), Some(TripKind::Planned));
    assert_eq!(ListTab::Collections.trip_kind(), None);
}

#[test]
fn us87_collections_are_asked_for_by_name_and_region_alone() {
    let filters = every_trip_filter_set(ListTab::Collections);

    assert_eq!(filters.collection_query(), "q=alp&bbox=10%2C59%2C11%2C60");
    assert_eq!(Filters::default().collection_query(), "");
}

#[test]
fn us87_on_the_collections_tab_only_its_own_filters_count() {
    let trip_filters_only = Filters {
        q: String::new(),
        bbox: String::new(),
        ..every_trip_filter_set(ListTab::Collections)
    };
    let searched = Filters {
        tab: ListTab::Collections,
        q: "alp".to_string(),
        ..Default::default()
    };
    let in_a_region = Filters {
        tab: ListTab::Collections,
        bbox: "10,59,11,60".to_string(),
        ..Default::default()
    };

    assert!(!trip_filters_only.any_set());
    assert!(searched.any_set());
    assert!(in_a_region.any_set());
}

#[test]
fn us87_clearing_on_the_collections_tab_keeps_the_hidden_trip_filters() {
    let filters = every_trip_filter_set(ListTab::Collections);

    let cleared = filters.cleared();

    assert_eq!(
        cleared,
        Filters {
            q: String::new(),
            bbox: String::new(),
            ..filters
        }
    );
}

#[test]
fn clearing_on_a_trip_tab_clears_everything_but_the_tab() {
    let cleared = every_trip_filter_set(ListTab::Planned).cleared();

    assert_eq!(
        cleared,
        Filters {
            tab: ListTab::Planned,
            ..Default::default()
        }
    );
}

#[test]
fn an_unknown_tab_in_the_url_shows_recorded_trips() {
    assert_eq!(Filters::from_query("kind=nonsense").tab, ListTab::Recorded);
}
