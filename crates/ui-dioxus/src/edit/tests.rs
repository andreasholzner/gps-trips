use super::*;
use crate::test_support::{
    import_gpx, import_sample, render, render_against_archive, serve_test_archive,
    serve_test_archive_with_ground,
};
use trip_archive_types::{KomootLink, KomootPrivacy};

fn a_trip(activity: ActivityType, komoot: Option<KomootLink>) -> Trip {
    Trip {
        id: 1,
        name: "Oslo Hills Walk".to_string(),
        activity_type: activity,
        tz_name: None,
        start_time: None,
        start_date: None,
        end_time: None,
        distance_m: 1000.0,
        ascent_m: None,
        descent_m: None,
        duration_secs: None,
        moving_secs: None,
        moving_distance_m: None,
        climb_gain_m: None,
        climb_secs: None,
        min_lat: None,
        min_lon: None,
        max_lat: None,
        max_lon: None,
        komoot,
    }
}

fn a_form(trip: &Trip) -> EditForm {
    EditForm::of(trip)
}

/// Just the privacy picker's own options. The form carries a second
/// `<select>` — the activity one — with a blank option of its own, so a
/// claim about "the blank option" has to say which picker it means.
fn privacy_picker(html: &str) -> &str {
    let from = html
        .find(r#"id="edit-privacy_status""#)
        .expect("a privacy picker to look at");
    let rest = &html[from..];
    &rest[..rest.find("</select>").expect("a closed picker")]
}

/// The form as the trip opens it, rendered.
fn rendered(trip: Trip) -> String {
    render(move || {
        rsx! {
            EditTripForm {
                trip: trip.clone(),
                on_saved: move |_| {},
                on_cancel: move |_| {},
            }
        }
    })
}

#[test]
fn a_form_nobody_touched_asks_for_no_change_at_all() {
    let trip = a_trip(ActivityType::Hiking, None);

    assert!(changes(&trip, &a_form(&trip)).is_empty());
}

#[test]
fn only_the_fields_that_changed_are_asked_for() {
    // US-15's reason for a PATCH: an omitted field is left unchanged, so
    // renaming a trip cannot write back an activity type that a Komoot
    // sync altered after this screen loaded.
    let trip = a_trip(ActivityType::Hiking, None);
    let mut form = a_form(&trip);
    form.name = "Renamed Trip".to_string();

    let edit = changes(&trip, &form);

    assert_eq!(edit.name.as_deref(), Some("Renamed Trip"));
    assert_eq!(edit.activity_type, None);
    assert_eq!(edit.privacy_status, None);
}

#[test]
fn clearing_the_activity_asks_for_the_unspecified_one() {
    // The picker's blank choice is `Unknown` on the wire — the same value
    // an import that chose no activity stores.
    let trip = a_trip(ActivityType::Hiking, None);
    let mut form = a_form(&trip);
    form.activity = String::new();

    assert_eq!(changes(&trip, &form).activity_type.as_deref(), Some(""));
}

#[test]
fn a_privacy_left_as_the_archive_reported_it_is_never_pushed_back() {
    // US-35/ADR-0021: a privacy the archive could not map shows as
    // "Unknown" and must never be sent to Komoot as a choice. The picker
    // opens on a placeholder, and leaving it there asks for nothing.
    let trip = a_trip(
        ActivityType::Hiking,
        Some(KomootLink {
            tour_id: "111".to_string(),
            privacy: Some(KomootPrivacy::Unknown),
        }),
    );

    assert!(changes(&trip, &a_form(&trip)).is_empty());
}

#[test]
fn a_chosen_privacy_is_asked_for() {
    let trip = a_trip(
        ActivityType::Hiking,
        Some(KomootLink {
            tour_id: "111".to_string(),
            privacy: Some(KomootPrivacy::Private),
        }),
    );
    let mut form = a_form(&trip);
    form.privacy = KomootPrivacy::Public.as_str().to_string();

    assert_eq!(
        changes(&trip, &form).privacy_status.as_deref(),
        Some("public")
    );
}

#[test]
fn the_form_opens_on_the_trips_current_values() {
    let trip = a_trip(ActivityType::Hiking, None);

    let html = render(move || {
        rsx! {
            EditTripForm {
                trip: trip.clone(),
                on_saved: move |_| {},
                on_cancel: move |_| {},
            }
        }
    });

    assert!(html.contains(r#"value="Oslo Hills Walk""#), "{html}");
    assert!(html.contains(ActivityType::Hiking.label()), "{html}");
    // Every activity the owner may choose, plus the unspecified one.
    assert!(html.contains(ActivityType::Cycling.label()), "{html}");
    assert!(html.contains(ActivityType::Unknown.label()), "{html}");
}

// US-62: the form opens over the screen, so it carries its own way out.
#[test]
fn the_form_can_be_left_without_saving() {
    let trip = a_trip(ActivityType::Hiking, None);

    let html = render(move || {
        rsx! {
            EditTripForm {
                trip: trip.clone(),
                on_saved: move |_| {},
                on_cancel: move |_| {},
            }
        }
    });

    assert!(html.contains(r#"id="edit-trip-cancel""#), "{html}");
}

#[test]
fn editing_is_offered_but_not_open() {
    let trip = a_trip(ActivityType::Hiking, None);

    let html = render(move || {
        rsx! { EditTrip { trip: trip.clone(), on_saved: move |_| {} } }
    });

    assert!(html.contains(r#"id="edit-trip""#), "{html}");
    assert!(!html.contains("edit-trip-form"), "{html}");
}

#[test]
fn a_trip_that_never_came_from_komoot_is_offered_no_privacy() {
    let trip = a_trip(ActivityType::Hiking, None);

    let html = render(move || {
        rsx! {
            EditTripForm {
                trip: trip.clone(),
                on_saved: move |_| {},
                on_cancel: move |_| {},
            }
        }
    });

    assert!(!html.contains("Komoot privacy"), "{html}");
}

#[test]
fn a_privacy_the_archive_does_not_know_opens_on_a_placeholder() {
    // A `<select>` with no `selected` option shows its *first* one, so a
    // bare picker would assert "Private" for a trip whose privacy the
    // archive has no idea about — and, since only a changed value is
    // sent, would make Private the one value the owner then could not
    // choose. Both "no privacy read yet" and "Komoot reported something
    // unmappable" must therefore open on a placeholder.
    for privacy in [None, Some(KomootPrivacy::Unknown)] {
        let trip = a_trip(
            ActivityType::Hiking,
            Some(KomootLink {
                tour_id: "111".to_string(),
                privacy,
            }),
        );

        let html = rendered(trip);
        let picker = privacy_picker(&html);

        assert!(
            picker.contains(r#"<option value="">"#),
            "a placeholder must be offered for {privacy:?}: {picker}"
        );
        for settable in KomootPrivacy::SELECTABLE {
            assert!(
                !picker.contains(&format!(r#"value="{}" selected"#, settable.as_str())),
                "{settable} must not look chosen for {privacy:?}: {picker}"
            );
        }
    }
}

#[test]
fn a_linked_trip_is_offered_the_settable_privacies() {
    let trip = a_trip(
        ActivityType::Hiking,
        Some(KomootLink {
            tour_id: "111".to_string(),
            privacy: Some(KomootPrivacy::Public),
        }),
    );

    let html = render(move || {
        rsx! {
            EditTripForm {
                trip: trip.clone(),
                on_saved: move |_| {},
                on_cancel: move |_| {},
            }
        }
    });

    assert!(html.contains("Komoot privacy"), "{html}");
    let picker = privacy_picker(&html);
    assert!(picker.contains(KomootPrivacy::Public.label()), "{picker}");
    assert!(picker.contains(KomootPrivacy::Private.label()), "{picker}");
    // The placeholder only exists to avoid claiming a privacy that is not
    // known; it is not a value the owner can go back to.
    assert!(!picker.contains(r#"<option value="">"#), "{picker}");
    // Never offered: it is a state Komoot put the tour in, not a choice.
    assert!(
        !html.contains(&format!("value=\"{}\"", KomootPrivacy::Unknown.as_str())),
        "{html}"
    );
}

// US-74: the name the archive suggests for the trip as stored, offered next
// to the field — and only offered.
#[tokio::test]
async fn us74_the_form_offers_the_suggested_name_without_changing_the_field() {
    let (archive, _dir) = serve_test_archive().await;
    let id = import_sample(&archive, &[("name", "My walk")]).await;
    let trip = api::get_trip(&archive, id).await.expect("trip");

    let html = render_against_archive(
        &archive,
        move || {
            rsx! {
                EditTripForm {
                    trip: trip.clone(),
                    on_saved: move |_| {},
                    on_cancel: move |_| {},
                }
            }
        },
        |html| html.contains("edit-name-suggestion"),
    )
    .await;

    // The test archive has no place database: the date and the GPX name.
    assert!(html.contains("2024-06-01 Oslo Hills Walk"), "{html}");
    assert!(html.contains(r#"value="My walk""#), "{html}");
}

// US-76: the activity the track looks like, offered next to the selector —
// and only offered.
#[tokio::test]
async fn us76_the_form_offers_the_suggested_activity_without_choosing_it() {
    let (archive, _dir) = serve_test_archive_with_ground().await;
    let kayaking = include_bytes!("../../../../tests/fixtures/activities/kayaking.gpx");
    let id = import_gpx(&archive, kayaking, &[("name", "Paddle")]).await;
    let trip = api::get_trip(&archive, id).await.expect("trip");

    let html = render_against_archive(
        &archive,
        move || {
            rsx! {
                EditTripForm {
                    trip: trip.clone(),
                    on_saved: move |_| {},
                    on_cancel: move |_| {},
                }
            }
        },
        |html| html.contains("edit-activity-suggestion"),
    )
    .await;

    let offer = &html[html.find("edit-activity-suggestion").expect("offered")..];
    assert!(offer.contains("Kayaking"), "{html}");
    // The selector still holds what the trip has: unspecified.
    assert!(
        html.contains(r#"id="edit-activity_type" value="""#),
        "{html}"
    );
}
