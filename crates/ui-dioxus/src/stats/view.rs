//! What the statistics screen is showing (US-77): the period, the measure
//! and the activity its three controls pick, and their place in the URL, so
//! a view can be bookmarked the way a narrowed trip list can (US-52).
//!
//! Pure and Dioxus-free, like `filters.rs`.

use trip_archive_types::ActivityType;

/// The one figure the screen shows at a time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Measure {
    #[default]
    Distance,
    Ascent,
    MovingTime,
    Trips,
    DaysOut,
}

impl Measure {
    pub const ALL: [Measure; 5] = [
        Self::Distance,
        Self::Ascent,
        Self::MovingTime,
        Self::Trips,
        Self::DaysOut,
    ];

    /// The URL's spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Distance => "distance",
            Self::Ascent => "ascent",
            Self::MovingTime => "moving_time",
            Self::Trips => "trips",
            Self::DaysOut => "days_out",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Distance => "Distance",
            Self::Ascent => "Ascent",
            Self::MovingTime => "Moving time",
            Self::Trips => "Trips",
            Self::DaysOut => "Days out",
        }
    }

    /// The label a chart's axis carries: the measure in its unit.
    pub fn axis_label(self) -> &'static str {
        match self {
            Self::Distance => "Distance (km)",
            Self::Ascent => "Ascent (m)",
            Self::MovingTime => "Moving time (h)",
            Self::Trips => "Trips",
            Self::DaysOut => "Days out",
        }
    }

    /// A value of this measure in its unit — kilometres, metres, hours or a
    /// count, as `figures` adds them up — the way the tables show it.
    pub fn format(self, value: f64) -> String {
        match self {
            Self::Distance if value < 100.0 => format!("{value:.1} km"),
            Self::Distance => format!("{value:.0} km"),
            Self::Ascent => format!("{value:.0} m"),
            Self::MovingTime => {
                let minutes = (value * 60.0).round() as i64;
                format!("{}:{:02} h", minutes / 60, minutes % 60)
            }
            Self::Trips | Self::DaysOut => format!("{value:.0}"),
        }
    }
}

impl std::str::FromStr for Measure {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|measure| measure.as_str() == value)
            .ok_or(())
    }
}

/// Every activity in the order the controls and tables list them: the
/// import form's, with the unspecified last.
pub fn activity_order() -> impl Iterator<Item = ActivityType> {
    ActivityType::SELECTABLE
        .into_iter()
        .chain([ActivityType::Unknown])
}

/// The three controls' choices. `None` is "all years"; no activities are
/// all of them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatsView {
    pub year: Option<i32>,
    pub measure: Measure,
    /// The chosen activities, in the order the control lists them.
    pub activities: Vec<ActivityType>,
}

impl StatsView {
    /// The activity, when exactly one is chosen — what the bars are drawn for.
    pub fn single(&self) -> Option<ActivityType> {
        match self.activities[..] {
            [activity] => Some(activity),
            _ => None,
        }
    }

    /// The view with `activity` ticked or unticked, keeping the control's
    /// order so the URL does not depend on the order of the clicks.
    pub fn toggled(&self, activity: ActivityType, on: bool) -> Self {
        let activities = activity_order()
            .filter(|candidate| {
                if *candidate == activity {
                    on
                } else {
                    self.activities.contains(candidate)
                }
            })
            .collect();
        Self {
            activities,
            ..self.clone()
        }
    }

    /// The query string, without a leading `?`; a choice left at its
    /// default is left out.
    pub fn to_query(&self) -> String {
        let mut params = Vec::new();
        if let Some(year) = self.year {
            params.push(format!("year={year}"));
        }
        if self.measure != Measure::default() {
            params.push(format!("measure={}", self.measure.as_str()));
        }
        if !self.activities.is_empty() {
            let activities: Vec<&str> = self.activities.iter().map(|a| a.as_str()).collect();
            params.push(format!("activity={}", activities.join(",")));
        }
        params.join("&")
    }

    /// The inverse of [`Self::to_query`]. Anything unreadable falls back to
    /// its default, so a hand-edited URL still opens the screen.
    pub fn from_query(query: &str) -> Self {
        let mut view = Self::default();
        for (name, value) in query.split('&').filter_map(|part| part.split_once('=')) {
            match name {
                "year" => view.year = value.parse().ok(),
                "measure" => view.measure = value.parse().unwrap_or_default(),
                "activity" => {
                    view.activities = value
                        .split(',')
                        .filter_map(|activity| activity.parse().ok())
                        .collect()
                }
                _ => {}
            }
        }
        view
    }
}

/// The router's half of the URL (US-52's mechanism). Every value is plain
/// ASCII, so there is nothing to escape.
impl std::fmt::Display for StatsView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_query())
    }
}

impl From<&str> for StatsView {
    fn from(query: &str) -> Self {
        Self::from_query(query)
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us77_a_view_round_trips_through_its_query() {
        let view = StatsView {
            year: Some(2024),
            measure: Measure::DaysOut,
            activities: vec![ActivityType::Hiking, ActivityType::SkiTouring],
        };

        assert_eq!(
            view.to_query(),
            "year=2024&measure=days_out&activity=hiking,ski_touring"
        );
        assert_eq!(StatsView::from_query(&view.to_query()), view);
    }

    #[test]
    fn us77_the_default_view_has_an_empty_query() {
        assert_eq!(StatsView::default().to_query(), "");
        assert_eq!(StatsView::from_query(""), StatsView::default());
    }

    #[test]
    fn us77_an_unreadable_query_falls_back_to_the_defaults() {
        assert_eq!(
            StatsView::from_query("year=soon&measure=speed&activity=flying&x"),
            StatsView::default()
        );
    }

    #[test]
    fn us77_ticking_keeps_the_controls_order_and_unticking_removes() {
        let view = StatsView::default()
            .toggled(ActivityType::Kayaking, true)
            .toggled(ActivityType::Hiking, true);

        assert_eq!(
            view.activities,
            [ActivityType::Hiking, ActivityType::Kayaking]
        );
        assert_eq!(view.single(), None);
        let view = view.toggled(ActivityType::Hiking, false);
        assert_eq!(view.single(), Some(ActivityType::Kayaking));
    }

    #[test]
    fn us77_each_measure_reads_in_its_own_unit() {
        assert_eq!(Measure::Distance.format(12.34), "12.3 km");
        assert_eq!(Measure::Distance.format(1234.5), "1234 km");
        assert_eq!(Measure::Ascent.format(1234.4), "1234 m");
        assert_eq!(Measure::MovingTime.format(12.5), "12:30 h");
        assert_eq!(Measure::Trips.format(3.0), "3");
        assert_eq!(Measure::DaysOut.format(14.0), "14");
    }
}
