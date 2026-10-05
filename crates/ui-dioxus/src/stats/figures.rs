//! The statistics screen's figures (US-77), added up from the archive's
//! [`StatsTrip`] rows. Pure and Dioxus-free, so every number the screen
//! shows is unit-tested on the host (ADR-0012); the components only lay
//! them out and the charts only draw them (ADR-0025).
//!
//! The rules, from the story: a trip counts in the year and month of its
//! local start date; days out are the distinct local dates the trips cover,
//! from start to end, wherever those dates fall.

use std::collections::{BTreeMap, BTreeSet};

use time::Date;
use trip_archive_types::{ActivityType, StatsTrip};

use super::view::{Measure, StatsView};

/// The longest span one trip's days out are counted over. A track whose
/// clock jumped years ahead would otherwise fill every column with days it
/// never saw.
const MAX_TRIP_DAYS: i64 = 366;

/// The months' short names, the one-year view's columns.
pub const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A trip with its dates read, the shape everything below works on.
#[derive(Clone, Copy)]
pub struct Dated<'a> {
    pub trip: &'a StatsTrip,
    pub start: Date,
    pub end: Date,
}

impl Dated<'_> {
    /// Every local date the trip covers.
    pub fn days(&self) -> impl Iterator<Item = Date> {
        let last = self
            .end
            .min(self.start + time::Duration::days(MAX_TRIP_DAYS - 1));
        std::iter::successors(Some(self.start), move |day| day.next_day())
            .take_while(move |day| *day <= last)
    }

    /// The trip's part of a measure, in the measure's unit: what it adds,
    /// and — for a ratio (US-80) — what it adds to the total that is divided
    /// by. A trip missing either side of a ratio adds to neither.
    fn parts(&self, measure: Measure) -> (f64, f64) {
        let trip = self.trip;
        match measure {
            Measure::Distance => (trip.distance_m / 1000.0, 0.0),
            Measure::Ascent => (trip.ascent_m.unwrap_or(0.0), 0.0),
            Measure::MovingTime => (trip.moving_secs.unwrap_or(0) as f64 / 3600.0, 0.0),
            Measure::AverageSpeed => match (trip.moving_distance_m, trip.moving_secs) {
                (Some(metres), Some(secs)) => (metres / 1000.0, secs as f64 / 3600.0),
                _ => (0.0, 0.0),
            },
            Measure::ClimbingRate => match (trip.climb_gain_m, trip.climb_secs) {
                (Some(gain_m), Some(secs)) => (gain_m, secs as f64 / 3600.0),
                _ => (0.0, 0.0),
            },
            Measure::Trips => (1.0, 0.0),
            // Not additive: see `Bucketing::add`.
            Measure::DaysOut => (0.0, 0.0),
        }
    }
}

/// `YYYY-MM-DD` as a date.
pub fn parse_date(date: &str) -> Option<Date> {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    Date::parse(date, format).ok()
}

/// The trips whose dates can be read, narrowed to `activities` unless none
/// are chosen. A trip that ends before it starts is taken to end the day it
/// started.
pub fn dated<'a>(trips: &'a [StatsTrip], activities: &[ActivityType]) -> Vec<Dated<'a>> {
    trips
        .iter()
        .filter(|trip| activities.is_empty() || activities.contains(&trip.activity_type))
        .filter_map(|trip| {
            let start = parse_date(&trip.start_date)?;
            let end = parse_date(&trip.end_date).unwrap_or(start).max(start);
            Some(Dated { trip, start, end })
        })
        .collect()
}

/// The years the period control offers: every year a trip starts in, latest
/// first.
pub fn years(trips: &[Dated]) -> Vec<i32> {
    let years: BTreeSet<i32> = trips.iter().map(|trip| trip.start.year()).collect();
    years.into_iter().rev().collect()
}

/// The activities present, in the order the import form lists them, with
/// the unspecified last.
pub fn activities(trips: &[Dated]) -> Vec<ActivityType> {
    super::view::activity_order()
        .filter(|activity| {
            trips
                .iter()
                .any(|trip| trip.trip.activity_type == *activity)
        })
        .collect()
}

// ── Totals ───────────────────────────────────────────────────────────────────

