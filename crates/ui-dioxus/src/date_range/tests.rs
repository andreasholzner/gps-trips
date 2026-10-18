//! US-110 — the date filter as one slider over the trips' months: what the
//! months hold, where the handles sit for a `from`/`to` and what they write
//! back, and the slider rendered (ADR-0012). Dragging a handle is a real
//! event — the browser layer's (`tests/browser/trip_list.spec.mjs`).

use super::*;
use crate::kinds::{Kind, Kinds};
use crate::test_support::render;
use time::macros::date;
use trip_archive_types::{ActivityType, TripKind};

const TODAY: Date = date!(2026 - 10 - 17);

fn a_trip(start_date: Option<&str>, trip_kind: TripKind) -> TripSummary {
    TripSummary {
        tags: Vec::new(),
        id: 1,
        name: "A trip".to_string(),
        activity_type: ActivityType::Hiking,
        start_time: None,
        start_date: start_date.map(str::to_string),
        distance_m: 1_000.0,
        ascent_m: None,
        duration_secs: None,
        trip_kind,
        privacy_status: None,
        min_lat: None,
        min_lon: None,
        max_lat: None,
        max_lon: None,
        has_recorded_trip: false,
    }
}

fn recorded(start_date: &str) -> TripSummary {
    a_trip(Some(start_date), TripKind::Recorded)
}

fn planned(start_date: &str) -> TripSummary {
    a_trip(Some(start_date), TripKind::Planned)
}

/// Sep 2012 to Oct 2026: the span of the owner's archive in the story.
fn archive_span() -> Span {
    Months::of(
        TODAY,
        Kinds::default(),
        Some(&[recorded("2012-09-03")]),
        &[],
    )
    .span
}

// ── The months ──────────────────────────────────────────────────────────────

#[test]
fn us110_the_months_run_from_the_oldest_trip_of_the_kinds_on_to_today() {
    let all = [
        recorded("2012-09-03"),
        recorded("2026-10-01"),
        recorded("2026-10-09"),
        planned("2011-02-01"),
        a_trip(None, TripKind::Recorded),
    ];

    let months = Months::of(TODAY, Kinds::default(), Some(&all), &[]);

    assert_eq!(months.bars.len(), 170, "Sep 2012 to Oct 2026");
    assert_eq!(months.bars[0].total, 1);
    assert_eq!(months.bars[169].total, 2);
    assert_eq!(months.bars.iter().map(|bar| bar.total).sum::<usize>(), 3);
    let both = Kinds::default().toggled(Kind::Planned);
    let months = Months::of(TODAY, both, Some(&all), &[]);
    assert_eq!(months.bars.len(), 189, "a planned trip from Feb 2011 too");
}

#[test]
fn us110_a_bar_holds_how_many_trips_match_the_other_filters() {
    let all = [
        recorded("2024-06-01"),
        recorded("2024-06-15"),
        recorded("2024-07-01"),
    ];
    let matching = [recorded("2024-06-15")];

    let months = Months::of(TODAY, Kinds::default(), Some(&all), &matching);

    assert_eq!(
        months.bars[0],
        Bar {
            total: 2,
            matching: 1
        }
    );
    assert_eq!(
        months.bars[1],
        Bar {
            total: 1,
            matching: 0
        }
    );
}

#[test]
fn us110_with_no_other_filter_set_every_trip_matches() {
    let all = [recorded("2024-06-01"), recorded("2024-07-01")];

    let months = Months::of(TODAY, Kinds::default(), Some(&all), &all);

    assert!(months.bars.iter().all(|bar| bar.total == bar.matching));
}

#[test]
fn us110_trips_after_today_are_not_counted() {
    let all = [planned("2024-06-01"), planned("2027-01-01")];
    let planned_only = Kinds::only(Kind::Planned);

    let months = Months::of(TODAY, planned_only, Some(&all), &all);

    assert_eq!(months.bars.iter().map(|bar| bar.total).sum::<usize>(), 1);
}

