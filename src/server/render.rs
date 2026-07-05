//! HTML page rendering (import form, trip list, trip detail) — kept separate
//! from `http.rs`'s routing/handlers, mirroring how `delete.rs`/`edit.rs`/
//! `import.rs` already isolate their own concerns rather than folding
//! everything into one file.

use crate::models::{ActivityType, TripDetail, TripSummary};

/// GET `/import` — the import form (US-1: the owner uploads a GPX file).
/// The `{options}` placeholder is filled via `format!`, not a runtime
/// string-replace: if a future edit ever drops the placeholder from the
/// template while `options` is still passed, the build fails immediately
/// (an unused named argument is a compile error) instead of silently
/// shipping a `<select>` with the literal text `{options}` in it.
pub fn render_import_form() -> String {
    format!(
        r#"<!DOCTYPE html>
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
        {options}
      </select>
    </p>
    <p>
      <label for="timezone">Photo timezone override (optional)</label><br>
      <input type="text" id="timezone" name="timezone"
             placeholder="auto-detected from the track's start location if left blank, e.g. Europe/Oslo">
    </p>
    <p>
      <label for="gpx">GPX file</label><br>
      <input type="file" id="gpx" name="gpx" accept=".gpx,application/gpx+xml" required>
    </p>
    <p>
      <label for="photos">Photos (optional)</label><br>
      <input type="file" id="photos" name="photos" accept="image/*" multiple>
    </p>
    <button type="submit">Import</button>
  </form>
  <p><a href="/">← All trips</a></p>
</body>
</html>"#,
        options = activity_type_options("")
    )
}

/// Build the `<option>` list for an activity-type `<select>`, marking
/// `selected` as chosen. Shared by the import form and the trip detail edit
/// form (US-15). Iterates `ActivityType::SELECTABLE`/`label()` — the single
/// canonical variant list in `models::activity_type` — instead of a second,
/// hand-maintained copy here, so the two forms can't drift out of sync with
/// `ActivityType`'s actual variants.
fn activity_type_options(selected: &str) -> String {
    let mut options = format!(
        "<option value=\"\"{sel}>{label}</option>\n",
        sel = if selected.is_empty() { " selected" } else { "" },
        label = ActivityType::Unknown.label(),
    );
    for activity in ActivityType::SELECTABLE {
        let value = activity.as_str();
        let sel = if value == selected { " selected" } else { "" };
        options.push_str(&format!(
            "<option value=\"{value}\"{sel}>{label}</option>\n",
            label = activity.label()
        ));
    }
    options
}

/// The `<select>` option value matching a trip's current activity type
/// (US-15's edit form pre-selects it): `Unknown` shows as "— unspecified —"
/// (value `""`), the same as a not-yet-chosen import — every other variant
/// uses its own wire value.
fn activity_type_select_value(activity: ActivityType) -> &'static str {
    if activity == ActivityType::Unknown {
        ""
    } else {
        activity.as_str()
    }
}