/// The totals table: one column per year (all years) or month (one year).
#[derive(Clone, Debug, PartialEq)]
pub struct Totals {
    pub columns: Vec<String>,
    /// One per activity, or the single chosen activity's alone.
    pub rows: Vec<TotalsRow>,
    /// The activities shown together, under the per-activity rows; `None`
    /// when one activity is chosen, since it would repeat that activity's
    /// row.
    pub sum: Option<TotalsRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TotalsRow {
    /// `None` on the sum row.
    pub activity: Option<ActivityType>,
    pub values: Vec<f64>,
    pub total: f64,
    /// The row's part of the sum row's total, as a fraction — only with all
    /// activities, where there is something to have a share of.
    pub share: Option<f64>,
}

/// Where a date falls among the columns.
struct Bucketing {
    year: Option<i32>,
    first_year: i32,
    columns: usize,
}

impl Bucketing {
    fn new(trips: &[Dated], year: Option<i32>) -> Self {
        match year {
            Some(year) => Self {
                year: Some(year),
                first_year: year,
                columns: 12,
            },
            None => {
                let first = trips.iter().map(|trip| trip.start.year()).min();
                let last = trips.iter().map(|trip| trip.end.year()).max();
                let (first, last) = first.zip(last).unwrap_or((0, -1));
                Self {
                    year: None,
                    first_year: first,
                    columns: (last - first + 1).max(0) as usize,
                }
            }
        }
    }

    fn labels(&self) -> Vec<String> {
        match self.year {
            Some(_) => MONTHS.iter().map(|month| month.to_string()).collect(),
            None => (0..self.columns)
                .map(|offset| (self.first_year + offset as i32).to_string())
                .collect(),
        }
    }

    fn column(&self, date: Date) -> Option<usize> {
        match self.year {
            Some(year) => (date.year() == year).then(|| date.month() as usize - 1),
            None => usize::try_from(date.year() - self.first_year)
                .ok()
                .filter(|column| *column < self.columns),
        }
    }

