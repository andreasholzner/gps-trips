//! What the Tags screen is narrowed to (US-83), and its place in the URL, so
//! it can be bookmarked and survives a reload the way a narrowed trip list
//! does (US-52). Pure and Dioxus-free, like `filters.rs`.

use crate::filters::{decode, encode};

/// The text the tags' names must contain, as typed — kept verbatim so the
/// field does not change under the owner's fingers; matching trims it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TagsView {
    pub q: String,
}

impl TagsView {
    /// The query string, without a leading `?`: one `q` parameter, or
    /// nothing.
    pub fn to_query(&self) -> String {
        if self.q.is_empty() {
            String::new()
        } else {
            format!("q={}", encode(&self.q))
        }
    }

    /// The inverse of [`Self::to_query`]; any other parameter is ignored.
    pub fn from_query(query: &str) -> Self {
        let q = query
            .split('&')
            .filter_map(|part| part.split_once('='))
            .find(|(name, _)| *name == "q")
            .map(|(_, value)| decode(value))
            .unwrap_or_default();
        Self { q }
    }
}

/// The router's half of the URL (US-52's mechanism). The router
/// percent-decodes the whole query before parsing it, so the escapes are
/// escaped once more, as `Filters` does for its free-text search.
impl std::fmt::Display for TagsView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_query().replace('%', "%25"))
    }
}

impl From<&str> for TagsView {
    fn from(query: &str) -> Self {
        Self::from_query(query)
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn view(q: &str) -> TagsView {
        TagsView { q: q.to_string() }
    }

    #[test]
    fn us83_the_filter_round_trips_through_the_query() {
        let narrowed = view("alps&co =x");

        assert_eq!(TagsView::from_query(&narrowed.to_query()), narrowed);
    }

    #[test]
    fn us83_no_filter_leaves_the_query_empty() {
        assert_eq!(TagsView::default().to_query(), "");
        assert_eq!(TagsView::from_query(""), TagsView::default());
    }

    #[test]
    fn us83_other_parameters_are_ignored() {
        assert_eq!(TagsView::from_query("other=1&q=alp"), view("alp"));
    }
}
