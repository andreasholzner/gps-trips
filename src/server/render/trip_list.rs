//! The trip list page (US-6/US-13), split into a Recorded/Planned tab
//! (US-32).

use crate::models::{normalize_tag_name, ActivityType, Tag, TripKind, TripSummary};
use crate::server::filter::TripFilterQuery;

use super::{dash, fmt_duration, fmt_metres, html_escape};

/// Render the trip list page (US-6). Shows each trip's name (linking to its
/// detail), activity type (US-11), date, distance, ascent, and duration; an
/// empty state otherwise. `query` is the filter form's current values (US-13)
/// — echoed back into the form so a follow-up edit doesn't reset what the
/// owner already typed, and used to tell "no trips at all" apart from "no
/// trips match this filter". `active_kind` (US-32) is which of the two tabs
/// `trips` belongs to — the caller (`http::trip_list`) has already resolved
/// it and filtered `trips` down to that single kind. `all_tags` (US-38) is
/// every tag that exists, alphabetical — populates the filter form's tag
/// multi-select, the same source `/api/tags` already serves the US-33/US-34
/// autocompletes from.
pub fn render_trip_list(
    trips: &[TripSummary],
    query: &TripFilterQuery,
    active_kind: TripKind,
    all_tags: &[Tag],
) -> String {
    let body = if trips.is_empty() {
        if any_filter_set(query) {
            "<p>No trips match your filters. <a href=\"/\">Clear filters</a>.</p>".to_string()
        } else if active_kind == TripKind::Planned {
            "<p>No planned trips yet.</p>".to_string()
        } else {
            "<p>No trips yet. <a href=\"/import\">Import your first trip</a>.</p>".to_string()
        }
    } else {
        let rows: String = trips.iter().map(render_trip_row).collect();
        format!(
            "<table>\n\
             <thead><tr><th><input type=\"checkbox\" id=\"select-all\"></th><th>Trip</th><th>Activity</th><th>Date</th><th>Distance</th><th>Ascent</th><th>Duration</th></tr></thead>\n\
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
  <nav>{tabs}</nav>
  {filter_form}
  {body}
  {bulk_tag_panel}
  <script src="/static/js/trip_list.js"></script>
</body>
</html>"#,
        tabs = render_kind_tabs(query, active_kind),
        filter_form = render_filter_form(query, all_tags),
        bulk_tag_panel = render_bulk_tag_panel(),
    )
}

/// The bulk-tag panel (US-34): hidden by default, `trip_list.js` shows it once
/// at least one row checkbox is checked. `#bulk-tag-input`/`#bulk-tag-suggestions`
/// mirror the detail page's `#tag-input`/`#tag-suggestions` (US-33) — same
/// `<datalist>` autocomplete, populated from the same `/api/tags` endpoint.
/// Typed names are staged as removable chips in `#bulk-tag-pending` before
/// `#bulk-tag-apply` submits all of them, for every checked trip, in one
/// `POST /api/trips/tags` request.
fn render_bulk_tag_panel() -> String {
    r#"<div id="bulk-tag-panel" style="display:none">
  <h2>Tag selected trips</h2>
  <div id="bulk-tag-pending"></div>
  <datalist id="bulk-tag-suggestions"></datalist>
  <input type="text" id="bulk-tag-input" list="bulk-tag-suggestions" placeholder="add a tag">
  <button type="button" id="bulk-tag-add">Add</button>
  <button type="button" id="bulk-tag-apply">Apply to 0 selected</button>
</div>"#
        .to_string()
}

/// The Recorded/Planned tab nav (US-32). The active tab is plain text (not a
/// link — resubmitting the current tab does nothing useful); the inactive
/// tab is a small GET form so switching tabs preserves whatever filters
/// (US-13) are currently active, the same query-string round trip the filter
/// form itself already relies on.
fn render_kind_tabs(query: &TripFilterQuery, active: TripKind) -> String {
    TripKind::ALL
        .iter()
        .map(|&kind| render_kind_tab(kind, query, active))
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_kind_tab(kind: TripKind, query: &TripFilterQuery, active: TripKind) -> String {
    if kind == active {
        format!("<strong>{}</strong>", kind.label())
    } else {
        format!(
            "<form method=\"get\" action=\"/\" style=\"display:inline\">\n\
             <input type=\"hidden\" name=\"kind\" value=\"{value}\">\n\
             {hidden_filters}\
             <button type=\"submit\">{label}</button>\n\
             </form>",
            value = kind.as_str(),
            hidden_filters = hidden_filter_inputs(query),
            label = kind.label(),
        )
    }
}

/// Hidden `<input>`s carrying every currently-active filter (US-13) other
/// than `kind` itself, so a tab-switch form resubmits them unchanged — the
/// same fields `render_filter_form` renders as visible inputs.
fn hidden_filter_inputs(query: &TripFilterQuery) -> String {
    [
        ("q", query.q.as_deref()),
        ("activity", query.activity.as_deref()),
        ("from", query.from.as_deref()),
        ("to", query.to.as_deref()),
        ("min_dist", query.min_dist.as_deref()),
        ("max_dist", query.max_dist.as_deref()),
        ("tags", query.tags.as_deref()),
    ]
    .into_iter()
    .filter_map(|(name, value)| {
        let value = value?.trim();
        (!value.is_empty()).then(|| {
            format!(
                "<input type=\"hidden\" name=\"{name}\" value=\"{value}\">\n",
                value = html_escape(value)
            )
        })
    })
    .collect()
}

/// Whether any filter field in `query` is set — distinguishes "no trips at
/// all" from "no trips match this filter" in `render_trip_list`'s empty
/// state. A blank value means "not set", matching `filter::parse_filter`'s
/// own blank-handling for every field. `kind` is deliberately excluded — it
/// selects a tab, not a "no results" narrowing.
fn any_filter_set(query: &TripFilterQuery) -> bool {
    is_non_blank(query.activity.as_deref())
        || is_non_blank(query.from.as_deref())
        || is_non_blank(query.to.as_deref())
        || is_non_blank(query.min_dist.as_deref())
        || is_non_blank(query.max_dist.as_deref())
        || is_non_blank(query.q.as_deref())
        || is_non_blank(query.tags.as_deref())
}

fn is_non_blank(s: Option<&str>) -> bool {
    s.is_some_and(|s| !s.trim().is_empty())
}

/// The trip-list filter form (US-13): free-text name search, activity type,
/// date range, distance range (shown/submitted in km, matching how distance
/// is displayed everywhere else — `repo::TripFilter` converts to metres), and
/// a tag multi-select (US-38). Every field but the tag select is a plain GET
/// form field: unlike the edit/delete actions, filtering is a read, so a
/// native query-string submission needs no JS. The tag select is the one
/// exception — `#tags-select` deliberately has no `name` (so a plain
/// multi-select submission can't produce the repeated `tags=`/`tags=` query
/// keys axum's `Query` extractor can't parse into a `Vec`); `trip_list.js`
/// joins its selected `<option>`s into the actual submitted field, the
/// comma-separated hidden `<input name="tags">`, on form submit.
fn render_filter_form(query: &TripFilterQuery, all_tags: &[Tag]) -> String {
    let q = html_escape(query.q.as_deref().unwrap_or(""));
    let from = html_escape(query.from.as_deref().unwrap_or(""));
    let to = html_escape(query.to.as_deref().unwrap_or(""));
    let min_dist = html_escape(query.min_dist.as_deref().unwrap_or(""));
    let max_dist = html_escape(query.max_dist.as_deref().unwrap_or(""));
    let tags_value = html_escape(query.tags.as_deref().unwrap_or(""));
    let activity_options = activity_filter_options(query.activity.as_deref().unwrap_or(""));
    let tags_options = tag_filter_options(query.tags.as_deref().unwrap_or(""), all_tags);

    format!(
        r#"<form method="get" action="/" id="filter-form">
  <input type="text" name="q" value="{q}" placeholder="Search by name">
  <select name="activity">
    {activity_options}
  </select>
  <label>From <input type="date" name="from" value="{from}"></label>
  <label>To <input type="date" name="to" value="{to}"></label>
  <label>Min <input type="number" step="0.1" name="min_dist" value="{min_dist}" placeholder="min km"></label>
  <label>Max <input type="number" step="0.1" name="max_dist" value="{max_dist}" placeholder="max km"></label>
  <label>Tags
    <select id="tags-select" multiple>
      {tags_options}
    </select>
  </label>
  <input type="hidden" id="tags-input" name="tags" value="{tags_value}">
  <button type="submit">Filter</button>
  <a href="/">Clear</a>
</form>"#
    )
}

/// Build the `<option>` list for the filter form's tag `<select multiple>`
/// (US-38): every tag that exists, `selected` if its name is one of the
/// comma-separated names in `selected_csv` — the same value the hidden
/// `tags` input carries and `filter::parse_filter` parses. Each segment is
/// normalized the same way `parse_filter` normalizes it before matching
/// against `tag.name` (always stored normalized) — so a hand-edited or
/// shared URL like `?tags=Alps` still shows `alps` as selected, matching
/// what the query actually filtered by, not just an exact-casing echo of the
/// raw query string. A segment that fails normalization (only possible if
/// `render_trip_list` is ever called with a query that didn't go through
/// `parse_filter` first) is simply not selectable, rather than panicking.
fn tag_filter_options(selected_csv: &str, all_tags: &[Tag]) -> String {
    let selected: std::collections::HashSet<String> = selected_csv
        .split(',')
        .filter_map(|s| normalize_tag_name(s.trim()).ok())
        .collect();

    all_tags
        .iter()
        .map(|tag| {
            let name = html_escape(&tag.name);
            let sel = if selected.contains(&tag.name) {
                " selected"
            } else {
                ""
            };
            format!("<option value=\"{name}\"{sel}>{name}</option>\n")
        })
        .collect()
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
        "<tr><td><input type=\"checkbox\" class=\"trip-select\" value=\"{id}\"></td>\
         <td><a href=\"/trips/{id}\">{name}</a></td><td>{activity}</td>\
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
