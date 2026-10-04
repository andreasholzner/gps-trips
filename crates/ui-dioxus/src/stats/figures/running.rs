//! The running total (US-77): each year's measure added up day by day — or,
//! for a ratio (US-80, US-81), each year's average so far. Split from
//! `figures` to keep that file under the repo's line cap.

use std::collections::{BTreeMap, BTreeSet};

use time::{Date, Month};

use super::{ratio, Dated, MONTHS};
use crate::stats::view::Measure;

/// Each year's measure added up day by day, for comparing a year with the
/// others at the same date.
#[derive(Clone, Debug, PartialEq)]
pub struct Running {
    /// Oldest first, each with a value per day of the year (1 January at
    /// index 0, 366 in all). The current year's stops at today.
    pub years: Vec<(i32, Vec<Option<f64>>)>,
    /// The chosen year, or else the current one.
    pub highlighted: i32,
    /// The current year so far, and the year before by the same date —
    /// `None` for a ratio with nothing yet to divide by.
    pub this_year: Option<f64>,
    pub last_year: Option<f64>,
}

/// The running totals for `measure` from trips already narrowed to the
/// chosen activity, as of `today`. A ratio (US-80) runs as its two sides,
/// each added up day by day, and reads as the one over the other: the
/// average so far, with nothing to show before the year's first trip that
/// gives it a base.
pub fn running(trips: &[Dated], measure: Measure, year: Option<i32>, today: Date) -> Running {
    // Per year and day of the year: the amount, and the base it is divided by.
    let mut per_day: BTreeMap<i32, [(f64, f64); 366]> = BTreeMap::new();
    fn slot(per_day: &mut BTreeMap<i32, [(f64, f64); 366]>, day: Date) -> &mut (f64, f64) {
        &mut per_day.entry(day.year()).or_insert([(0.0, 0.0); 366])[day.ordinal() as usize - 1]
    }
    if measure == Measure::DaysOut {
        let days: BTreeSet<Date> = trips.iter().flat_map(Dated::days).collect();
        for day in days {
            slot(&mut per_day, day).0 += 1.0;
        }
    } else {
        for trip in trips {
            let (amount, base) = trip.parts(measure);
            let day = slot(&mut per_day, trip.start);
            day.0 += amount;
            day.1 += base;
        }
    }

    let value = |(amount, base): (f64, f64)| -> Option<f64> {
        if measure.is_ratio() {
            ratio(amount, base)
        } else {
            Some(amount)
        }
    };
    let add = |sum: (f64, f64), day: &(f64, f64)| (sum.0 + day.0, sum.1 + day.1);
    let up_to = |year: i32, day: Date| -> Option<f64> {
        let days = per_day.get(&year);
        let sum = days.map_or((0.0, 0.0), |days| {
            days[..day.ordinal() as usize].iter().fold((0.0, 0.0), add)
        });
        value(sum)
    };
    let same_date_last_year =
        Date::from_calendar_date(today.year() - 1, today.month(), today.day())
            // 29 February, a year later, is 28 February.
            .or_else(|_| Date::from_calendar_date(today.year() - 1, Month::February, 28))
            .unwrap_or(today);

    Running {
        years: per_day
            .iter()
            .map(|(year, days)| {
                let values = days
                    .iter()
                    .scan((0.0, 0.0), |sum, day| {
                        *sum = add(*sum, day);
                        Some(*sum)
                    })
                    .enumerate()
                    .map(|(index, sum)| {
                        let after_today =
                            *year == today.year() && index >= today.ordinal() as usize;
                        if after_today {
                            None
                        } else {
                            value(sum)
                        }
                    })
                    .collect();
                (*year, values)
            })
            .collect(),
        highlighted: year.unwrap_or(today.year()),
        this_year: up_to(today.year(), today),
        last_year: up_to(today.year() - 1, same_date_last_year),
    }
}

/// The day-of-year index each month starts at, for the running chart's ticks.
pub fn month_starts() -> Vec<usize> {
    // A leap year's: the chart has a slot for 29 February.
    let mut start = 0;
    (1..=12u8)
        .map(|month| {
            let this = start;
            let month = Month::try_from(month).expect("1 to 12 is a month");
            start += month.length(2024) as usize;
            this
        })
        .collect()
}

/// The name of each day of the year the running chart has a slot for —
/// `1 Jan` to `31 Dec`, with `29 Feb` — for its cursor's readout.
pub fn day_labels() -> Vec<String> {
    (1..=12u8)
        .flat_map(|month| {
            let length = Month::try_from(month)
                .expect("1 to 12 is a month")
                .length(2024);
            (1..=length).map(move |day| format!("{day} {}", MONTHS[month as usize - 1]))
        })
        .collect()
}
