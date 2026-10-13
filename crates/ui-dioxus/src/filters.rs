//! The trip-list filter state (US-13/US-32) and its translation into the
//! query string `GET /api/trips` already accepts (ADR-0008/0011) — the same
//! parameter names the server-rendered list submits.
//!
//! Pure and Dioxus-free so it is unit-testable on the host; the components
//! only hold it in a signal and hand it to `api::list_trips` and, with
//! collections on (US-87, US-90), to `api::list_collections`.

use trip_archive_types::ActivityType;

use crate::kinds::Kinds;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filters {
    /// What is listed (US-90): recorded trips, planned trips and
    /// collections, in any combination but none. The name search, the
    /// activity and the region narrow every kind; the rest are about trips,
    /// and with no trips on they wait, unchanged, for a kind of trip to be
    /// switched back on.
    pub kinds: Kinds,
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
    /// Only trips without a proper name (US-66) — what counts as one is the
    /// server's to decide.
    pub unnamed: bool,
    /// Only trips with photos left unplaced (US-66).
    pub unplaced: bool,
}

impl Filters {
    /// The SPA's own URL query (US-52), without a leading `?`, so a filtered
    /// list is bookmarkable and survives a reload — see [`Filters`]'s
    /// `Display`. Blank fields are left out entirely rather than sent empty,
    /// so the URL stays readable and `use_resource` re-runs only when
    /// something real changed. The trips are asked for with the same
    /// parameter names, but their own `kind` ([`Self::trip_query`]).
    pub fn to_query(&self) -> String {
        self.query(Some(self.kinds.param()))
    }

    /// The query string with `kind` as given, or without one.
    fn query(&self, kind: Option<String>) -> String {
        let mut params: Vec<(&str, String)> = kind.map(|kind| ("kind", kind)).into_iter().collect();
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
        for (name, on) in [("unnamed", self.unnamed), ("unplaced", self.unplaced)] {
            if on {
                params.push((name, "true".to_string()));
            }
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
                "kind" => filters.kinds = Kinds::from_param(&value),
                "activity" => filters.activity = value.parse().ok(),
                "q" => filters.q = value,
                "from" => filters.from = value,
                "to" => filters.to = value,
                "min_dist" => filters.min_dist = value,
                "max_dist" => filters.max_dist = value,
                "bbox" => filters.bbox = value,
                "unnamed" => filters.unnamed = value == "true",
                "unplaced" => filters.unplaced = value == "true",
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

    /// The query string for `GET /api/trips`, or `None` with no kind of trip
    /// on. The archive takes one kind or none, which is both (US-90).
    pub fn trip_query(&self) -> Option<String> {
        self.kinds
            .has_trips()
            .then(|| self.query(self.kinds.trip_kind().map(|kind| kind.as_str().to_string())))
    }

    /// The query string for `GET /api/collections` (US-87): the name search,
    /// the region and the activity (US-90), the filters that are not about
    /// trips alone.
    pub fn collection_query(&self) -> String {
        let activity = self.activity.map(|activity| activity.as_str().to_string());
        [
            ("q", self.q.trim()),
            ("bbox", self.bbox.trim()),
            ("activity", activity.as_deref().unwrap_or_default()),
        ]
        .into_iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(name, value)| format!("{name}={}", encode(value)))
        .collect::<Vec<_>>()
        .join("&")
    }

    /// What "Clear filters" leaves: the kinds, and with no kind of trip on
    /// the trip filters too, which are hidden then (US-87) — only what the
    /// owner can see is cleared.
    pub fn cleared(&self) -> Filters {
        if self.kinds.has_trips() {
            Filters {
                kinds: self.kinds,
                ..Default::default()
            }
        } else {
            Filters {
                q: String::new(),
                bbox: String::new(),
                activity: None,
                ..self.clone()
            }
        }
    }

    /// Whether anything beyond the kinds is narrowing the list — the list
    /// uses it to tell "nothing imported yet" from "nothing matches". With
    /// no kind of trip on, only the filters that apply count.
    pub fn any_set(&self) -> bool {
        let shared = self.activity.is_some()
            // A chosen region narrows the list, so an empty result is
            // "nothing matches" rather than "nothing imported yet" (US-14) —
            // the spike got this wrong by keeping bbox outside Filters, and
            // the wrong empty state is what gave it away.
            || [&self.q, &self.bbox]
                .iter()
                .any(|value| !value.trim().is_empty());
        if !self.kinds.has_trips() {
            return shared;
        }
        shared
            || !self.tags.is_empty()
            || self.unnamed
            || self.unplaced
            || [&self.from, &self.to, &self.min_dist, &self.max_dist]
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
pub(crate) fn encode(value: &str) -> String {
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
pub(crate) fn decode(value: &str) -> String {
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

// Split into filters/tests.rs to keep this file under the repo's 500-line cap.
#[cfg(test)]
mod tests;
