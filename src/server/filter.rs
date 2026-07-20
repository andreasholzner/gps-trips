//! Query-string → `repo::TripFilter` translation (US-13, ADR-0008/0011),
//! shared by the HTML trip list (`GET /`) and the JSON list (`GET /api/trips`)
//! in `http.rs` so both filter identically.

use serde::Deserialize;
use time::Date;

use crate::models::{ActivityType, TripKind};
use crate::server::{error::AppError, repo::TripFilter};

/// `"[year]-[month]-[day]"`, built once at compile time (rather than
/// re-parsed on every request) via the `time` crate's `macros` feature.
const DATE_FORMAT: &[time::format_description::FormatItem<'_>] =
    time::macros::format_description!("[year]-[month]-[day]");

/// The raw query-string shape both `GET /` and `GET /api/trips` accept —
/// ADR-0008 fixes these exact parameter names (except `bbox`, which belongs
/// to US-14's geographic-region filter and is out of v1 scope). `min_dist`/
/// `max_dist` are in kilometres, matching how distance is shown everywhere
/// else in the UI; `parse_filter` converts to the DB's metres.
///
/// `min_dist`/`max_dist` are kept as raw strings (not `f64`) so that a blank
/// value — which is exactly what a real `<form method="get">` submits for an
/// untouched `<input type="number">` — doesn't fail axum's `Query` extractor
/// itself before `parse_filter` ever runs; blank is instead handled uniformly
/// with every other "no filter" case, and a genuinely invalid value gets the
/// app's own `AppError::BadRequest` rather than axum's raw rejection body.
#[derive(Debug, Default, Deserialize)]
pub struct TripFilterQuery {
    pub activity: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub min_dist: Option<String>,
    pub max_dist: Option<String>,
    pub q: Option<String>,
    /// Recorded vs. planned (US-32). Blank/absent means "don't filter on
    /// this dimension" here, same as every other field — the trip-list page
    /// (`http::trip_list`) is what turns an absent value into "default to
    /// the Recorded tab", not this shared parser.
    pub kind: Option<String>,
}

/// Parse a raw query into a `TripFilter`, validating each field at this HTTP
/// boundary: a blank value for any field means "don't filter on this
/// dimension" (matching what an unfilled form field submits); a non-blank but
/// invalid value (unrecognized activity, malformed date, non-finite/negative
/// distance, or a `from`/`to`/`min_dist`/`max_dist` range given backwards) is
/// rejected with 400 rather than silently matching nothing.
pub fn parse_filter(query: &TripFilterQuery) -> Result<TripFilter, AppError> {
    let activity_type = match query.activity.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(value) => Some(
            value
                .parse::<ActivityType>()
                .map_err(AppError::BadRequest)?,
        ),
    };

    let from = parse_optional_date(query.from.as_deref())?;
    let to = parse_optional_date(query.to.as_deref())?;
    if let (Some(from), Some(to)) = (&from, &to) {
        if from > to {
            return Err(AppError::BadRequest(format!(
                "'from' ({from}) must not be after 'to' ({to})"
            )));
        }
    }

    let min_dist_km = parse_optional_distance_km(query.min_dist.as_deref())?;
    let max_dist_km = parse_optional_distance_km(query.max_dist.as_deref())?;
    if let (Some(min), Some(max)) = (min_dist_km, max_dist_km) {
        if min > max {
            return Err(AppError::BadRequest(format!(
                "min_dist ({min} km) must not be greater than max_dist ({max} km)"
            )));
        }
    }

    let name_query = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let trip_kind = match query.kind.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(value) => Some(value.parse::<TripKind>().map_err(AppError::BadRequest)?),
    };

    Ok(TripFilter {
        activity_type,
        from,
        to,
        min_dist_m: min_dist_km.map(|km| km * 1000.0),
        max_dist_m: max_dist_km.map(|km| km * 1000.0),
        name_query,
        trip_kind,
    })
}

/// Blank (absent, empty, or whitespace-only) → `None` ("no filter"); anything
/// else validated as a real `YYYY-MM-DD` calendar date and returned unchanged
/// — `TripFilter` keeps it as a plain string since `repo::list_trips` only
/// ever compares it against `start_time` textually, never parses it further.
fn parse_optional_date(s: Option<&str>) -> Result<Option<String>, AppError> {
    match s.map(str::trim) {
        None | Some("") => Ok(None),
        Some(value) => Date::parse(value, DATE_FORMAT)
            .map(|_| Some(value.to_string()))
            .map_err(|_| {
                AppError::BadRequest(format!("invalid date (expected YYYY-MM-DD): {value:?}"))
            }),
    }
}

/// Blank → `None` ("no filter"); anything else parsed as a finite,
/// non-negative number of kilometres, or rejected with 400. Rejecting
/// `NaN`/negative here (rather than passing them through to SQL unchecked)
/// matters because SQLite silently binds a `NaN` `REAL` parameter as `NULL` —
/// which this feature's `IS NULL`-based "no filter" check can't tell apart
/// from the dimension never having been set at all.
fn parse_optional_distance_km(s: Option<&str>) -> Result<Option<f64>, AppError> {
    match s.map(str::trim) {
        None | Some("") => Ok(None),
        Some(value) => {
            let km: f64 = value.parse().map_err(|_| {
                AppError::BadRequest(format!("invalid distance (expected a number): {value:?}"))
            })?;
            if !km.is_finite() || km < 0.0 {
                return Err(AppError::BadRequest(format!(
                    "distance must be a non-negative number of km: {value:?}"
                )));
            }
            Ok(Some(km))
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
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
}
