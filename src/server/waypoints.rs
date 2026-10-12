//! One waypoint of a collection over the JSON API (US-98, ADR-0008): read
//! for its page, corrected, and deleted. A waypoint is a record of its
//! collection (ADR-0028).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};

use crate::models::{WaypointDetail, WaypointEdit};
use crate::server::{
    collections::edited_name,
    error::AppError,
    repo::{self, WaypointChange},
    state::AppState,
};

/// The longest symbol taken: Garmin's longest names are a few dozen
/// characters, and the field is free text from someone's file.
const MAX_SYMBOL_LEN: usize = 100;

/// The waypoint routes, merged into the owner's router behind its gate.
pub fn router() -> Router<AppState> {
    Router::new().route(
        "/api/waypoints/:id",
        get(handle_get).patch(handle_edit).delete(handle_delete),
    )
}

/// `GET /api/waypoints/:id` — the waypoint, its collection and its track.
async fn handle_get(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<WaypointDetail>, AppError> {
    let waypoint = repo::get_waypoint(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(waypoint))
}

/// An optional text field as edited: trimmed, and blank takes it away.
fn optional_text(value: Option<&String>) -> Option<Option<&str>> {
    value.map(|text| Some(text.trim()).filter(|text| !text.is_empty()))
}

/// The elevation as the form holds it: blank takes it away, anything else
/// must be a number of metres.
fn edited_ele(value: Option<&String>) -> Result<Option<Option<f64>>, AppError> {
    let Some(text) = optional_text(value) else {
        return Ok(None);
    };
    text.map(|text| {
        text.parse::<f64>()
            .ok()
            .filter(|ele| ele.is_finite())
            .ok_or_else(|| {
                AppError::BadRequest(format!(
                    "elevation must be a number of metres, not {text:?}"
                ))
            })
    })
    .transpose()
    .map(Some)
}

/// A new position: both halves or neither, each in range.
fn edited_position(lat: Option<f64>, lon: Option<f64>) -> Result<Option<(f64, f64)>, AppError> {
    match (lat, lon) {
        (None, None) => Ok(None),
        (Some(lat), Some(lon))
            if (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) =>
        {
            Ok(Some((lat, lon)))
        }
        (Some(_), Some(_)) => Err(AppError::BadRequest(
            "lat must be within ±90 and lon within ±180".to_string(),
        )),
        _ => Err(AppError::BadRequest(
            "a position needs both lat and lon".to_string(),
        )),
    }
}

/// `PATCH /api/waypoints/:id` — correct the name, description, symbol,
/// elevation or position. 400 for a blank name, an elevation that is no
/// number, an overlong symbol or half a position; 404 for no such waypoint.
async fn handle_edit(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(edit): Json<WaypointEdit>,
) -> Result<StatusCode, AppError> {
    let name = edited_name(edit.name)?;
    let symbol = optional_text(edit.symbol.as_ref());
    if symbol
        .flatten()
        .is_some_and(|symbol| symbol.chars().count() > MAX_SYMBOL_LEN)
    {
        return Err(AppError::BadRequest(format!(
            "a symbol is at most {MAX_SYMBOL_LEN} characters"
        )));
    }
    let change = WaypointChange {
        name: name.as_deref(),
        description: optional_text(edit.description.as_ref()),
        symbol,
        ele: edited_ele(edit.ele.as_ref())?,
        position: edited_position(edit.lat, edit.lon)?,
    };
    if !repo::update_waypoint(&state.pool, id, &change).await? {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/waypoints/:id` — the waypoint alone; its collection and
/// tracks stay.
async fn handle_delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    if !repo::delete_waypoint(&state.pool, id).await? {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