/// Render the trip detail page — relive a trip (US-7): the track on an OSM map,
/// an elevation profile, and a photo gallery.
///
/// The map and chart are driven from a single track-GeoJSON fetch (ADR-0005/0006);
/// the gallery fetches the photos JSON (US-2) and renders `<img>` elements. The
/// page only emits the containers and the vendored, self-hosted assets (US-10);
/// data URLs are handed to the client via `data-*` attributes on `<body>`.
pub fn render_detail(trip: &TripDetail) -> String {
    let distance_km = trip.distance_m / 1000.0;
    let ascent = trip.ascent_m.map(fmt_metres).unwrap_or_else(dash);
    let descent = trip.descent_m.map(fmt_metres).unwrap_or_else(dash);
    let duration = trip.duration_secs.map(fmt_duration).unwrap_or_else(dash);
    let start = trip.start_time.clone().unwrap_or_else(dash);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{name}</title>
  <link rel="stylesheet" href="/static/vendor/leaflet.css">
  <link rel="stylesheet" href="/static/vendor/uPlot.min.css">
  <style>
    #map {{ height: 24rem; }}
    #elevation {{ margin-top: 1rem; }}
  </style>
</head>
<body data-track-url="/api/trips/{id}/track.geojson"
      data-photos-url="/api/trips/{id}/photos"
      data-trip-id="{id}">
  <h1 id="trip-name">{name}</h1>
  <p><strong>Activity:</strong> <span id="trip-activity">{activity}</span></p>
  <p><button id="edit-trip">Edit name / activity</button></p>
  <form id="edit-trip-form" style="display:none">
    <p>
      <label for="edit-name">Name</label><br>
      <input type="text" id="edit-name" name="name" value="{name}">
    </p>
    <p>
      <label for="edit-activity_type">Activity</label><br>
      <select id="edit-activity_type" name="activity_type">
        {activity_options}
      </select>
    </p>
    <button type="submit" id="edit-trip-save">Save</button>
    <button type="button" id="edit-trip-cancel">Cancel</button>
  </form>
  <p><strong>Photo timestamp timezone:</strong> {tz_name}</p>
  <p><strong>Start:</strong> {start}</p>
  <ul>
    <li>Distance: {distance:.2} km</li>
    <li>Ascent: {ascent}</li>
    <li>Descent: {descent}</li>
    <li>Duration: {duration}</li>
  </ul>

  <div id="map"></div>
  <div id="elevation"></div>

  <h2>Photos</h2>
  <div id="gallery"></div>
  <form method="post" action="/api/trips/{id}/photos" enctype="multipart/form-data">
    <input type="file" name="photos" accept="image/*" multiple>
    <button type="submit">Add photos</button>
  </form>

  <p><a href="/api/trips/{id}/gpx">Download original GPX</a></p>
  <p><button id="delete-trip">Delete trip</button></p>
  <p><a href="/">← All trips</a></p>

  <script src="/static/vendor/leaflet.js"></script>
  <script src="/static/vendor/uPlot.iife.min.js"></script>
  <script src="/static/js/trip_detail.js"></script>
</body>
</html>"#,
        id = trip.id,
        name = html_escape(&trip.name),
        activity = html_escape(trip.activity_type.as_str()),
        activity_options = activity_type_options(activity_type_select_value(trip.activity_type)),
        tz_name = html_escape(trip.tz_name.as_deref().unwrap_or("unknown")),
        start = html_escape(&start),
        distance = distance_km,
        ascent = ascent,
        descent = descent,
        duration = duration,
    )
}

/// Render the trip list page (US-6). Shows each trip's name (linking to its
/// detail), activity type (US-11), date, distance, ascent, and duration; an
/// empty state otherwise.
pub fn render_trip_list(trips: &[TripSummary]) -> String {
    let body = if trips.is_empty() {
        "<p>No trips yet. <a href=\"/import\">Import your first trip</a>.</p>".to_string()
    } else {
        let rows: String = trips.iter().map(render_trip_row).collect();
        format!(
            "<table>\n\
             <thead><tr><th>Trip</th><th>Activity</th><th>Date</th><th>Distance</th><th>Ascent</th><th>Duration</th></tr></thead>\n\
             <tbody>\n{rows}</tbody>\n\
             </table>"
        )
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Trip Archive</title>
</head>
<body>
  <h1>Trips</h1>
  <p><a href="/import">Import a trip</a></p>
  {body}
</body>
</html>"#
    )
}

/// One row of the trip list table.
fn render_trip_row(trip: &TripSummary) -> String {
    // start_time is RFC-3339 (e.g. "2024-06-01T08:00:00+00:00"); show the date part.
    let date = trip
        .start_time
        .as_deref()
        .and_then(|s| s.split('T').next())
        .unwrap_or("—");
    let distance_km = trip.distance_m / 1000.0;
    let ascent = trip.ascent_m.map(fmt_metres).unwrap_or_else(dash);
    let duration = trip.duration_secs.map(fmt_duration).unwrap_or_else(dash);

    format!(
        "<tr><td><a href=\"/trips/{id}\">{name}</a></td><td>{activity}</td>\
         <td>{date}</td><td>{distance:.2} km</td><td>{ascent}</td><td>{duration}</td></tr>\n",
        id = trip.id,
        name = html_escape(&trip.name),
        activity = html_escape(trip.activity_type.as_str()),
        date = html_escape(date),
        distance = distance_km,
        ascent = ascent,
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

/// Minimal HTML escaping for the small set of fields we interpolate — safe in
/// both text content and quoted attribute values (US-15's edit form puts the
/// trip name in a `value="..."` attribute, unlike earlier text-only uses).
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
