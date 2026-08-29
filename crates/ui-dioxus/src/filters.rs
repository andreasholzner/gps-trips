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
    /// Chosen tag names (US-38): only trips carrying *all* of them are
    /// listed. Already-normalized names, straight from the server's own tag
    /// list, so no validation happens here.
    pub tags: Vec<String>,
    /// The region rectangle (US-14/US-52) as the API's own parameter,
    /// `minLon,minLat,maxLon,maxLat`; blank means no region chosen. Kept as
    /// the raw string like every other field here, so it travels in the URL
    /// and to the server unchanged, and the map is the only thing that has
    /// to know it is four numbers.
    pub bbox: String,
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
            tags: Vec::new(),
            bbox: String::new(),
        }
    }
}

impl Filters {
    /// The query string for `GET /api/trips`, without a leading `?`. Blank
    /// fields are left out entirely rather than sent empty, so the URL stays
    /// readable and `use_resource` re-runs only when something real changed.
    ///
    /// This doubles as the SPA's own URL query (US-52), so a filtered list is
    /// bookmarkable and survives a reload — see [`Filters`]'s `Display`.
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
            ("bbox", &self.bbox),
        ] {
            let value = value.trim();
            if !value.is_empty() {
                params.push((name, value.to_string()));
            }
        }
        if !self.tags.is_empty() {
            // One comma-separated parameter (US-38, ADR-0011): unambiguous
            // because a tag name can never contain a comma (US-33).
            params.push(("tags", self.tags.join(",")));
        }
        let query: Vec<String> = params
            .iter()
            .map(|(name, value)| format!("{name}={}", encode(value)))
            .collect();
        query.join("&")
    }

    /// Read filters back out of a query string (no leading `?`), the inverse
    /// of [`Self::to_query`]. Anything unparseable is simply not applied:
    /// a hand-edited or truncated URL yields a working list rather than an
    /// error page, which is also how the server's own parser treats a blank
    /// value.
    pub fn from_query(query: &str) -> Self {
        let mut filters = Filters::default();
        for (name, value) in query
            .split('&')
            .filter(|part| !part.is_empty())
            .filter_map(|part| part.split_once('='))
        {
            let value = decode(value);
            match name {
                "kind" => filters.kind = value.parse().unwrap_or_default(),
                "activity" => filters.activity = value.parse().ok(),
                "q" => filters.q = value,
                "from" => filters.from = value,
                "to" => filters.to = value,
                "min_dist" => filters.min_dist = value,
                "max_dist" => filters.max_dist = value,
                "bbox" => filters.bbox = value,
                "tags" => {
                    filters.tags = value
                        .split(',')
                        .filter(|tag| !tag.is_empty())
                        .map(str::to_string)
                        .collect()
                }
                _ => {}
            }
        }
        filters
    }

    /// Whether anything beyond the tab itself is narrowing the list — the
    /// list uses it to tell "nothing imported yet" from "nothing matches".
    pub fn any_set(&self) -> bool {
        self.activity.is_some()
            || !self.tags.is_empty()
            || [
                &self.q,
                &self.from,
                &self.to,
                &self.min_dist,
                &self.max_dist,
                // A chosen region narrows the list, so an empty result is
                // "nothing matches" rather than "nothing imported yet"
                // (US-14) — the spike got this wrong by keeping bbox outside
                // Filters, and the wrong empty state is what gave it away.
                &self.bbox,
            ]
            .iter()
            .any(|value| !value.trim().is_empty())
    }
}

/// The SPA's own URL query (US-52), so the router can put the filters in the
/// address bar and read them back on the next load.
///
/// **Not** simply [`Self::to_query`]: dioxus-router percent-decodes the whole
/// query string once before handing it to `FromQuery`, which would turn a
/// `%26` inside a value back into a bare `&` and split that value in two.
/// Escaping the escapes here means one decode lands exactly on the form
/// [`Self::from_query`] expects. The round-trip through `Route` is asserted
/// in `main.rs`, because this compensates for someone else's behaviour and
/// reasoning about it is not evidence.
impl std::fmt::Display for Filters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_query().replace('%', "%25"))
    }
}

