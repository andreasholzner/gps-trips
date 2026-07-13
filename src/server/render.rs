//! HTML page rendering (import form, trip list, trip detail) — kept separate
//! from `http.rs`'s routing/handlers, mirroring how `delete.rs`/`edit.rs`/
//! `import.rs` already isolate their own concerns rather than folding
//! everything into one file.

use crate::models::{ActivityType, TripDetail, TripSummary};
use crate::server::filter::TripFilterQuery;
use crate::server::komoot_sync::{SyncCandidate, SyncResultQuery};

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
/// empty state otherwise. `query` is the filter form's current values (US-13)
/// — echoed back into the form so a follow-up edit doesn't reset what the
/// owner already typed, and used to tell "no trips at all" apart from "no
/// trips match this filter".
pub fn render_trip_list(trips: &[TripSummary], query: &TripFilterQuery) -> String {
    let body = if trips.is_empty() {
        if any_filter_set(query) {
            "<p>No trips match your filters. <a href=\"/\">Clear filters</a>.</p>".to_string()
        } else {
            "<p>No trips yet. <a href=\"/import\">Import your first trip</a>.</p>".to_string()
        }
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
  <p><a href="/import">Import a trip</a> · <a href="/komoot/sync">Sync with Komoot</a></p>
  {filter_form}
  {body}
</body>
</html>"#,
        filter_form = render_filter_form(query),
    )
}

/// Whether any filter field in `query` is set — distinguishes "no trips at
/// all" from "no trips match this filter" in `render_trip_list`'s empty
/// state. A blank value means "not set", matching `filter::parse_filter`'s
/// own blank-handling for every field.
fn any_filter_set(query: &TripFilterQuery) -> bool {
    is_non_blank(query.activity.as_deref())
        || is_non_blank(query.from.as_deref())
        || is_non_blank(query.to.as_deref())
        || is_non_blank(query.min_dist.as_deref())
        || is_non_blank(query.max_dist.as_deref())
        || is_non_blank(query.q.as_deref())
}

fn is_non_blank(s: Option<&str>) -> bool {
    s.is_some_and(|s| !s.trim().is_empty())
}

/// The trip-list filter form (US-13): free-text name search, activity type,
/// date range, distance range (shown/submitted in km, matching how distance
/// is displayed everywhere else — `repo::TripFilter` converts to metres). A
/// plain GET form: unlike the edit/delete actions, filtering is a read, so a
/// native query-string submission needs no JS.
fn render_filter_form(query: &TripFilterQuery) -> String {
    let q = html_escape(query.q.as_deref().unwrap_or(""));
    let from = html_escape(query.from.as_deref().unwrap_or(""));
    let to = html_escape(query.to.as_deref().unwrap_or(""));
    let min_dist = html_escape(query.min_dist.as_deref().unwrap_or(""));
    let max_dist = html_escape(query.max_dist.as_deref().unwrap_or(""));
    let activity_options = activity_filter_options(query.activity.as_deref().unwrap_or(""));

    format!(
        r#"<form method="get" action="/">
  <input type="text" name="q" value="{q}" placeholder="Search by name">
  <select name="activity">
    {activity_options}
  </select>
  <label>From <input type="date" name="from" value="{from}"></label>
  <label>To <input type="date" name="to" value="{to}"></label>
  <label>Min <input type="number" step="0.1" name="min_dist" value="{min_dist}" placeholder="min km"></label>
  <label>Max <input type="number" step="0.1" name="max_dist" value="{max_dist}" placeholder="max km"></label>
  <button type="submit">Filter</button>
  <a href="/">Clear</a>
</form>"#
    )
}

/// Build the `<option>` list for the filter form's activity `<select>`
/// (US-13). Distinct from `activity_type_options`: here the blank option means
/// "don't filter on activity at all", not "unspecified" — so `Unknown` gets
/// its own explicit, filterable option rather than sharing the blank one.
fn activity_filter_options(selected: &str) -> String {
    let mut options = format!(
        "<option value=\"\"{sel}>All activities</option>\n",
        sel = if selected.is_empty() { " selected" } else { "" },
    );
    options.push_str(&format!(
        "<option value=\"unknown\"{sel}>{label}</option>\n",
        sel = if selected == "unknown" {
            " selected"
        } else {
            ""
        },
        label = ActivityType::Unknown.label(),
    ));
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

/// GET `/komoot/sync` — the "Sync now" review page (US-22): every Komoot
/// tour not yet in `trip_komoot_link`, each unchecked by default so the
/// owner opts in per tour rather than a plain submit pulling in everything
/// new at once (full historical backfill is a separate story, US-23).
/// `result` carries the previous run's outcome back from the POST redirect
/// (no session/flash mechanism here, consistent with every other page in
/// this server-rendered app).
pub fn render_sync_candidates(candidates: &[SyncCandidate], result: &SyncResultQuery) -> String {
    let banner = render_sync_result_banner(result);
    let body = if candidates.is_empty() {
        "<p>No new tours to sync — everything on Komoot is already in the archive.</p>".to_string()
    } else {
        let rows: String = candidates.iter().map(render_sync_candidate_row).collect();
        format!(
            "<form id=\"sync-form\">\n\
             <table>\n\
             <thead><tr><th></th><th>Tour</th><th>Activity</th><th>Date</th><th>Distance</th></tr></thead>\n\
             <tbody>\n{rows}</tbody>\n\
             </table>\n\
             <button type=\"submit\">Sync selected</button>\n\
             </form>"
        )
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Trip Archive — Sync with Komoot</title>
</head>
<body>
  <h1>Sync with Komoot</h1>
  {banner}
  {body}
  <p><a href="/">← All trips</a></p>
  <script src="/static/js/komoot_sync.js"></script>
</body>
</html>"#,
    )
}

/// One row of the sync candidates table: an unchecked `tour_id` checkbox
/// plus the tour's own metadata, read straight off Komoot's `list_tours`
/// response (no extra per-tour call needed — see `docs/komoot-api.md`).
fn render_sync_candidate_row(c: &SyncCandidate) -> String {
    format!(
        "<tr><td><input type=\"checkbox\" name=\"tour_id\" value=\"{id}\"></td>\
         <td>{name}</td><td>{sport}</td><td>{date}</td><td>{distance:.2} km</td></tr>\n",
        id = html_escape(&c.tour_id),
        name = html_escape(&c.name),
        sport = html_escape(&c.sport),
        date = html_escape(&c.date),
        distance = c.distance_m / 1000.0,
    )
}

/// The one-line result banner shown after a sync run redirects back here;
/// empty (no banner) on a fresh, un-redirected visit to the page.
fn render_sync_result_banner(result: &SyncResultQuery) -> String {
    if result.synced.is_none() && result.failed_tour.is_none() {
        return String::new();
    }
    let synced_msg = result
        .synced
        .map(|n| format!("Synced {n} tour(s). "))
        .unwrap_or_default();
    let failed_msg = result
        .failed_tour
        .as_deref()
        .map(|tour_id| {
            format!(
                "Failed on tour {}: {}",
                html_escape(tour_id),
                html_escape(result.failed_msg.as_deref().unwrap_or("unknown error"))
            )
        })
        .unwrap_or_default();
    format!("<p><strong>{synced_msg}{failed_msg}</strong></p>")
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
