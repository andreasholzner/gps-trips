//! The markers, the photos' views and captions, and adding photos later —
//! without a browser where a rendered string or a real archive can say it.

use super::*;
use crate::test_support::{a_photo, import_sample, render, serve_test_archive};
use trip_archive_types::LocationSource;

// US-3 and US-4: a photo is on the map when it has a position, however it
// got one. US-4's unplaced photo — outside the track's time range — has
// none, and is left off rather than guessed at.
#[test]
fn only_photos_with_a_position_become_map_markers() {
    let photos = vec![
        a_photo(1, "geotagged.jpg", Some((59.91, 10.75))),
        a_photo(2, "unplaced.jpg", None),
    ];

    let markers = photo_markers("http://archive.test", &photos);

    assert_eq!(markers.len(), 1, "{markers:?}");
    assert_eq!(markers[0].lat, 59.91);
    assert_eq!(markers[0].lon, 10.75);
    assert_eq!(markers[0].photos.len(), 1);
    assert_eq!(markers[0].photos[0].name, "geotagged.jpg");
    assert_eq!(
        markers[0].photos[0].thumbnail_url,
        "http://archive.test/media/trips/1/thumb-geotagged.jpg"
    );
}

// ── US-62: when each photo was taken, and looking at it properly ─────

fn taken(photo: PhotoResponse, at: &str, offset_secs: Option<i32>) -> PhotoResponse {
    PhotoResponse {
        taken_at: Some(at.to_string()),
        taken_offset_secs: offset_secs,
        ..photo
    }
}

#[test]
fn a_photo_is_captioned_with_when_it_was_taken_where_it_was_taken() {
    let photo = taken(
        a_photo(1, "a.jpg", None),
        "2024-06-01T08:15:00Z",
        Some(7200),
    );

    assert_eq!(caption(&photo, false).as_deref(), Some("10:15 (+02:00)"));
    assert_eq!(
        caption(&photo, true).as_deref(),
        Some("1 Jun 10:15 (+02:00)")
    );
}

#[test]
fn a_photo_whose_zone_is_unknown_is_captioned_in_labelled_utc() {
    let photo = taken(a_photo(1, "a.jpg", None), "2024-06-01T08:15:00Z", None);

    assert_eq!(caption(&photo, false).as_deref(), Some("08:15 UTC"));
}

#[test]
fn a_photo_with_no_capture_time_has_no_caption() {
    // Silence rather than a dash: a dash claims a missing value.
    assert_eq!(caption(&a_photo(1, "a.jpg", None), false), None);
}

#[test]
fn captions_carry_the_date_only_when_the_photos_span_more_than_one() {
    let morning = taken(
        a_photo(1, "a.jpg", None),
        "2024-06-01T08:15:00Z",
        Some(7200),
    );
    let evening = taken(
        a_photo(2, "b.jpg", None),
        "2024-06-01T20:15:00Z",
        Some(7200),
    );
    // 22:30 UTC is already the 2nd at +02:00: dates are the local ones.
    let midnight = taken(
        a_photo(3, "c.jpg", None),
        "2024-06-01T22:30:00Z",
        Some(7200),
    );
    let untimed = a_photo(4, "d.jpg", None);

    assert!(!captions_need_dates(&[
        morning.clone(),
        evening.clone(),
        untimed
    ]));
    assert!(captions_need_dates(&[morning, midnight]));
}

#[test]
fn the_viewer_is_given_every_photo_in_gallery_order_at_full_size() {
    let photos = vec![
        taken(
            a_photo(1, "first.jpg", None),
            "2024-06-01T08:15:00Z",
            Some(7200),
        ),
        a_photo(2, "second.jpg", Some((59.91, 10.75))),
    ];

    let views = photo_views("http://archive.test", &photos);

    assert_eq!(
        views,
        vec![
            PhotoView {
                thumbnail_url: "http://archive.test/media/trips/1/thumb-first.jpg".to_string(),
                url: "http://archive.test/media/trips/1/first.jpg".to_string(),
                name: "first.jpg".to_string(),
                caption: Some("10:15 (+02:00)".to_string()),
                // US-30: which photo it is, and where it is and why, for
                // placing it by hand from the viewer.
                id: 1,
                position: None,
                source: LocationSource::None,
            },
            PhotoView {
                thumbnail_url: "http://archive.test/media/trips/1/thumb-second.jpg".to_string(),
                url: "http://archive.test/media/trips/1/second.jpg".to_string(),
                name: "second.jpg".to_string(),
                caption: None,
                id: 2,
                position: Some([59.91, 10.75]),
                source: LocationSource::Exif,
            },
        ]
    );
}

