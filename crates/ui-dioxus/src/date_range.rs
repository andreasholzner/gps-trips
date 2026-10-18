//! The trip list's date filter as one slider (US-110): two handles over the
//! months from the oldest trip of the kinds on to today, and above the track
//! a bar per month — every trip of the kinds on, and the part of them the
//! other filters match.
//!
//! What the months hold and where the handles sit are pure, like
//! [`Filters`]; the component only draws them and writes `from`/`to` back.
//! A handle at an end is no limit on that side, so the filter's meaning and
//! its URL (US-13, US-52) are what they were with two date fields.

use dioxus::prelude::*;
use time::{Date, OffsetDateTime};
use trip_archive_types::TripSummary;

use crate::api::{self, ApiClient};
use crate::filters::Filters;
use crate::format;
use crate::kinds::{Kind, Kinds};

/// How many years are marked under the track at most; past that only every
/// other one (or every third…) is, so the labels never run into each other.
const MAX_TICKS: usize = 8;

/// Today, which is where the slider ends. In UTC: the handles move in whole
/// months, which only an hour or two either side of one's end tells apart.
pub fn today() -> Date {
    OffsetDateTime::now_utc().date()
}

/// A calendar month, counted from year 0, so that consecutive months are
/// consecutive numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Month(i32);

impl Month {
    fn of(date: Date) -> Month {
        Month(date.year() * 12 + i32::from(u8::from(date.month())) - 1)
    }

    /// The month a `YYYY-MM-DD` falls in — `None` for anything else, which
    /// is then no limit as far as the handles go.
    fn parse(date: &str) -> Option<Month> {
        let format = time::macros::format_description!("[year]-[month]-[day]");
        Date::parse(date.trim(), &format).ok().map(Month::of)
    }

    fn first_day(self) -> Date {
        let month = u8::try_from(self.0.rem_euclid(12) + 1).expect("1 to 12");
        let month = time::Month::try_from(month).expect("1 to 12");
        Date::from_calendar_date(self.0.div_euclid(12), month, 1).expect("every month has a 1st")
    }

    fn last_day(self) -> Date {
        Month(self.0 + 1)
            .first_day()
            .previous_day()
            .expect("not the first day there is")
    }
}

fn month_of(trip: &TripSummary) -> Option<Month> {
    trip.start_date.as_deref().and_then(Month::parse)
}

/// The months the slider runs over, the oldest first and the current one
/// last; a handle's position is an index into them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    first: Month,
    len: usize,
}

impl Span {
    fn last(self) -> usize {
        self.len - 1
    }

    fn month_at(self, index: usize) -> Month {
        Month(self.first.0 + i32::try_from(index).expect("a few hundred months"))
    }

    /// Where `month` sits, at the nearer end if it lies outside.
    fn index(self, month: Month) -> usize {
        usize::try_from(month.0 - self.first.0).map_or(0, |index| index.min(self.last()))
    }

    /// Where the handles sit for a `from` and a `to`: on the months holding
    /// them, at the ends with none.
    pub fn handles(self, from: &str, to: &str) -> (usize, usize) {
        (
            Month::parse(from).map_or(0, |month| self.index(month)),
            Month::parse(to).map_or(self.last(), |month| self.index(month)),
        )
    }

    /// The `from` the left handle at `index` sets: its month's first day,
    /// none at the left end.
    pub fn date_from(self, index: usize) -> String {
        if index == 0 {
            return String::new();
        }
        self.month_at(index).first_day().to_string()
    }

    /// The `to` the right handle at `index` sets: its month's last day, none
    /// at the right end.
    pub fn date_to(self, index: usize) -> String {
        if index >= self.last() {
            return String::new();
        }
        self.month_at(index).last_day().to_string()
    }

    /// A handle's month in words.
    pub fn label_at(self, index: usize) -> String {
        format::month(self.month_at(index).first_day())
    }

    /// The range in words. A date is named as it is rather than as its
    /// handle sits, so one from a link outside the months still reads true.
    pub fn summary(self, from: &str, to: &str) -> String {
        if from.trim().is_empty() && to.trim().is_empty() {
            return "Any date".to_string();
        }
        let from = format::month(Month::parse(from).unwrap_or(self.first).first_day());
        let to = Month::parse(to).map_or_else(
            || "today".to_string(),
            |month| format::month(month.first_day()),
        );
        format!("{from} – {to}")
    }