/// The inverse, for the router's `FromQuery` (which it derives for any
/// `From<&str>`).
impl From<&str> for Filters {
    fn from(query: &str) -> Self {
        Filters::from_query(query)
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

/// Undo [`encode`].
fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            // Byte-wise, never slicing the `&str`: a `%` followed by
            // multi-byte UTF-8 would put a slice boundary mid-character.
            b'%' if i + 2 < bytes.len() => {
                match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                    (Some(hi), Some(lo)) => {
                        out.push(hi << 4 | lo);
                        i += 3;
                    }
                    // Not an escape after all — keep the '%' as typed.
                    _ => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_untouched_filter_asks_only_for_its_tab() {
        assert_eq!(Filters::default().to_query(), "kind=recorded");
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
            tags: Vec::new(),
            bbox: String::new(),
        };

        assert_eq!(
            filters.to_query(),
            "kind=planned&activity=hiking&q=oslo&from=2026-01-01&to=2026-12-31&min_dist=5&max_dist=40"
        );
        assert!(filters.any_set());
    }

    #[test]
    fn blank_and_whitespace_only_fields_are_left_out() {
        let filters = Filters {
            q: "   ".to_string(),
            ..Default::default()
        };

        assert_eq!(filters.to_query(), "kind=recorded");
        assert!(!filters.any_set());
    }

    #[test]
    fn chosen_tags_travel_as_one_comma_separated_parameter() {
        // US-38: comma-joined is unambiguous because a tag name can never
        // contain a comma (US-33).
        let filters = Filters {
            tags: vec!["alpine".to_string(), "summer".to_string()],
            ..Default::default()
        };

        assert_eq!(filters.to_query(), "kind=recorded&tags=alpine%2Csummer");
        assert!(filters.any_set());
    }

    #[test]
    fn every_filter_survives_a_round_trip_through_the_query_string() {
        // The SPA's URL is this same string (US-52), so a filtered list is
        // bookmarkable: what goes into the address bar must come back out.
        let filters = Filters {
            kind: TripKind::Planned,
            q: "oslo".to_string(),
            activity: Some(ActivityType::SkiTouring),
            from: "2026-01-01".to_string(),
            to: "2026-12-31".to_string(),
            min_dist: "5".to_string(),
            max_dist: "40".to_string(),
            tags: vec!["alpine".to_string(), "summer".to_string()],
            bbox: String::new(),
        };

        assert_eq!(Filters::from_query(&filters.to_query()), filters);
    }

    #[test]
    fn a_search_full_of_awkward_characters_round_trips_too() {
        let filters = Filters {
            q: "oslo & bergen? 100% + å".to_string(),
            ..Default::default()
        };

        assert_eq!(Filters::from_query(&filters.to_query()), filters);
    }

    #[test]
    fn a_chosen_region_travels_as_the_apis_bbox_parameter() {
        let filters = Filters {
            bbox: "10.75,59.91,11.25,60.12".to_string(),
            ..Default::default()
        };

        assert_eq!(
            filters.to_query(),
            "kind=recorded&bbox=10.75%2C59.91%2C11.25%2C60.12"
        );
        assert_eq!(Filters::from_query(&filters.to_query()), filters);
    }

    #[test]
    fn a_chosen_region_counts_as_a_filter() {
        // Otherwise an empty result reads "no trips yet" instead of "nothing
        // matches your filters" (US-14).
        let filters = Filters {
            bbox: "10.75,59.91,11.25,60.12".to_string(),
            ..Default::default()
        };

        assert!(filters.any_set());
        assert!(!Filters::default().any_set());
    }

    #[test]
    fn an_empty_query_is_the_default_view() {
        assert_eq!(Filters::from_query(""), Filters::default());
    }

    #[test]
    fn a_hand_edited_query_keeps_what_it_can() {
        // A truncated or mistyped URL should still show a working list, the
        // way the server's own parser treats a blank value.
        let filters = Filters::from_query("kind=planned&activity=nonsense&q=oslo&bogus=1&novalue");

        assert_eq!(filters.kind, TripKind::Planned);
        assert_eq!(filters.activity, None);
        assert_eq!(filters.q, "oslo");
    }

    #[test]
    fn a_free_text_search_is_encoded_so_it_cannot_break_the_query_string() {
        let filters = Filters {
            q: "oslo & bergen?".to_string(),
            ..Default::default()
        };

        assert_eq!(filters.to_query(), "kind=recorded&q=oslo+%26+bergen%3F");
    }
}
