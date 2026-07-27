//! Query-string → `repo::TripFilter` translation (US-13, ADR-0008/0011),
//! shared by the HTML trip list (`GET /`) and the JSON list (`GET /api/trips`)
//! in `http.rs` so both filter identically.

use serde::Deserialize;
use time::Date;

use crate::models::{normalize_tag_name, ActivityType, BoundingBox, TripKind};
use crate::server::{error::AppError, repo::TripFilter};

/// `"[year]-[month]-[day]"`, built once at compile time (rather than
/// re-parsed on every request) via the `time` crate's `macros` feature.
const DATE_FORMAT: &[time::format_description::FormatItem<'_>] =
    time::macros::format_description!("[year]-[month]-[day]");

/// The raw query-string shape both `GET /` and `GET /api/trips` accept —
/// ADR-0008 fixes these exact parameter names. `min_dist`/`max_dist` are in
/// kilometres, matching how distance is shown everywhere else in the UI;
/// `parse_filter` converts to the DB's metres.
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
    /// Comma-separated tag names (US-38) — a trip must carry all of them to
    /// match. Comma-joined rather than repeated `tags=`/`tags=` keys because
    /// axum's `Query` extractor (via `serde_urlencoded`) cannot deserialize
    /// repeated query keys into a `Vec` field; kept as a raw string, like
    /// every other field here, and split/validated in `parse_filter`.
    pub tags: Option<String>,
    /// The map-drawn region (US-14) as `minLon,minLat,maxLon,maxLat` — the
    /// exact parameter shape ADR-0008 fixes. Kept as a raw string like every
    /// other field here, and split/validated in `parse_filter`.
    pub bbox: Option<String>,
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

    let tags = parse_tags(query.tags.as_deref())?;
    let region = parse_optional_bbox(query.bbox.as_deref())?;

    Ok(TripFilter {
        activity_type,
        from,
        to,
        min_dist_m: min_dist_km.map(|km| km * 1000.0),
        max_dist_m: max_dist_km.map(|km| km * 1000.0),
        name_query,
        trip_kind,
        tags,
        region,
    })
}

/// Blank → `None` ("no region selected", which is what the filter form's
/// untouched region control submits); otherwise exactly four comma-separated
/// numbers in ADR-0008's `minLon,minLat,maxLon,maxLat` order, each finite and
/// within its coordinate range.
///
/// A backwards range on either axis is a 400 rather than a silently empty
/// result: on longitude that is also how a rectangle crossing the antimeridian
/// would arrive, and ADR-0011's v1 rectangle doesn't wrap — better to say so
/// than to return "no trips" for a region the owner did select. Non-finite
/// values are rejected for the same reason `parse_optional_distance_km`
/// rejects them: SQLite binds a `NaN` `REAL` parameter as `NULL`, which the
/// query can't tell apart from "no filter".
fn parse_optional_bbox(s: Option<&str>) -> Result<Option<BoundingBox>, AppError> {
    let raw = match s.map(str::trim) {
        None | Some("") => return Ok(None),
        Some(value) => value,
    };

    let parts: Vec<&str> = raw.split(',').collect();
    let [min_lon, min_lat, max_lon, max_lat] = parts.as_slice() else {
        return Err(AppError::BadRequest(format!(
            "invalid bbox (expected minLon,minLat,maxLon,maxLat): {raw:?}"
        )));
    };

    let min_lon = parse_coordinate(min_lon, "longitude", -180.0, 180.0)?;
    let min_lat = parse_coordinate(min_lat, "latitude", -90.0, 90.0)?;
    let max_lon = parse_coordinate(max_lon, "longitude", -180.0, 180.0)?;
    let max_lat = parse_coordinate(max_lat, "latitude", -90.0, 90.0)?;

    if min_lon > max_lon {
        return Err(AppError::BadRequest(format!(
            "bbox min longitude ({min_lon}) must not be greater than max longitude ({max_lon})"
        )));
    }
    if min_lat > max_lat {
        return Err(AppError::BadRequest(format!(
            "bbox min latitude ({min_lat}) must not be greater than max latitude ({max_lat})"
        )));
    }

    Ok(Some(BoundingBox {
        min_lat,
        min_lon,
        max_lat,
        max_lon,
    }))
}

/// One bbox coordinate: a finite number within `[min, max]` degrees.
fn parse_coordinate(s: &str, axis: &str, min: f64, max: f64) -> Result<f64, AppError> {
    let value: f64 = s.trim().parse().map_err(|_| {
        AppError::BadRequest(format!("invalid bbox {axis} (expected a number): {s:?}"))
    })?;
    if !value.is_finite() || value < min || value > max {
        return Err(AppError::BadRequest(format!(
            "bbox {axis} must be between {min} and {max}: {s:?}"
        )));
    }
    Ok(value)
}

/// Blank → no filter (empty `Vec`); otherwise split on `,`, normalize each
/// name the same way US-33/US-34 normalize a tag on write (so `Alps` in the
/// URL matches a stored `alps`), and dedupe. Splitting on `,` is unambiguous
/// because `normalize_tag_name` rejects a comma in a tag name at creation
/// time — no stored tag can ever contain one, so a segment here can never be
/// a fragment of some other, comma-containing tag. A segment that isn't a
/// well-formed tag name (whitespace, a comma, or empty) is rejected with 400
/// — same as an unrecognized `activity`/`kind` value — since that can never
/// be a real stored tag; a well-formed but *nonexistent* tag name is not an
/// error, it simply matches no trips.
fn parse_tags(s: Option<&str>) -> Result<Vec<String>, AppError> {
    let raw = match s.map(str::trim) {
        None | Some("") => return Ok(Vec::new()),
        Some(value) => value,
    };

    let mut tags = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let name = normalize_tag_name(part).map_err(AppError::BadRequest)?;
        if !tags.contains(&name) {
            tags.push(name);
        }
    }
    Ok(tags)
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
mod tests;