#[test]
fn a_marker_carries_what_the_viewer_needs_for_its_photos() {
    // A tap in the popup opens the viewer on that marker's photos, so
    // each one travels with its full-size URL and its caption.
    let photos = vec![taken(
        a_photo(1, "here.jpg", Some((59.91, 10.75))),
        "2024-06-01T08:15:00Z",
        Some(7200),
    )];

    let markers = photo_markers("http://archive.test", &photos);

    assert_eq!(
        markers[0].photos,
        photo_views("http://archive.test", &photos)
    );
}

#[test]
fn a_tap_in_a_popup_opens_that_markers_photos_at_the_one_tapped() {
    // The popup's set, in the popup's order — not the trip's (US-62).
    let photos = vec![
        a_photo(1, "alone.jpg", Some((59.91, 10.75))),
        a_photo(2, "first.jpg", Some((60.5, 11.0))),
        a_photo(3, "second.jpg", Some((60.5, 11.0))),
    ];
    let markers = photo_markers("", &photos);

    let (set, start) = tapped(
        &markers,
        PopupTap {
            marker: 1,
            photo: 1,
        },
    )
    .unwrap();

    assert_eq!(
        set.iter()
            .map(|photo| photo.name.as_str())
            .collect::<Vec<_>>(),
        ["first.jpg", "second.jpg"]
    );
    assert_eq!(start, 1);
}

#[test]
fn a_tap_on_a_marker_the_map_no_longer_has_opens_nothing() {
    // The tap and the markers arrive on different paths: a popup drawn
    // for the previous photos can answer after they have changed.
    let markers = photo_markers("", &[a_photo(1, "a.jpg", Some((59.91, 10.75)))]);

    assert_eq!(
        tapped(
            &markers,
            PopupTap {
                marker: 3,
                photo: 0
            }
        ),
        None
    );
    assert_eq!(
        tapped(
            &markers,
            PopupTap {
                marker: 0,
                photo: 4
            }
        ),
        None
    );
}

// ── US-57: every photo taken at the same place is reachable ──────────

/// A latitude `metres` north of `lat` — one degree of latitude is the
/// same distance everywhere, so this needs no longitude.
fn north_of(lat: f64, metres: f64) -> f64 {
    lat + metres / (EARTH_RADIUS_M * std::f64::consts::PI / 180.0)
}

#[test]
fn photos_taken_at_the_same_place_share_one_marker_that_says_how_many() {
    // What US-4's interpolation produces routinely: several photos
    // snapped onto one track point, which used to stack markers so that
    // only the topmost could be clicked.
    let photos = vec![
        a_photo(1, "first.jpg", Some((59.91, 10.75))),
        a_photo(2, "second.jpg", Some((59.91, 10.75))),
        a_photo(3, "third.jpg", Some((north_of(59.91, 5.0), 10.75))),
    ];

    let markers = photo_markers("http://archive.test", &photos);

    assert_eq!(markers.len(), 1, "{markers:?}");
    // The seed fixes the position, and the photos keep the order the API
    // listed them in.
    assert_eq!(markers[0].lat, 59.91);
    assert_eq!(
        markers[0]
            .photos
            .iter()
            .map(|photo| photo.name.as_str())
            .collect::<Vec<_>>(),
        ["first.jpg", "second.jpg", "third.jpg"]
    );
}

#[test]
fn a_photo_further_than_twenty_metres_away_keeps_its_own_marker() {
    let photos = vec![
        a_photo(1, "here.jpg", Some((59.91, 10.75))),
        a_photo(2, "over-there.jpg", Some((north_of(59.91, 25.0), 10.75))),
    ];

    let markers = photo_markers("http://archive.test", &photos);

    assert_eq!(markers.len(), 2, "{markers:?}");
    assert_eq!(markers[0].photos[0].name, "here.jpg");
    assert_eq!(markers[1].photos[0].name, "over-there.jpg");
}

