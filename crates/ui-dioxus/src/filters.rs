//! The trip-list filter state (US-13/US-32) and its translation into the
//! query string `GET /api/trips` already accepts (ADR-0008/0011) — the same
//! parameter names the server-rendered list submits.
//!
//! Pure and Dioxus-free so it is unit-testable on the host; the components
//! only hold it in a signal and hand it to `api::list_trips`.

use trip_archive_types::{ActivityType, TripKind};

/// Everything the list screen can narrow itself by. Every text field is kept
/// as the raw string the input holds: blank means "don't filter on this
/// dimension", exactly as the server's own parser reads it, so a half-typed
/// value never needs a separate "is it valid yet" state here.
#[derive(Clone, Debug, PartialEq)]
pub struct Filters {
    /// Which tab is showing (US-32). Always concrete — the list is one tab's
    /// worth of trips, never both at once.
    pub kind: TripKind,
    pub q: String,
    pub activity: Option<ActivityType>,
    pub from: String,
    pub to: String,
    /// Kilometres, the unit `min_dist`/`max_dist` are documented in.
    pub min_dist: String,
    pub max_dist: String,
}

impl Default for Filters {
    fn default() -> Self {
        Self {
            kind: TripKind::Recorded,
            q: String::new(),
            activity: None,
            from: String::new(),
            to: String::new(),
            min_dist: String::new(),
            max_dist: String::new(),
        }
    }
}

impl Filters {
    /// The query string for `GET /api/trips`, leading `?` included. Blank
    /// fields are left out entirely rather than sent empty, so the URL stays
    /// readable and `use_resource` re-runs only when something real changed.
    pub fn to_query(&self) -> String {
        let mut params = vec![("kind", self.kind.as_str().to_string())];
        if let Some(activity) = self.activity {
            params.push(("activity", activity.as_str().to_string()));
        }
        for (name, value) in [
            ("q", &self.q),
            ("from", &self.from),
            ("to", &self.to),
            ("min_dist", &self.min_dist),
            ("max_dist", &self.max_dist),
        ] {
            let value = value.trim();
            if !value.is_empty() {
                params.push((name, value.to_string()));
            }
        }
        let query: Vec<String> = params
            .iter()
            .map(|(name, value)| format!("{name}={}", encode(value)))
            .collect();
        format!("?{}", query.join("&"))
    }

    /// Whether anything beyond the tab itself is narrowing the list — the
    /// list uses it to tell "nothing imported yet" from "nothing matches".
    pub fn any_set(&self) -> bool {
        self.activity.is_some()
            || [
                &self.q,
                &self.from,
                &self.to,
                &self.min_dist,
                &self.max_dist,
            ]
            .iter()
            .any(|value| !value.trim().is_empty())
    }
}

/// Percent-encode a query-parameter value. Only the characters that would
/// otherwise break the query string are escaped — a free-text search is the
/// one field an owner can type anything into.
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => "+".to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_untouched_filter_asks_only_for_its_tab() {
        assert_eq!(Filters::default().to_query(), "?kind=recorded");
        assert!(!Filters::default().any_set());
    }

    #[test]
    fn set_fields_become_the_servers_own_parameter_names() {
        let filters = Filters {
            kind: TripKind::Planned,
            q: "oslo".to_string(),
            activity: Some(ActivityType::Hiking),
            from: "2026-01-01".to_string(),
            to: "2026-12-31".to_string(),
            min_dist: "5".to_string(),
            max_dist: "40".to_string(),
        };

        assert_eq!(
            filters.to_query(),
            "?kind=planned&activity=hiking&q=oslo&from=2026-01-01&to=2026-12-31&min_dist=5&max_dist=40"
        );
        assert!(filters.any_set());
    }

    #[test]
    fn blank_and_whitespace_only_fields_are_left_out() {
        let filters = Filters {
            q: "   ".to_string(),
            ..Default::default()
        };

        assert_eq!(filters.to_query(), "?kind=recorded");
        assert!(!filters.any_set());
    }

    #[test]
    fn a_free_text_search_is_encoded_so_it_cannot_break_the_query_string() {
        let filters = Filters {
            q: "oslo & bergen?".to_string(),
            ..Default::default()
        };

        assert_eq!(filters.to_query(), "?kind=recorded&q=oslo+%26+bergen%3F");
    }
}