#[test]
fn us110_with_no_dated_trip_the_months_are_the_current_one_alone() {
    let undated = [a_trip(None, TripKind::Recorded)];
    let future = [planned("2027-01-01")];

    for all in [&undated[..], &future[..]] {
        let months = Months::of(
            TODAY,
            Kinds::default().toggled(Kind::Planned),
            Some(all),
            &[],
        );
        assert_eq!(months.bars, [Bar::default()]);
    }
    let unread = Months::of(TODAY, Kinds::default(), None, &[]);
    assert_eq!(unread.bars, [Bar::default()], "until the trips are read");
}

// ── The handles ─────────────────────────────────────────────────────────────

#[test]
fn us110_with_no_dates_the_handles_sit_at_the_ends() {
    assert_eq!(archive_span().handles("", ""), (0, 169));
}

#[test]
fn us110_a_date_puts_its_handle_on_the_month_holding_it() {
    let span = archive_span();

    assert_eq!(span.handles("2019-04-01", "2020-06-30"), (79, 93));
    assert_eq!(
        span.handles("2019-04-17", "2020-06-03"),
        (79, 93),
        "a day-exact date from a link"
    );
}

#[test]
fn us110_a_date_outside_the_span_puts_its_handle_at_the_nearer_end() {
    let span = archive_span();

    assert_eq!(span.handles("2010-01-01", "2030-01-01"), (0, 169));
    assert_eq!(span.handles("2030-01-01", "2010-01-01"), (169, 0));
}

#[test]
fn us110_an_unreadable_date_leaves_its_handle_at_the_end() {
    assert_eq!(archive_span().handles("someday", "2020-13-01"), (0, 169));
}

#[test]
fn us110_a_handle_at_an_end_means_no_limit() {
    let span = archive_span();

    assert_eq!(span.date_from(0), "");
    assert_eq!(span.date_to(169), "");
}

#[test]
fn us110_a_handle_moved_inward_covers_its_whole_month() {
    let span = archive_span();

    assert_eq!(span.date_from(79), "2019-04-01");
    assert_eq!(span.date_to(93), "2020-06-30");
    assert_eq!(span.date_to(137), "2024-02-29", "a leap year's February");
    assert_eq!(span.date_from(169), "2026-10-01", "the current month");
    assert_eq!(span.date_to(0), "2012-09-30", "the oldest trip's month");
}

#[test]
fn us110_what_a_handle_writes_puts_it_back_where_it_was() {
    let span = archive_span();

    for index in 0..=169 {
        let (from, _) = span.handles(&span.date_from(index), "");
        let (_, to) = span.handles("", &span.date_to(index));
        assert_eq!((from, to), (index, index), "month {index}");
    }
}

// ── What it says ────────────────────────────────────────────────────────────

#[test]
fn us110_the_range_is_named_by_its_months() {
    let span = archive_span();

    assert_eq!(span.summary("", ""), "Any date");
    assert_eq!(span.summary("2019-04-17", ""), "Apr. 2019 – today");
    assert_eq!(span.summary("", "2020-06-30"), "Sep. 2012 – Jun. 2020");
    assert_eq!(
        span.summary("2019-04-01", "2020-06-30"),
        "Apr. 2019 – Jun. 2020"
    );
    assert_eq!(
        span.summary("2010-01-01", ""),
        "Jan. 2010 – today",
        "a date from a link is named as it is, not as its handle sits"
    );
}

#[test]
fn us110_a_handle_is_named_by_its_month() {
    let span = archive_span();

    assert_eq!(span.label_at(0), "Sep. 2012");
    assert_eq!(span.label_at(79), "Apr. 2019");
    assert_eq!(span.label_at(169), "Oct. 2026");
}

#[test]
fn us110_the_years_are_marked_under_the_track_no_more_than_eight() {
    let ticks = archive_span().ticks();

    let years: Vec<_> = ticks.iter().map(|(_, year)| *year).collect();
    assert_eq!(years, [2013, 2015, 2017, 2019, 2021, 2023, 2025]);
    let (left, _) = ticks[0];
    assert!(
        (left - 4.0 / 169.0 * 100.0).abs() < 1e-9,
        "Jan 2013: {left}"
    );

    let short = Months::of(
        TODAY,
        Kinds::default(),
        Some(&[recorded("2025-03-01")]),
        &[],
    );
    let years: Vec<_> = short.span.ticks().into_iter().map(|(_, y)| y).collect();
    assert_eq!(years, [2026]);
}

