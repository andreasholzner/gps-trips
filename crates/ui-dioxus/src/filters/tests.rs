//! `Filters` unit tests, split out to keep `filters.rs` under the repo's
//! file-length cap.

use super::*;
use crate::kinds::Kind;

#[test]
fn an_untouched_filter_asks_only_for_recorded_trips() {
    assert_eq!(Filters::default().to_query(), "kind=recorded");
    assert!(!Filters::default().any_set());
}

#[test]
fn set_fields_become_the_servers_own_parameter_names() {
    let filters = Filters {
        kinds: Kinds::only(Kind::Planned),
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
        kinds: Kinds::only(Kind::Planned),
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

    assert_eq!(filters.kinds, Kinds::only(Kind::Planned));
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

// ── US-90: kinds instead of tabs ────────────────────────────────────────

fn every_filter_set(kinds: Kinds) -> Filters {
    Filters {
        kinds,
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

fn collections_only() -> Kinds {
    Kinds::only(Kind::Collections)
}

fn all_kinds() -> Kinds {
    Kinds::default()
        .toggled(Kind::Planned)
        .toggled(Kind::Collections)
}

#[test]
fn us90_the_kinds_on_live_in_the_url() {
    let filters = Filters {
        kinds: all_kinds(),
        ..Default::default()
    };

    assert_eq!(filters.to_query(), "kind=recorded%2Cplanned%2Ccollections");
    assert_eq!(Filters::from_query(&filters.to_query()), filters);
}

#[test]
fn us87_a_link_to_the_collections_tab_opens_with_collections_alone() {
    assert_eq!(
        Filters::from_query("kind=collections").kinds,
        collections_only()
    );
}

#[test]
fn us90_switching_trips_off_keeps_every_trip_filter_in_the_url() {
    let filters = every_filter_set(collections_only());

    assert_eq!(Filters::from_query(&filters.to_query()), filters);
}

#[test]
fn us90_trips_are_asked_for_by_their_kind_or_both_without_one() {
    assert_eq!(
        Filters::default().trip_query().as_deref(),
        Some("kind=recorded")
    );
    let planned = Filters {
        kinds: Kinds::only(Kind::Planned).toggled(Kind::Collections),
        q: "alp".to_string(),
        ..Default::default()
    };
    assert_eq!(planned.trip_query().as_deref(), Some("kind=planned&q=alp"));
    let both = Filters {
        kinds: all_kinds(),
        q: "alp".to_string(),
        ..Default::default()
    };
    assert_eq!(both.trip_query().as_deref(), Some("q=alp"));
}

#[test]
fn us90_with_no_kind_of_trip_on_no_trips_are_asked_for() {
    let filters = Filters {
        kinds: collections_only(),
        ..Default::default()
    };

    assert_eq!(filters.trip_query(), None);
}

#[test]
fn us90_collections_are_asked_for_by_name_region_and_activity() {
    let filters = every_filter_set(collections_only());

    assert_eq!(
        filters.collection_query(),
        "activity=hiking&q=alp&bbox=10%2C59%2C11%2C60",
        "in the order the trips' query has them"
    );
    assert_eq!(Filters::default().collection_query(), "");
}

#[test]
fn us90_with_collections_alone_only_their_filters_count() {
    let trip_filters_only = Filters {
        q: String::new(),
        bbox: String::new(),
        activity: None,
        ..every_filter_set(collections_only())
    };
    let narrowed = [
        Filters {
            q: "alp".to_string(),
            ..trip_filters_only.clone()
        },
        Filters {
            bbox: "10,59,11,60".to_string(),
            ..trip_filters_only.clone()
        },
        Filters {
            activity: Some(ActivityType::Cycling),
            ..trip_filters_only.clone()
        },
    ];

    assert!(!trip_filters_only.any_set());
    for filters in narrowed {
        assert!(filters.any_set(), "{filters:?}");
    }
}

#[test]
fn us90_clearing_with_collections_alone_keeps_the_hidden_trip_filters() {
    let filters = every_filter_set(collections_only());

    let cleared = filters.cleared();

    assert_eq!(
        cleared,
        Filters {
            q: String::new(),
            bbox: String::new(),
            activity: None,
            ..filters
        }
    );
}

#[test]
fn clearing_with_trips_on_clears_everything_but_the_kinds() {
    let cleared = every_filter_set(all_kinds()).cleared();

    assert_eq!(
        cleared,
        Filters {
            kinds: all_kinds(),
            ..Default::default()
        }
    );
}

#[test]
fn an_unknown_kind_in_the_url_shows_recorded_trips() {
    assert_eq!(Filters::from_query("kind=nonsense").kinds, Kinds::default());
}
