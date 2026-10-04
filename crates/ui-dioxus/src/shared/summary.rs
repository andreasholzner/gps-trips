//! A shared tag summary (US-82), as its recipient sees it: the owner's
//! Summary screen for the shared tags — figures, map and trips, with the
//! tags' names and colors — without choosing or removing a tag, and with
//! every trip opening as a shared one.

use dioxus::prelude::*;
use trip_archive_types::{ShareOverview, SharedSummary, StatsTrip, TagSummaries, TagTrips};

use super::title;
use crate::summary::{ChosenTags, SummaryBody, SummaryView, Viewer};

/// The summary the share `overview` carries, under the share's title.
#[component]
pub fn SharedSummaryView(token: String, overview: ShareOverview) -> Element {
    let Some(shared) = &overview.summary else {
        return rsx! {};
    };
    let heading = title(&overview);
    let view = SummaryView {
        tags: shared.tags.iter().map(|tag| tag.name.clone()).collect(),
    };
    rsx! {
        h1 { id: "share-title", "{heading}" }
        ChosenTags { view, removable: false }
        SummaryBody { summary: as_tag_summaries(shared), viewer: Viewer::Share(token) }
    }
}

/// The recipient's summary as the Summary screen's figures take it. Built
/// here, on the recipient's side, as `as_trip_detail` builds a trip.
pub fn as_tag_summaries(shared: &SharedSummary) -> TagSummaries {
    TagSummaries {
        tags: shared
            .tags
            .iter()
            .map(|tag| TagTrips {
                name: tag.name.clone(),
                trip_ids: tag.trip_ids.clone(),
                undated: tag.undated,
            })
            .collect(),
        trips: shared
            .trips
            .iter()
            .map(|trip| StatsTrip {
                id: trip.id,
                name: trip.name.clone(),
                activity_type: trip.activity_type,
                start_date: trip.start_date.clone(),
                end_date: trip.end_date.clone(),
                distance_m: trip.distance_m,
                ascent_m: trip.ascent_m,
                descent_m: trip.descent_m,
                moving_secs: trip.moving_secs,
                moving_distance_m: trip.moving_distance_m,
                climb_gain_m: trip.climb_gain_m,
                climb_secs: trip.climb_secs,
            })
            .collect(),
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
