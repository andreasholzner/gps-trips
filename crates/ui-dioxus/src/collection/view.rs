//! What a collection page's track and waypoint lists are narrowed to — with
//! or without a recorded trip (US-99) — and its place in the URL, so it can
//! be bookmarked and survives a reload (US-52). Pure and Dioxus-free, like
//! `filters.rs`.

use trip_archive_types::RecordedTripFilter;

/// The page's own query: nothing narrows its lists by default.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CollectionView {
    pub recorded_trip: Option<RecordedTripFilter>,
}

impl CollectionView {
    /// The query string, without a leading `?`.
    pub fn to_query(self) -> String {
        self.recorded_trip
            .map(|filter| format!("recorded_trip={}", filter.as_str()))
            .unwrap_or_default()
    }

    /// The inverse of [`Self::to_query`]; anything else, or a value it does
    /// not know, narrows nothing.
    pub fn from_query(query: &str) -> Self {
        let recorded_trip = query
            .split('&')
            .filter_map(|part| part.split_once('='))
            .find(|(name, _)| *name == "recorded_trip")
            .and_then(|(_, value)| value.parse().ok());
        Self { recorded_trip }
    }

    /// Whether something with `has_recorded_trip` stays listed.
    pub fn keeps(self, has_recorded_trip: bool) -> bool {
        match self.recorded_trip {
            None => true,
            Some(RecordedTripFilter::With) => has_recorded_trip,
            Some(RecordedTripFilter::Without) => !has_recorded_trip,
        }
    }
}

/// The router's half of the URL. Nothing in it needs escaping.
impl std::fmt::Display for CollectionView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_query())
    }
}

impl From<&str> for CollectionView {
    fn from(query: &str) -> Self {
        Self::from_query(query)
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us99_the_filter_round_trips_through_the_query() {
        for recorded_trip in [
            None,
            Some(RecordedTripFilter::With),
            Some(RecordedTripFilter::Without),
        ] {
            let view = CollectionView { recorded_trip };
            assert_eq!(CollectionView::from_query(&view.to_query()), view);
        }
        assert_eq!(CollectionView::default().to_query(), "");
        assert_eq!(
            CollectionView::from_query("recorded_trip=maybe"),
            CollectionView::default()
        );
    }

    #[test]
    fn us99_the_filter_keeps_what_it_asks_for() {
        let with = CollectionView {
            recorded_trip: Some(RecordedTripFilter::With),
        };
        let without = CollectionView {
            recorded_trip: Some(RecordedTripFilter::Without),
        };

        assert!(CollectionView::default().keeps(true));
        assert!(CollectionView::default().keeps(false));
        assert!(with.keeps(true) && !with.keeps(false));
        assert!(!without.keeps(true) && without.keeps(false));
    }
}
