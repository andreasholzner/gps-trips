//! Tests for the query-string → `TripFilter` translation (written first —
//! ADR-0012). Split out of `filter.rs` to keep that file under the repo's
//! 500-line cap.

use super::*;

fn query(f: impl FnOnce(&mut TripFilterQuery)) -> TripFilterQuery {
    let mut q = TripFilterQuery::default();
    f(&mut q);
    q
}

#[test]
fn empty_query_produces_no_filters() {
    let filter = parse_filter(&TripFilterQuery::default()).unwrap();
    assert!(filter.activity_type.is_none());
    assert!(filter.from.is_none());
    assert!(filter.to.is_none());
    assert!(filter.min_dist_m.is_none());
    assert!(filter.max_dist_m.is_none());
    assert!(filter.name_query.is_none());
    assert!(filter.trip_kind.is_none());
    assert!(filter.tags.is_empty());
    assert!(filter.region.is_none());
}

#[test]
fn blank_tags_means_no_filter() {
    let q = query(|q| q.tags = Some("   ".to_string()));
    assert!(parse_filter(&q).unwrap().tags.is_empty());
}

#[test]
fn a_single_tag_is_parsed() {
    let q = query(|q| q.tags = Some("alps".to_string()));
    assert_eq!(parse_filter(&q).unwrap().tags, vec!["alps".to_string()]);
}

#[test]
fn comma_separated_tags_are_split_and_trimmed() {
    let q = query(|q| q.tags = Some(" alps , hiking ".to_string()));
    assert_eq!(
        parse_filter(&q).unwrap().tags,
        vec!["alps".to_string(), "hiking".to_string()]
    );
}

#[test]
fn tags_are_normalized_like_us33s_write_path() {
    let q = query(|q| q.tags = Some("Alps".to_string()));
    assert_eq!(parse_filter(&q).unwrap().tags, vec!["alps".to_string()]);
}

#[test]
fn duplicate_tags_are_deduplicated() {
    let q = query(|q| q.tags = Some("alps,alps".to_string()));
    assert_eq!(parse_filter(&q).unwrap().tags, vec!["alps".to_string()]);
}

