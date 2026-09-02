//! HTML page rendering (the Komoot review page) — kept separate from
//! `http.rs`'s routing/handlers, mirroring how `delete.rs`/`edit.rs`/
//! `import.rs` already isolate their own concerns rather than folding
//! everything into one file.
//!
//! This is the last of the proof-of-concept UI. The trip-list page was
//! deleted by US-52, the trip detail page by US-42 and the import form by
//! US-43, each once the SPA carried its acceptance assertions
//! ([ADR-0012](../../../docs/adr/0012-tdd-test-strategy.md)'s migration
//! rule); the Komoot review page goes with US-44.

use crate::models::SyncCandidate;
use crate::server::komoot_sync::SyncResultQuery;

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
    use crate::models::TripKind;

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