#[test]
fn grouping_is_anchored_on_the_seed_rather_than_chaining_along_a_line() {
    // Three photos 15 m apart in a line. Merging transitively would put
    // all three in one group 30 m wide — wider than the threshold the
    // marker claims. Each photo is measured against its group's seed, so
    // the third opens a group of its own.
    let photos = vec![
        a_photo(1, "a.jpg", Some((59.91, 10.75))),
        a_photo(2, "b.jpg", Some((north_of(59.91, 15.0), 10.75))),
        a_photo(3, "c.jpg", Some((north_of(59.91, 30.0), 10.75))),
    ];

    let markers = photo_markers("http://archive.test", &photos);

    assert_eq!(markers.len(), 2, "{markers:?}");
    assert_eq!(
        markers[0]
            .photos
            .iter()
            .map(|photo| photo.name.as_str())
            .collect::<Vec<_>>(),
        ["a.jpg", "b.jpg"]
    );
    assert_eq!(markers[1].photos[0].name, "c.jpg");
}

#[test]
fn distance_is_metres_on_the_ground_in_both_directions() {
    // Sub-metre at this threshold is all the grouping needs, which is why
    // it is an equirectangular approximation computed here rather than a
    // reason to pull `geo` into this crate.
    let oslo = (59.91, 10.75);

    let north = metres_between(oslo, (north_of(59.91, 15.0), 10.75));
    assert!((north - 15.0).abs() < 0.01, "{north}");

    // A degree of longitude shortens with the cosine of the latitude:
    // ~55.6 m per 0.001° at 59.91°N, not the ~111 m it would be at the
    // equator.
    let east = metres_between(oslo, (59.91, 10.751));
    assert!((east - 55.7).abs() < 0.5, "{east}");
}

#[test]
fn a_photos_url_is_resolved_against_the_archive_it_came_from() {
    // The archive serves photo blobs under a path of its own; on Android
    // the app is not served from that origin at all (US-16), so the URL
    // the markers and the gallery use is made absolute here rather than
    // left for a webview to resolve against itself.
    assert_eq!(
        absolute("http://archive.test", "/media/trips/1/a.jpg"),
        "http://archive.test/media/trips/1/a.jpg"
    );
    // The web build fetches from its own origin, which is what `base_url`
    // already is; an empty one leaves the path alone.
    assert_eq!(absolute("", "/media/trips/1/a.jpg"), "/media/trips/1/a.jpg");
    // A blob store that serves its own URLs (ADR-0007) has already
    // resolved it; resolving it again would corrupt it.
    assert_eq!(
        absolute("http://archive.test", "https://bucket.example/a.jpg"),
        "https://bucket.example/a.jpg"
    );
}

// US-2's other half, on the screen: photos can be added at a later time.
// Choosing files and clicking are real events (the browser layer); that
// the control is offered at all is assertable here.
// ── US-54: no single request carries every chosen photo ────────────

fn uploads(count: usize) -> Vec<PhotoUpload> {
    (0..count)
        .map(|i| PhotoUpload {
            file_name: format!("{i}.jpg"),
            content_type: Some("image/jpeg".to_string()),
            bytes: b"\xFF\xD8\xFF-fake-jpeg".to_vec(),
        })
        .collect()
}

#[tokio::test]
async fn photos_added_to_a_trip_travel_in_batches_and_all_arrive() {
    let (archive, _dir) = serve_test_archive().await;
    let id = import_sample(&archive, &[]).await;
    let mut reported = Vec::new();

    upload_in_batches(&archive, id, uploads(20), |done| reported.push(done))
        .await
        .expect("upload");

    // One report per request: 20 photos, `import::batches`' 8 to a request.
    assert_eq!(reported, vec![8, 16, 20]);
    assert_eq!(
        api::list_photos(&archive, id).await.expect("photos").len(),
        20
    );
}

#[tokio::test]
async fn a_failed_batch_says_how_many_photos_made_it_before_it() {
    let (archive, _dir) = serve_test_archive().await;

    let failed = upload_in_batches(&archive, 9_999, uploads(3), |_| {})
        .await
        .expect_err("there is no such trip to add to");

    assert_eq!(failed.uploaded, 0);
}

#[test]
fn the_screen_offers_a_way_to_add_photos_later() {
    let html = render(|| {
        rsx! { AddPhotos { id: 1, on_added: move |_| {} } }
    });

    assert!(html.contains(r#"type="file""#), "{html}");
    assert!(html.contains("multiple"), "{html}");
    // The action row's "Add photos" is what opens this; the form's own
    // button says what it does next (US-62).
    assert!(html.contains("Upload"), "{html}");
}