// ── The slider rendered ─────────────────────────────────────────────────────

fn render_with(from: &str, to: &str, months: Months) -> String {
    let filters = Filters {
        from: from.to_string(),
        to: to.to_string(),
        ..Default::default()
    };
    render(move || {
        let filters = Signal::new(filters.clone());
        rsx! { DateRange { filters, months: months.clone() } }
    })
}

#[test]
fn us110_the_slider_offers_two_handles_named_by_their_months() {
    let html = render_with(
        "2019-04-01",
        "",
        Months::of(
            TODAY,
            Kinds::default(),
            Some(&[recorded("2012-09-03")]),
            &[],
        ),
    );

    let slider = |name: &str| {
        html.split("<input")
            .find(|input| input.contains(name))
            .unwrap_or_else(|| panic!("no {name}: {html}"))
            .to_string()
    };
    let earliest = slider("Earliest month");
    assert!(earliest.contains(r#"type="range""#), "{html}");
    assert!(earliest.contains(r#"max="169""#), "{html}");
    assert!(earliest.contains(r#"value="79""#), "{html}");
    assert!(earliest.contains(r#"aria-valuetext="Apr. 2019""#), "{html}");
    let latest = slider("Latest month");
    assert!(latest.contains(r#"value="169""#), "{html}");
    assert!(latest.contains(r#"aria-valuetext="today""#), "{html}");
    assert!(
        html.contains(r#"<output aria-live="off">"#),
        "the handles announce their months, the range is not repeated: {html}"
    );
    assert!(
        html.contains("Apr. 2019 – today"),
        "the range in words: {html}"
    );
}

#[test]
fn us110_a_date_in_the_current_month_is_not_today() {
    let months = Months::of(
        TODAY,
        Kinds::default(),
        Some(&[recorded("2012-09-03")]),
        &[],
    );

    let html = render_with("2026-10-01", "2026-10-05", months);

    for input in html.split("<input").skip(1) {
        assert!(input.contains(r#"value="169""#), "{html}");
        assert!(input.contains(r#"aria-valuetext="Oct. 2026""#), "{html}");
    }
}

#[test]
fn us110_a_bar_per_month_its_lower_part_what_matches_highlighted_between_the_handles() {
    let all = [
        recorded("2024-06-01"),
        recorded("2024-06-15"),
        recorded("2024-08-01"),
        recorded("2024-08-02"),
    ];
    let matching = [recorded("2024-06-15")];
    let months = Months::of(TODAY, Kinds::default(), Some(&all), &matching);

    let html = render_with("2024-08-01", "", months);

    let bars: Vec<_> = html.split(r#"<g class="bar"#).skip(1).collect();
    assert_eq!(bars.len(), 29, "Jun 2024 to Oct 2026: {html}");
    let heights = |bar: &str| -> Vec<String> {
        bar.split(r#"height=""#)
            .skip(1)
            .map(|rest| rest.split('"').next().unwrap_or_default().to_string())
            .collect()
    };
    assert!(
        bars[0].starts_with(r#"""#),
        "June lies outside the handles: {html}"
    );
    assert_eq!(
        heights(bars[0]),
        ["100", "50"],
        "the peak, one of its two matching"
    );
    assert_eq!(heights(bars[1]), ["0", "0"], "July is empty");
    assert!(
        bars[2].starts_with(r#" in""#),
        "August lies between: {html}"
    );
    assert_eq!(
        heights(bars[2]),
        ["100", "0"],
        "neither of its two matching"
    );
}

#[test]
fn us110_until_the_trips_are_read_there_are_no_bars() {
    let html = render_with("", "", Months::of(TODAY, Kinds::default(), None, &[]));

    assert!(html.contains("Any date"), "{html}");
    assert!(!html.contains(r#"height="100""#), "{html}");
}
