use axum::{
    extract::{Path, State},
    response::Html,
    routing::{get, post},
    Router,
};

use crate::models::TripDetail;
use crate::server::{error::AppError, import::handle_import, repo, state::AppState};

/// Build the application router. Shared by `main` and the integration tests so
/// both exercise the exact same routing (ADR-0012).
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/import", post(handle_import))
        .route("/trips/:id", get(trip_detail))
        .with_state(state)
}

/// GET `/` — the import form (US-1: the owner uploads a GPX file).
async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

/// GET `/trips/:id` — the trip detail page (the redirect target after import).
async fn trip_detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Html<String>, AppError> {
    let trip = repo::get_trip(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Html(render_detail(&trip)))
}

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Trip Archive — Import</title>
</head>
<body>
  <h1>Import a Trip</h1>
  <form method="post" action="/api/import" enctype="multipart/form-data">
    <p>
      <label for="name">Trip name (optional)</label><br>
      <input type="text" id="name" name="name"
             placeholder="leave blank to use the GPX track name">
    </p>
    <p>
      <label for="activity_type">Activity (optional)</label><br>
      <select id="activity_type" name="activity_type">
        <option value="">— unspecified —</option>
        <option value="hiking">Hiking</option>
        <option value="cycling">Cycling</option>
        <option value="running">Running</option>
        <option value="skiing">Skiing</option>
      </select>
    </p>
    <p>
      <label for="gpx">GPX file</label><br>
      <input type="file" id="gpx" name="gpx" accept=".gpx,application/gpx+xml" required>
    </p>
    <button type="submit">Import</button>
  </form>
</body>
</html>"#;

/// Render the trip detail page. Minimal for US-1 — the map, elevation chart and
/// photo gallery come in later milestones (US-7).
fn render_detail(trip: &TripDetail) -> String {
    let distance_km = trip.distance_m / 1000.0;
    let ascent = trip.ascent_m.map(fmt_metres).unwrap_or_else(dash);
    let descent = trip.descent_m.map(fmt_metres).unwrap_or_else(dash);
    let duration = trip.duration_secs.map(fmt_duration).unwrap_or_else(dash);
    let start = trip.start_time.clone().unwrap_or_else(dash);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>{name}</title></head>
<body>
  <h1>{name}</h1>
  <p><strong>Activity:</strong> {activity}</p>
  <p><strong>Start:</strong> {start}</p>
  <ul>
    <li>Distance: {distance:.2} km</li>
    <li>Ascent: {ascent}</li>
    <li>Descent: {descent}</li>
    <li>Duration: {duration}</li>
  </ul>
  <p><a href="/">← Import another trip</a></p>
</body>
</html>"#,
        name = html_escape(&trip.name),
        activity = html_escape(&trip.activity_type),
        start = html_escape(&start),
        distance = distance_km,
        ascent = ascent,
        descent = descent,
        duration = duration,
    )
}

fn fmt_metres(m: f64) -> String {
    format!("{m:.0} m")
}

fn fmt_duration(secs: i64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    format!("{h:02}:{m:02}:{s:02}")
}

fn dash() -> String {
    "—".to_string()
}

/// Minimal HTML escaping for the small set of fields we interpolate.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