    /// The Januaries marked under the track, as how far along it they are
    /// in percent and their year — no more than [`MAX_TICKS`].
    pub fn ticks(self) -> Vec<(f64, i32)> {
        let januaries: Vec<usize> = (0..self.len)
            .filter(|&index| self.month_at(index).0 % 12 == 0)
            .collect();
        let step = januaries.len().div_ceil(MAX_TICKS).max(1);
        januaries
            .into_iter()
            .step_by(step)
            .map(|index| (self.percent(index), self.month_at(index).0 / 12))
            .collect()
    }

    /// How far along the track `index` is, in percent.
    fn percent(self, index: usize) -> f64 {
        if self.last() == 0 {
            return 0.0;
        }
        index as f64 / self.last() as f64 * 100.0
    }
}

/// One month's trips: all of the kinds on, and how many of them the other
/// filters match.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bar {
    pub total: usize,
    pub matching: usize,
}

/// The months the slider runs over and a bar for each.
#[derive(Clone, Debug, PartialEq)]
pub struct Months {
    pub span: Span,
    pub bars: Vec<Bar>,
}

impl Months {
    /// The months from the oldest trip of `kinds` in `all` to today's, each
    /// counting its trips, and how many of them are in `matching` — the
    /// trips every filter but the dates matches. Trips without a date or
    /// dated after today are not counted. With `all` not read yet, or no
    /// trip in it dated, the current month alone.
    pub fn of(
        today: Date,
        kinds: Kinds,
        all: Option<&[TripSummary]>,
        matching: &[TripSummary],
    ) -> Months {
        let current = Month::of(today);
        let dated = |trips: &[TripSummary]| -> Vec<Month> {
            trips
                .iter()
                .filter(|trip| kinds.contains(Kind::of_trip(trip.trip_kind)))
                .filter_map(month_of)
                .filter(|month| *month <= current)
                .collect()
        };
        let all = dated(all.unwrap_or_default());
        let first = all.iter().min().copied().unwrap_or(current);
        let span = Span {
            first,
            len: usize::try_from(current.0 - first.0).expect("not after today") + 1,
        };
        let mut bars = vec![Bar::default(); span.len];
        for month in all {
            bars[span.index(month)].total += 1;
        }
        for month in dated(matching) {
            if month >= first {
                bars[span.index(month)].matching += 1;
            }
        }
        // Two reads, so a trip imported between them could match without
        // being counted; a bar never shows more matching than it holds.
        for bar in &mut bars {
            bar.matching = bar.matching.min(bar.total);
        }
        Months { span, bars }
    }
}

/// The trips every filter but the dates matches, read only while a date is
/// set: without one they are the list itself.
pub fn use_dateless_trips(
    filters: Signal<Filters>,
) -> Resource<Option<Result<Vec<TripSummary>, api::ApiError>>> {
    let archive = use_context::<Signal<ApiClient>>();
    let query = use_memo(move || {
        let filters = filters.read();
        if !has_dates(&filters) {
            return None;
        }
        Filters {
            from: String::new(),
            to: String::new(),
            ..filters.clone()
        }
        .trip_query()
    });
    use_resource(move || async move {
        let query = query()?;
        Some(api::list_trips(&archive(), query).await)
    })
}

/// Whether a date narrows the list.
pub fn has_dates(filters: &Filters) -> bool {
    !filters.from.trim().is_empty() || !filters.to.trim().is_empty()
}

/// Which of the two handles.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Handle {
    From,
    To,
}

/// `part` of the busiest month's trips, as a bar's height out of 100.
fn height(part: usize, peak: usize) -> f64 {
    if peak == 0 {
        return 0.0;
    }
    part as f64 / peak as f64 * 100.0
}

/// How much of its month's width a bar leaves empty, so neighbours stay
/// apart.
const BAR_GAP: f64 = 0.2;

