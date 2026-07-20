//! HTML page rendering (import form, trip list, trip detail) — kept separate
//! from `http.rs`'s routing/handlers, mirroring how `delete.rs`/`edit.rs`/
//! `import.rs` already isolate their own concerns rather than folding
//! everything into one file.
//!
//! The trip-list page (US-6/US-13/US-32) lives in its own `trip_list`
//! submodule to keep this file under the repo's 500-line cap.

mod trip_list;

use crate::models::{ActivityType, TripDetail, TripKind};
use crate::server::komoot_sync::{SyncCandidate, SyncResultQuery};

pub use trip_list::render_trip_list;

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
      <label>Trip kind</label><br>
      {kind_radios}
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
        options = activity_type_options(""),
        kind_radios = trip_kind_radios(TripKind::Recorded)
    )
}

/// The Recorded/Planned radio pair for the import form (US-31), `selected`
/// pre-checked. Iterates `TripKind::ALL`/`.label()` — the single canonical
/// variant list in `models::trip_kind` — the same pattern
/// `activity_type_options` uses for `ActivityType`, so the form can't drift
/// out of sync with `TripKind`'s actual variants.
fn trip_kind_radios(selected: TripKind) -> String {
    TripKind::ALL
        .iter()
        .map(|&kind| {
            format!(
                "<label><input type=\"radio\" name=\"kind\" value=\"{value}\"{checked}> {label}</label>",
                value = kind.as_str(),
                checked = if kind == selected { " checked" } else { "" },
                label = kind.label(),
            )
        })
        .collect::<Vec<_>>()
        .join("\n      ")
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

/// GET `/komoot/sync` — the "Sync now" review page (US-20/US-22): every
/// Komoot tour not yet in `trip_komoot_link`, each unchecked by default so
/// the owner opts in per tour rather than a plain submit pulling in
/// everything new at once (full historical backfill is a separate story,
/// US-23). `pending_edit_count` (US-20) is how many trips have an edit
/// queued to push back to Komoot. `result` carries the previous run's
/// outcome back from the POST redirect (no session/flash mechanism here,
/// consistent with every other page in this server-rendered app).
///
/// The form (and its submit button) is always rendered, even with zero pull
/// candidates — a sync with nothing new to pull can still have pending edits
/// to push, and the owner needs a way to trigger that.
pub fn render_sync_candidates(
    candidates: &[SyncCandidate],
    pending_edit_count: i64,
    result: &SyncResultQuery,
) -> String {
    let banner = render_sync_result_banner(result);
    let pending_edits_note = if pending_edit_count > 0 {
        format!("<p>{pending_edit_count} pending edit(s) to push to Komoot.</p>",)
    } else {
        String::new()
    };
    let table = if candidates.is_empty() {
        "<p>No new tours to sync — everything on Komoot is already in the archive.</p>".to_string()
    } else {
        let rows: String = candidates.iter().map(render_sync_candidate_row).collect();
        format!(
            "<table>\n\
             <thead><tr><th></th><th>Tour</th><th>Kind</th><th>Activity</th><th>Date</th><th>Distance</th></tr></thead>\n\
             <tbody>\n{rows}</tbody>\n\
             </table>\n"
        )
    };
    let body = format!(
        "<form id=\"sync-form\">\n\
         {table}\
         <button type=\"submit\">Sync now</button>\n\
         </form>"
    );

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
  {pending_edits_note}
  {body}
  <p><a href="/">← All trips</a></p>
  <script src="/static/js/komoot_sync.js"></script>
</body>
</html>"#,
    )
}

/// One row of the sync candidates table: an unchecked `tour_id` checkbox
/// plus the tour's own metadata, read straight off Komoot's tour listing (no
/// extra per-tour call needed — see `docs/komoot-api.md`), and its kind
/// (Recorded/Planned, US-29) so the owner sees which tab it will land on.
fn render_sync_candidate_row(c: &SyncCandidate) -> String {
    format!(
        "<tr><td><input type=\"checkbox\" name=\"tour_id\" value=\"{id}\" data-kind=\"{kind_value}\"></td>\
         <td>{name}</td><td>{kind}</td><td>{sport}</td><td>{date}</td><td>{distance:.2} km</td></tr>\n",
        id = html_escape(&c.tour_id),
        name = html_escape(&c.name),
        kind_value = c.kind.as_str(),
        kind = c.kind.label(),
        sport = html_escape(&c.sport),
        date = html_escape(&c.date),
        distance = c.distance_m / 1000.0,
    )
}

/// The one-line result banner shown after a sync run redirects back here;
/// empty (no banner) on a fresh, un-redirected visit to the page. Reports
/// all phases (US-20's edit-push, US-24's delete-push, US-22's pull) and, on
/// a halt, which phase (`failed_phase`) the failing trip/tour belongs to.
/// A delete-push failure's `failed_msg` is prefixed `"delete tour: "` by
/// `push_pending_deletes` — the banner wording itself stays generic ("push"),
/// but that prefix keeps the underlying error traceable to which push step
/// actually failed.
fn render_sync_result_banner(result: &SyncResultQuery) -> String {
    if result.pushed.is_none()
        && result.deleted.is_none()
        && result.synced.is_none()
        && result.failed_tour.is_none()
    {
        return String::new();
    }
    let pushed_msg = result
        .pushed
        .filter(|&n| n > 0)
        .map(|n| format!("Pushed {n} edit(s). "))
        .unwrap_or_default();
    let deleted_msg = result
        .deleted
        .filter(|&n| n > 0)
        .map(|n| format!("Deleted {n} tour(s) on Komoot. "))
        .unwrap_or_default();
    let synced_msg = result
        .synced
        .map(|n| format!("Synced {n} tour(s). "))
        .unwrap_or_default();
    let failed_msg = result
        .failed_tour
        .as_deref()
        .map(|tour_id| {
            let phase = result.failed_phase.as_deref().unwrap_or("pull");
            format!(
                "Failed to {} tour {}: {}",
                if phase == "push" { "push" } else { "pull" },
                html_escape(tour_id),
                html_escape(result.failed_msg.as_deref().unwrap_or("unknown error"))
            )
        })
        .unwrap_or_default();
    format!("<p><strong>{pushed_msg}{deleted_msg}{synced_msg}{failed_msg}</strong></p>")
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

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn a_candidate(tour_id: &str, name: &str, kind: TripKind) -> SyncCandidate {
        SyncCandidate {
            tour_id: tour_id.to_string(),
            name: name.to_string(),
            sport: "hike".to_string(),
            date: "2026-07-11".to_string(),
            distance_m: 1000.0,
            kind,
        }
    }

    #[test]
    fn sync_candidates_label_each_row_by_its_kind() {
        // US-29: the review page must let the owner tell a planned route from a
        // recorded tour before importing it.
        let candidates = vec![
            a_candidate("1", "Recorded ride", TripKind::Recorded),
            a_candidate("2", "Planned route", TripKind::Planned),
        ];
        let html = render_sync_candidates(&candidates, 0, &SyncResultQuery::default());

        assert!(html.contains("<th>Kind</th>"));
        assert!(html.contains("<td>Recorded</td>"));
        assert!(html.contains("<td>Planned</td>"));
    }
}
