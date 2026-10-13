//! The owner's share form, dialog and panels (US-53, US-82, US-94).

use super::*;
use crate::test_support::render;

#[test]
fn us53_the_button_says_how_many_trips_it_shares() {
    assert_eq!(
        create_label(&ShareTarget::Trips(vec![1])),
        "Make a link to this trip"
    );
    assert_eq!(
        create_label(&ShareTarget::Trips(vec![1, 2, 3])),
        "Make a link to 3 trips"
    );
}

#[test]
fn us94_the_button_says_how_many_collections_or_tracks_it_shares() {
    assert_eq!(
        create_label(&ShareTarget::Collections(vec![1])),
        "Make a link to this collection"
    );
    assert_eq!(
        create_label(&ShareTarget::Collections(vec![1, 2])),
        "Make a link to 2 collections"
    );
    assert_eq!(
        create_label(&ShareTarget::CollectionTracks(vec![1])),
        "Make a link to this track"
    );
    assert_eq!(
        create_label(&ShareTarget::CollectionTracks(vec![1, 2, 3])),
        "Make a link to 3 tracks"
    );
}

#[test]
fn us94_each_target_is_sent_as_its_own_field() {
    let sent = request(
        &ShareTarget::Collections(vec![3, 4]),
        "For Kari",
        ShareExpiry::OneMonth,
    );
    assert_eq!(
        sent,
        CreateShare {
            collection_ids: vec![3, 4],
            label: Some("For Kari".to_string()),
            expiry: ShareExpiry::OneMonth,
            ..CreateShare::default()
        }
    );
    let sent = request(
        &ShareTarget::CollectionTracks(vec![7]),
        "  ",
        ShareExpiry::Never,
    );
    assert_eq!(
        sent,
        CreateShare {
            collection_track_ids: vec![7],
            ..CreateShare::default()
        }
    );
    assert_eq!(
        request(&ShareTarget::Trips(vec![1]), "", ShareExpiry::Never).trip_ids,
        [1]
    );
    assert_eq!(
        request(
            &ShareTarget::Tags(vec!["alps".to_string()]),
            "",
            ShareExpiry::Never
        )
        .tags,
        ["alps"]
    );
}

#[test]
fn us94_the_panel_names_what_is_selected() {
    let html = render(|| {
        let selected = use_signal(|| BTreeSet::from([4, 5]));
        rsx! { ShareSelectedPanel { selected, of: Selected::Collections } }
    });
    assert!(html.contains("Share selected collections"), "{html}");
    assert!(html.contains("Share 2 selected…"), "{html}");
    let html = render(|| {
        let selected = use_signal(|| BTreeSet::from([4]));
        rsx! { ShareSelectedPanel { selected, of: Selected::CollectionTracks } }
    });
    assert!(html.contains("Share selected tracks"), "{html}");
}

#[test]
fn us94_a_pages_share_button_opens_nothing_until_pressed() {
    let html = render(|| {
        rsx! { ShareButton { id: "share-collection", target: ShareTarget::Collections(vec![1]) } }
    });
    assert!(html.contains(r#"id="share-collection""#), "{html}");
    assert!(!html.contains(r#"role="dialog""#), "{html}");
}

#[test]
fn us82_the_button_says_it_shares_the_summary() {
    let tags = ShareTarget::Tags(vec!["alps".to_string(), "norway".to_string()]);
    assert_eq!(create_label(&tags), "Make a link to this summary");
}

#[test]
fn us82_the_summary_is_shared_with_the_same_options() {
    let html = render(|| {
        rsx! {
            ShareDialog {
                target: ShareTarget::Tags(vec!["alps".to_string()]),
                on_close: move |_| {},
            }
        }
    });
    assert!(html.contains(r#"name="share-label""#), "{html}");
    assert!(html.contains(r#"name="share-expiry""#), "{html}");
    assert!(html.contains("Make a link to this summary"), "{html}");
}

#[test]
fn us53_the_form_offers_a_title_and_the_three_expiries() {
    let html = render(|| rsx! { ShareForm { target: ShareTarget::Trips(vec![1, 2]) } });
    assert!(html.contains(r#"name="share-label""#), "{html}");
    for expiry in ShareExpiry::ALL {
        assert!(
            html.contains(&format!(r#"value="{}""#, expiry.as_str())),
            "{expiry:?}: {html}"
        );
    }
    assert!(html.contains("Make a link to 2 trips"), "{html}");
}

#[test]
fn us53_the_panel_appears_only_with_a_selection() {
    let html = render(|| {
        let selected = use_signal(BTreeSet::new);
        rsx! { ShareSelectedPanel { selected } }
    });
    assert!(!html.contains("Share selected trips"), "{html}");

    let html = render(|| {
        let selected = use_signal(|| BTreeSet::from([4, 5]));
        rsx! { ShareSelectedPanel { selected } }
    });
    assert!(html.contains("Share selected trips"), "{html}");
    assert!(html.contains(r#"id="share-selected""#), "{html}");
    assert!(html.contains("Share 2 selected…"), "{html}");
}

#[test]
fn the_share_options_stay_closed_until_asked_for() {
    // Sharing is occasional, so its options sit in an overlay rather
    // than beside tagging and setting the activity.
    let html = render(|| {
        let selected = use_signal(|| BTreeSet::from([4]));
        rsx! { ShareSelectedPanel { selected } }
    });
    assert!(!html.contains(r#"name="share-label""#), "{html}");
    assert!(!html.contains(r#"role="dialog""#), "{html}");
}

#[test]
fn the_share_options_open_in_a_dialog() {
    let html = render(
        || rsx! { ShareDialog { target: ShareTarget::Trips(vec![1, 2]), on_close: move |_| {} } },
    );
    assert!(html.contains(r#"role="dialog""#), "{html}");
    assert!(html.contains(r#"name="share-label""#), "{html}");
    assert!(html.contains("Make a link to 2 trips"), "{html}");
    assert!(html.contains(r#"id="close-share""#), "{html}");
}

#[test]
fn us53_a_made_link_says_how_long_it_works() {
    let html = render(|| {
        rsx! {
            ShareLink {
                url: "https://archive.example/app/s/abc".to_string(),
                expires_at: Some("2026-10-25T12:00:00Z".to_string()),
            }
        }
    });
    assert!(html.contains("https://archive.example/app/s/abc"), "{html}");
    assert!(html.contains("works until"), "{html}");

    let html = render(|| {
        rsx! { ShareLink { url: "x".to_string(), expires_at: None } }
    });
    assert!(html.contains("until you stop it"), "{html}");
    assert!(html.contains(TRIPS_SHOWN), "{html}");
}

#[test]
fn us94_a_link_says_what_it_shows() {
    assert_eq!(shown(&ShareTarget::Collections(vec![1])), COLLECTIONS_SHOWN);
    assert_eq!(shown(&ShareTarget::CollectionTracks(vec![1])), TRACKS_SHOWN);
    assert_eq!(shown(&ShareTarget::Tags(Vec::new())), TRIPS_SHOWN);
    let html = render(|| {
        rsx! { ShareLink { url: "x".to_string(), expires_at: None, shows: TRACKS_SHOWN } }
    });
    assert!(html.contains(TRACKS_SHOWN), "{html}");
}