/// The slider. A bar's height is its trips against the busiest month's, its
/// lower part what matches; the bars between the handles are highlighted
/// (`in`). Drawn with SVG attributes rather than `style`s: the page's CSP
/// refuses inline styles.
#[component]
pub fn DateRange(filters: Signal<Filters>, months: Months) -> Element {
    let span = months.span;
    // The handle being moved and where to, until it is let go: only then is
    // the filter set, so a drag across many months narrows the list once.
    let mut moving = use_signal(|| None::<(Handle, usize)>);
    let (from, to) = {
        let filters = filters.read();
        match moving() {
            Some((Handle::From, index)) => (span.date_from(index), filters.to.clone()),
            Some((Handle::To, index)) => (filters.from.clone(), span.date_to(index)),
            None => (filters.from.clone(), filters.to.clone()),
        }
    };
    let (lo, hi) = span.handles(&from, &to);
    let lo = lo.min(hi);
    let summary = span.summary(&from, &to);
    // "today" only where there is no limit: a `to` from a link can end a few
    // days into the current month.
    let hi_text = if to.trim().is_empty() {
        "today".to_string()
    } else {
        span.label_at(hi)
    };
    let last = span.last();
    let peak = months.bars.iter().map(|bar| bar.total).max().unwrap_or(0);
    let (left, right) = (span.percent(lo), span.percent(hi));
    // A month a unit wide, centred on its handle position: the first and
    // last bars reach half a unit past the track's ends, into the room the
    // handles take there.
    let view_box = format!("0 0 {} 100", last.max(1));
    // Two handles on one track, each pushed back if dragged past the other.
    // Pushing back is the filter's: the input itself has to be told, as the
    // value Dioxus renders may not have changed.
    let held = move |id: &str, index: usize| {
        document::eval(&format!("document.getElementById('{id}').value = {index}"));
    };
    rsx! {
        // A fieldset, like the tags': the range in words is part of the
        // legend, so the two handles are announced with it.
        fieldset { class: "date-range",
            legend {
                span { "Dates" }
                // Not announced: each handle says its own month as it moves.
                output { aria_live: "off", "{summary}" }
            }
            svg {
                class: "date-bars",
                view_box: "{view_box}",
                preserve_aspect_ratio: "none",
                "aria-hidden": "true",
                for (index, bar) in months.bars.iter().enumerate() {
                    g {
                        key: "{index}",
                        class: if (lo..=hi).contains(&index) { "bar in" } else { "bar" },
                        rect {
                            class: "total",
                            x: "{index as f64 - 0.5 + BAR_GAP / 2.0}",
                            width: "{1.0 - BAR_GAP}",
                            y: "{100.0 - height(bar.total, peak)}",
                            height: "{height(bar.total, peak)}",
                        }
                        rect {
                            class: "match",
                            x: "{index as f64 - 0.5 + BAR_GAP / 2.0}",
                            width: "{1.0 - BAR_GAP}",
                            y: "{100.0 - height(bar.matching, peak)}",
                            height: "{height(bar.matching, peak)}",
                        }
                    }
                }
            }
            div { class: "date-slider",
                svg { class: "date-track", "aria-hidden": "true",
                    rect { width: "100%", height: "100%" }
                    rect { class: "date-fill", x: "{left}%", width: "{right - left}%", height: "100%" }
                }
                input {
                    id: "date-from",
                    r#type: "range",
                    min: "0",
                    max: "{last}",
                    step: "1",
                    value: "{lo}",
                    aria_label: "Earliest month",
                    aria_valuetext: "{span.label_at(lo)}",
                    // Above the other once past the middle, so two handles
                    // together at the right end can still be pulled apart.
                    class: if lo * 2 > last { "on-top" },
                    oninput: move |event| {
                        let Ok(index) = event.value().parse::<usize>() else { return };
                        if index > hi {
                            held("date-from", hi);
                        }
                        moving.set(Some((Handle::From, index.min(hi))));
                    },
                    onchange: move |_| {
                        if let Some((Handle::From, index)) = moving.take() {
                            let from = span.date_from(index);
                            if filters.read().from != from {
                                filters.write().from = from;
                            }
                        }
                    },
                }
                input {
                    id: "date-to",
                    r#type: "range",
                    min: "0",
                    max: "{last}",
                    step: "1",
                    value: "{hi}",
                    aria_label: "Latest month",
                    aria_valuetext: "{hi_text}",
                    oninput: move |event| {
                        let Ok(index) = event.value().parse::<usize>() else { return };
                        if index < lo {
                            held("date-to", lo);
                        }
                        moving.set(Some((Handle::To, index.max(lo))));
                    },
                    onchange: move |_| {
                        if let Some((Handle::To, index)) = moving.take() {
                            let to = span.date_to(index);
                            if filters.read().to != to {
                                filters.write().to = to;
                            }
                        }
                    },
                }
            }
            svg { class: "date-ticks", "aria-hidden": "true",
                for (at, year) in span.ticks() {
                    text { key: "{year}", x: "{at}%", y: "0", "{year}" }
                }
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
