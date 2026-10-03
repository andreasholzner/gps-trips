//! Which tags the Summary screen is showing (US-78), and their place in the
//! URL, so a summary can be bookmarked the way a narrowed trip list can
//! (US-52). Pure and Dioxus-free, like `filters.rs`.

use trip_archive_types::normalize_tag_name;

use crate::filters::{decode, encode};

/// The chosen tags, in the order they were chosen — which decides their
/// columns and their colors.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SummaryView {
    pub tags: Vec<String>,
}

impl SummaryView {
    /// The view of one tag — where a tag on a trip's page leads.
    pub fn of(tag: &str) -> Self {
        Self::default().with(tag)
    }

    /// The view with `tag` added at the end, normalized as the archive
    /// stores it (US-33). A name the archive would refuse, or one already
    /// chosen, changes nothing.
    pub fn with(&self, tag: &str) -> Self {
        let mut tags = self.tags.clone();
        if let Ok(name) = normalize_tag_name(tag) {
            if !tags.contains(&name) {
                tags.push(name);
            }
        }
        Self { tags }
    }

    /// The view without `tag`.
    pub fn without(&self, tag: &str) -> Self {
        Self {
            tags: self.tags.iter().filter(|t| *t != tag).cloned().collect(),
        }
    }

    /// The query string, without a leading `?`: one comma-separated `tags`
    /// parameter, as the trip list's (US-38), or nothing.
    pub fn to_query(&self) -> String {
        if self.tags.is_empty() {
            String::new()
        } else {
            format!("tags={}", encode(&self.tags.join(",")))
        }
    }

    /// The inverse of [`Self::to_query`]. A name that is not a tag is
    /// dropped, so a hand-edited URL still opens the screen.
    pub fn from_query(query: &str) -> Self {
        query
            .split('&')
            .filter_map(|part| part.split_once('='))
            .filter(|(name, _)| *name == "tags")
            .flat_map(|(_, value)| {
                decode(value)
                    .split(',')
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .fold(Self::default(), |view, tag| view.with(&tag))
    }
}

/// The router's half of the URL (US-52's mechanism).
impl std::fmt::Display for SummaryView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_query())
    }
}

impl From<&str> for SummaryView {
    fn from(query: &str) -> Self {
        Self::from_query(query)
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn view(tags: &[&str]) -> SummaryView {
        SummaryView {
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
        }
    }

    #[test]
    fn us78_the_chosen_tags_round_trip_through_the_query_in_their_order() {
        let chosen = view(&["norway", "alps&co"]);

        assert_eq!(chosen.to_query(), "tags=norway%2Calps%26co");
        assert_eq!(SummaryView::from_query(&chosen.to_query()), chosen);
    }

    #[test]
    fn us78_no_tags_leave_the_query_empty() {
        assert_eq!(SummaryView::default().to_query(), "");
        assert_eq!(SummaryView::from_query(""), SummaryView::default());
    }

    #[test]
    fn us78_a_tag_is_added_normalized_and_once() {
        let chosen = view(&["alps"])
            .with(" Norway ")
            .with("ALPS")
            .with("day trip");

        assert_eq!(chosen, view(&["alps", "norway"]));
    }

    #[test]
    fn us78_a_chosen_tag_can_be_removed_again() {
        assert_eq!(view(&["alps", "norway"]).without("alps"), view(&["norway"]));
    }

    #[test]
    fn us78_a_hand_edited_query_keeps_only_real_tag_names() {
        let read = SummaryView::from_query("tags=Alps,,alps,day+trip&other=1");

        assert_eq!(read, view(&["alps"]));
    }
}