    /// `trips`' measure per column, and over the whole period. A ratio's
    /// two sides are added up apart and divided last, so it is never an
    /// average of averages; a column with nothing to divide by reads as 0,
    /// which the tables show as empty.
    fn add(&self, trips: &[Dated], measure: Measure) -> (Vec<f64>, f64) {
        let mut amounts = vec![0.0; self.columns];
        let mut bases = vec![0.0; self.columns];
        if measure == Measure::DaysOut {
            let days: BTreeSet<Date> = trips.iter().flat_map(Dated::days).collect();
            for column in days.iter().filter_map(|day| self.column(*day)) {
                amounts[column] += 1.0;
            }
        } else {
            for trip in trips {
                if let Some(column) = self.column(trip.start) {
                    let (amount, base) = trip.parts(measure);
                    amounts[column] += amount;
                    bases[column] += base;
                }
            }
        }
        if !measure.is_ratio() {
            let total = amounts.iter().sum();
            return (amounts, total);
        }
        let total = ratio(amounts.iter().sum(), bases.iter().sum()).unwrap_or(0.0);
        let values = amounts
            .iter()
            .zip(&bases)
            .map(|(amount, base)| ratio(*amount, *base).unwrap_or(0.0))
            .collect();
        (values, total)
    }
}

/// `amount` over `base`, when there is a base to divide by.
fn ratio(amount: f64, base: f64) -> Option<f64> {
    (base > 0.0).then(|| amount / base)
}

/// The totals table for `view`, from trips already narrowed to its activities.
pub fn totals(trips: &[Dated], view: &StatsView) -> Totals {
    let bucketing = Bucketing::new(trips, view.year);
    let in_period: Vec<Dated> = trips
        .iter()
        .copied()
        .filter(|trip| view.year.is_none_or(|year| trip.start.year() == year))
        .collect();
    let row = |activity: ActivityType| {
        let of_activity: Vec<Dated> = trips
            .iter()
            .copied()
            .filter(|trip| trip.trip.activity_type == activity)
            .collect();
        let (values, total) = bucketing.add(&of_activity, view.measure);
        TotalsRow {
            activity: Some(activity),
            values,
            total,
            share: None,
        }
    };

    if let Some(activity) = view.single() {
        return Totals {
            columns: bucketing.labels(),
            rows: vec![row(activity)],
            sum: None,
        };
    }
    let (values, total) = bucketing.add(trips, view.measure);
    let mut rows: Vec<TotalsRow> = activities(&in_period).into_iter().map(row).collect();
    // A ratio is no share of anything (US-80).
    if !view.measure.is_ratio() {
        for row in &mut rows {
            row.share = (total > 0.0).then(|| row.total / total);
        }
    }
    Totals {
        columns: bucketing.labels(),
        rows,
        sum: Some(TotalsRow {
            activity: None,
            values,
            total,
            share: None,
        }),
    }
}

// ── Records ──────────────────────────────────────────────────────────────────

/// How many places each record lists.
pub const TOP: usize = 3;

/// The records of the period: overall and per activity, or the single
/// chosen activity's alone. Each lists its best [`TOP`], best first; fewer
/// when there are fewer trips.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordRow {
    /// `None` on the overall row.
    pub activity: Option<ActivityType>,
    /// The longest trips, in kilometres.
    pub longest: Vec<TripRecord>,
    /// The trips with the most ascent, in metres.
    pub most_ascent: Vec<TripRecord>,
    /// The dates with the most distance started on them, in kilometres.
    pub longest_day: Vec<DayRecord>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TripRecord {
    pub id: i64,
    pub name: String,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DayRecord {
    pub date: String,
    pub km: f64,
    /// The trips started on that date, as `(id, name)`.
    pub trips: Vec<(i64, String)>,
}

/// The records for `view`, from trips already narrowed to its activities.
pub fn records(trips: &[Dated], view: &StatsView) -> Vec<RecordRow> {
    let in_period: Vec<Dated> = trips
        .iter()
        .copied()
        .filter(|trip| view.year.is_none_or(|year| trip.start.year() == year))
        .collect();
    let row = |activity: Option<ActivityType>| {
        let of: Vec<&Dated> = in_period
            .iter()
            .filter(|trip| activity.is_none_or(|activity| trip.trip.activity_type == activity))
            .collect();
        RecordRow {
            activity,
            longest: best(&of, |trip| Some(trip.distance_m / 1000.0)),
            most_ascent: best(&of, |trip| trip.ascent_m),
            longest_day: longest_days(&of),
        }
    };
    match view.single() {
        Some(activity) => vec![row(Some(activity))],
        None => std::iter::once(None)
            .chain(activities(&in_period).into_iter().map(Some))
            .map(row)
            .collect(),
    }
}

/// The [`TOP`] items by `value`, highest first. The sort is stable, so of
/// equals the earlier comes first — the trips arrive oldest first.
fn top<T>(mut items: Vec<(T, f64)>) -> Vec<(T, f64)> {
    items.sort_by(|a, b| b.1.total_cmp(&a.1));
    items.truncate(TOP);
    items
}

/// The trips with the highest `value`.
fn best(trips: &[&Dated], value: impl Fn(&StatsTrip) -> Option<f64>) -> Vec<TripRecord> {
    let valued = trips
        .iter()
        .filter_map(|trip| Some((trip.trip, value(trip.trip)?)))
        .collect();
    top(valued)
        .into_iter()
        .map(|(trip, value)| TripRecord {
            id: trip.id,
            name: trip.name.clone(),
            value,
        })
        .collect()
}

/// The dates with the most distance started on them.
pub fn longest_days(trips: &[&Dated]) -> Vec<DayRecord> {
    let mut days: BTreeMap<Date, Vec<&StatsTrip>> = BTreeMap::new();
    for trip in trips {
        days.entry(trip.start).or_default().push(trip.trip);
    }
    let valued = days
        .into_iter()
        .map(|(date, trips)| {
            let metres = trips.iter().map(|trip| trip.distance_m).sum::<f64>();
            ((date, trips), metres)
        })
        .collect();
    top(valued)
        .into_iter()
        .map(|((date, trips), metres)| DayRecord {
            date: date.to_string(),
            km: metres / 1000.0,
            trips: trips
                .iter()
                .map(|trip| (trip.id, trip.name.clone()))
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests;
