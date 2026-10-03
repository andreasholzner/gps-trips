//! The tag summary's figures (US-78), added up from the archive's
//! [`TagSummaries`]. Pure and Dioxus-free, so every number the screen shows
//! is unit-tested on the host (ADR-0012); the components only lay them out.
//!
//! Days out and the longest day are US-77's, and are worked out by its
//! [`figures`](crate::stats::figures).

use std::collections::BTreeSet;

use time::Date;
use trip_archive_types::{ActivityType, TagSummaries};

use crate::stats::activity_order;
use crate::stats::figures::{self as stats, DayRecord};

/// One chosen tag's summary.
#[derive(Clone, Debug, PartialEq)]
pub struct TagSummary {
    pub name: String,
    /// Its recorded trips without dates, counted nowhere.
    pub undated: u32,
    /// `None` when the tag holds no dated recorded trip.
    pub figures: Option<TagFigures>,
}

/// What a tag's trips add up to.
#[derive(Clone, Debug, PartialEq)]
pub struct TagFigures {
    /// The local start date of its first trip and the local end date of its
    /// last, `YYYY-MM-DD`.
    pub first: String,
    pub last: String,
    /// Per activity present, in the import form's order.
    pub activities: Vec<(ActivityType, Sums)>,
    /// Every activity together.
    pub together: Sums,
    pub longest_day: Option<DayRecord>,
}

impl TagFigures {
    /// `activity`'s figures, if the tag holds any of it.
    pub fn of(&self, activity: ActivityType) -> Option<&Sums> {
        self.activities
            .iter()
            .find(|(candidate, _)| *candidate == activity)
            .map(|(_, sums)| sums)
    }
}

/// The figures of some trips, in the units the screen shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sums {
    pub trips: u32,
    /// The distinct local dates the trips cover.
    pub days_out: u32,
    pub km: f64,
    pub ascent_m: f64,
    pub descent_m: f64,
    pub moving_hours: f64,
}

impl Sums {
    fn of(trips: &[stats::Dated]) -> Self {
        let days: BTreeSet<Date> = trips.iter().flat_map(stats::Dated::days).collect();
        Self {
            trips: trips.len() as u32,
            days_out: days.len() as u32,
            km: trips.iter().map(|t| t.trip.distance_m).sum::<f64>() / 1000.0,
            ascent_m: trips.iter().filter_map(|t| t.trip.ascent_m).sum(),
            descent_m: trips.iter().filter_map(|t| t.trip.descent_m).sum(),
            moving_hours: trips.iter().filter_map(|t| t.trip.moving_secs).sum::<i64>() as f64
                / 3600.0,
        }
    }
}

/// A summary per chosen tag, in the order chosen.
pub fn summaries(data: &TagSummaries) -> Vec<TagSummary> {
    let dated = stats::dated(&data.trips, &[]);
    data.tags
        .iter()
        .map(|tag| {
            let trips: Vec<stats::Dated> = dated
                .iter()
                .copied()
                .filter(|trip| tag.trip_ids.contains(&trip.trip.id))
                .collect();
            TagSummary {
                name: tag.name.clone(),
                undated: tag.undated,
                figures: figures(&trips),
            }
        })
        .collect()
}

/// What `trips` add up to; `None` for no trips.
fn figures(trips: &[stats::Dated]) -> Option<TagFigures> {
    let first = trips.iter().map(|trip| trip.start).min()?;
    let last = trips.iter().map(|trip| trip.end).max()?;
    let activities = activity_order()
        .filter_map(|activity| {
            let of: Vec<stats::Dated> = trips
                .iter()
                .copied()
                .filter(|trip| trip.trip.activity_type == activity)
                .collect();
            (!of.is_empty()).then(|| (activity, Sums::of(&of)))
        })
        .collect();
    let all: Vec<&stats::Dated> = trips.iter().collect();
    Some(TagFigures {
        first: first.to_string(),
        last: last.to_string(),
        activities,
        together: Sums::of(trips),
        longest_day: stats::longest_days(&all).into_iter().next(),
    })
}

/// Every activity any of the tags holds, in the import form's order — the
/// table's groups of rows.
pub fn activities(summaries: &[TagSummary]) -> Vec<ActivityType> {
    activity_order()
        .filter(|activity| {
            summaries
                .iter()
                .filter_map(|tag| tag.figures.as_ref())
                .any(|figures| figures.of(*activity).is_some())
        })
        .collect()
}

#[cfg(test)]
mod tests;