#[test]
fn a_tag_containing_whitespace_is_rejected_with_bad_request() {
    let q = query(|q| q.tags = Some("day trip".to_string()));
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn a_stray_comma_is_tolerated() {
    let q = query(|q| q.tags = Some("alps,,hiking,".to_string()));
    assert_eq!(
        parse_filter(&q).unwrap().tags,
        vec!["alps".to_string(), "hiking".to_string()]
    );
}

// ── US-14: the map-drawn region ──────────────────────────────────────

#[test]
fn blank_bbox_means_no_filter() {
    let q = query(|q| q.bbox = Some("   ".to_string()));
    assert!(parse_filter(&q).unwrap().region.is_none());
}

#[test]
fn a_valid_bbox_is_parsed_as_min_lon_min_lat_max_lon_max_lat() {
    // ADR-0008 fixes the parameter order as minLon,minLat,maxLon,maxLat —
    // lon first, unlike `BoundingBox`'s lat-first field order.
    let q = query(|q| q.bbox = Some("10.7,59.9,10.8,60.0".to_string()));
    assert_eq!(
        parse_filter(&q).unwrap().region,
        Some(BoundingBox {
            min_lat: 59.9,
            min_lon: 10.7,
            max_lat: 60.0,
            max_lon: 10.8,
        })
    );
}

#[test]
fn a_bbox_with_whitespace_around_its_numbers_is_accepted() {
    let q = query(|q| q.bbox = Some(" 10.7 , 59.9 , 10.8 , 60.0 ".to_string()));
    assert!(parse_filter(&q).unwrap().region.is_some());
}

#[test]
fn a_negative_bbox_coordinate_is_accepted() {
    // Western/southern hemispheres are ordinary input, not an error.
    let q = query(|q| q.bbox = Some("-10.5,-33.9,-9.5,-33.0".to_string()));
    assert_eq!(
        parse_filter(&q).unwrap().region,
        Some(BoundingBox {
            min_lat: -33.9,
            min_lon: -10.5,
            max_lat: -33.0,
            max_lon: -9.5,
        })
    );
}

#[test]
fn a_zero_area_bbox_is_accepted() {
    // A click rather than a drag — degenerate but unambiguous, and
    // accepted for the same reason `from == to` is.
    let q = query(|q| q.bbox = Some("10.7,59.9,10.7,59.9".to_string()));
    assert!(parse_filter(&q).unwrap().region.is_some());
}

#[test]
fn a_bbox_with_too_few_values_is_rejected_with_bad_request() {
    let q = query(|q| q.bbox = Some("10.7,59.9,10.8".to_string()));
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn a_bbox_with_too_many_values_is_rejected_with_bad_request() {
    let q = query(|q| q.bbox = Some("10.7,59.9,10.8,60.0,1".to_string()));
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn a_non_numeric_bbox_is_rejected_with_bad_request() {
    let q = query(|q| q.bbox = Some("10.7,59.9,east,60.0".to_string()));
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn a_non_finite_bbox_value_is_rejected_with_bad_request() {
    // Same reason `parse_optional_distance_km` rejects these: SQLite binds
    // a NaN REAL parameter as NULL, which the query can't tell apart from
    // "no filter".
    for raw in ["nan,59.9,10.8,60.0", "10.7,59.9,inf,60.0"] {
        let q = query(|q| q.bbox = Some(raw.to_string()));
        assert!(
            matches!(parse_filter(&q), Err(AppError::BadRequest(_))),
            "expected 400 for {raw:?}"
        );
    }
}

#[test]
fn an_out_of_range_latitude_is_rejected_with_bad_request() {
    let q = query(|q| q.bbox = Some("10.7,-91.0,10.8,60.0".to_string()));
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn an_out_of_range_longitude_is_rejected_with_bad_request() {
    let q = query(|q| q.bbox = Some("10.7,59.9,180.5,60.0".to_string()));
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn a_backwards_latitude_range_is_rejected_with_bad_request() {
    let q = query(|q| q.bbox = Some("10.7,60.0,10.8,59.9".to_string()));
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn a_backwards_longitude_range_is_rejected_with_bad_request() {
    // Which is also how a rectangle crossing the antimeridian would look;
    // ADR-0011's v1 rectangle doesn't wrap, so this is a 400 rather than a
    // silently empty result.
    let q = query(|q| q.bbox = Some("179.0,59.9,-179.0,60.0".to_string()));
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn blank_kind_means_no_filter() {
    let q = query(|q| q.kind = Some(String::new()));
    assert!(parse_filter(&q).unwrap().trip_kind.is_none());
}

#[test]
fn a_valid_kind_is_parsed() {
    let q = query(|q| q.kind = Some("planned".to_string()));
    assert_eq!(
        parse_filter(&q).unwrap().trip_kind,
        Some(crate::models::TripKind::Planned)
    );
}

#[test]
fn unrecognized_kind_is_rejected_with_bad_request() {
    let q = query(|q| q.kind = Some("hypothetical".to_string()));
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn blank_activity_means_no_filter() {
    let q = query(|q| q.activity = Some(String::new()));
    assert!(parse_filter(&q).unwrap().activity_type.is_none());
}

#[test]
fn activity_is_trimmed_before_parsing_like_import_and_edit_do() {
    let q = query(|q| q.activity = Some("  cycling  ".to_string()));
    assert_eq!(
        parse_filter(&q).unwrap().activity_type,
        Some(ActivityType::Cycling)
    );
}

#[test]
fn explicit_unknown_activity_is_a_valid_filter_value() {
    let q = query(|q| q.activity = Some("unknown".to_string()));
    assert_eq!(
        parse_filter(&q).unwrap().activity_type,
        Some(ActivityType::Unknown)
    );
}

#[test]
fn unrecognized_activity_is_rejected_with_bad_request() {
    let q = query(|q| q.activity = Some("unicycling".to_string()));
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn blank_from_and_to_mean_no_filter() {
    let q = query(|q| {
        q.from = Some("   ".to_string());
        q.to = Some(String::new());
    });
    let filter = parse_filter(&q).unwrap();
    assert!(filter.from.is_none());
    assert!(filter.to.is_none());
}

#[test]
fn a_valid_date_round_trips_unchanged() {
    let q = query(|q| q.from = Some("2024-06-01".to_string()));
    assert_eq!(
        parse_filter(&q).unwrap().from.as_deref(),
        Some("2024-06-01")
    );
}

#[test]
fn an_invalid_date_is_rejected_with_bad_request() {
    let q = query(|q| q.to = Some("2024-13-40".to_string()));
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn from_after_to_is_rejected_with_bad_request() {
    let q = query(|q| {
        q.from = Some("2024-06-10".to_string());
        q.to = Some("2024-06-01".to_string());
    });
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn from_equal_to_to_is_accepted() {
    let q = query(|q| {
        q.from = Some("2024-06-01".to_string());
        q.to = Some("2024-06-01".to_string());
    });
    assert!(parse_filter(&q).is_ok());
}

#[test]
fn blank_min_and_max_dist_mean_no_filter() {
    let q = query(|q| {
        q.min_dist = Some("   ".to_string());
        q.max_dist = Some(String::new());
    });
    let filter = parse_filter(&q).unwrap();
    assert!(filter.min_dist_m.is_none());
    assert!(filter.max_dist_m.is_none());
}

#[test]
fn min_max_dist_are_converted_from_km_to_metres() {
    let q = query(|q| {
        q.min_dist = Some("1.5".to_string());
        q.max_dist = Some("10".to_string());
    });
    let filter = parse_filter(&q).unwrap();
    assert_eq!(filter.min_dist_m, Some(1500.0));
    assert_eq!(filter.max_dist_m, Some(10_000.0));
}

#[test]
fn non_numeric_dist_is_rejected_with_bad_request() {
    let q = query(|q| q.min_dist = Some("abc".to_string()));
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn nan_dist_is_rejected_with_bad_request() {
    let q = query(|q| q.min_dist = Some("nan".to_string()));
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn infinite_dist_is_rejected_with_bad_request() {
    let q = query(|q| q.max_dist = Some("inf".to_string()));
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn negative_dist_is_rejected_with_bad_request() {
    let q = query(|q| q.min_dist = Some("-5".to_string()));
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn min_dist_greater_than_max_dist_is_rejected_with_bad_request() {
    let q = query(|q| {
        q.min_dist = Some("50".to_string());
        q.max_dist = Some("5".to_string());
    });
    assert!(matches!(parse_filter(&q), Err(AppError::BadRequest(_))));
}

#[test]
fn min_dist_equal_to_max_dist_is_accepted() {
    let q = query(|q| {
        q.min_dist = Some("5".to_string());
        q.max_dist = Some("5".to_string());
    });
    assert!(parse_filter(&q).is_ok());
}

#[test]
fn blank_name_query_is_no_filter() {
    let q = query(|q| q.q = Some("   ".to_string()));
    assert!(parse_filter(&q).unwrap().name_query.is_none());
}

#[test]
fn name_query_is_trimmed() {
    let q = query(|q| q.q = Some("  oslo  ".to_string()));
    assert_eq!(
        parse_filter(&q).unwrap().name_query.as_deref(),
        Some("oslo")
    );
}
