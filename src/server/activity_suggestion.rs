//! The activity type suggestion (US-76): what a track is, from what it runs
//! on, how steep it is and how fast it went. First match wins:
//!
//! 1. a winter trip is a ski trip — cross-country skiing if fast, ski
//!    touring if slow — even across frozen lakes;
//! 2. a track mostly on water is kayaking;
//! 3. one mostly on roads or good paths is cycling if fast, bikepacking if
//!    slow;
//! 4. one mostly off roads is hiking, or mountaineering if steep.
//!
//! A track none of these place gets no suggestion; snowshoeing and
//! "unspecified" are never suggested. Every threshold is in
//! `config::activity_suggestion`. Pure, so it is tested directly
//! (ADR-0012); `suggestion` gathers the facts.

use time::Month;

use crate::config::activity_suggestion::{
    CROSS_COUNTRY_MIN_KMH, CYCLING_MIN_KMH, MAJORITY, MOVING_MIN_KMH, STEEP_GRADIENT_PCT,
    STEEP_SHARE, WINTER_MONTHS,
};
use crate::models::{ActivityType, INCLINE_WINDOW_M};
use crate::server::{moving_time, profile::Profile};

/// What a track runs on, as shares of its length that add up to 1.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Ground {
    pub water: f64,
    /// Roads, and good paths: tracks, cycleways, gravel.
    pub road: f64,
    /// Small paths, or no way at all.
    pub off_road: f64,
    /// Where the ground data does not reach.
    pub unknown: f64,
}

/// What the rules decide on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Facts {
    pub ground: Ground,
    /// The month the trip started in, where the track says when.
    pub start_month: Option<Month>,
    /// The average speed in motion, in km/h, where the track has times.
    pub moving_kmh: Option<f64>,
    /// The share of the track at least `STEEP_GRADIENT_PCT` steep.
    pub steep_share: f64,
}

/// The activity `facts` suggest, if any.
pub fn suggest(facts: &Facts) -> Option<ActivityType> {
    let ground = facts.ground;
    if ground.unknown >= MAJORITY {
        return None;
    }
    let fast = |min_kmh: f64| facts.moving_kmh.map(|kmh| kmh >= min_kmh);
    if facts
        .start_month
        .is_some_and(|month| WINTER_MONTHS.contains(&month))
    {
        return Some(match fast(CROSS_COUNTRY_MIN_KMH) {
            Some(true) => ActivityType::CrossCountrySkiing,
            Some(false) | None => ActivityType::SkiTouring,
        });
    }
    if ground.water >= MAJORITY {
        return Some(ActivityType::Kayaking);
    }
    if ground.road >= MAJORITY {
        return Some(match fast(CYCLING_MIN_KMH) {
            Some(true) | None => ActivityType::Cycling,
            Some(false) => ActivityType::Bikepacking,
        });
    }
    if ground.off_road >= MAJORITY {
        return Some(if facts.steep_share >= STEEP_SHARE {
            ActivityType::Mountaineering
        } else {
            ActivityType::Hiking
        });
    }
    None
}

/// The average speed in motion of `profile`, in km/h — a stretch slower
/// than [`MOVING_MIN_KMH`] is a break — or `None` without times.
pub fn moving_kmh(profile: &Profile) -> Option<f64> {
    let moving = moving_time::moving(profile, MOVING_MIN_KMH)?;
    (moving.secs > 0).then(|| moving.distance_m / moving.secs as f64 * 3.6)
}

/// The share of `profile`'s length whose incline, up or down, measured over
/// US-79's window, is at least [`STEEP_GRADIENT_PCT`].
pub fn steep_share(profile: &Profile) -> f64 {
    let distance = &profile.distance_m;
    let Some(&total) = distance.last().filter(|&&total| total > 0.0) else {
        return 0.0;
    };
    let smooth = profile.smoothed();
    let mut steep = 0.0;
    for i in 0..distance.len().saturating_sub(1) {
        // Across the window ahead, or as much of it as the track has left.
        let ahead = distance.partition_point(|&d| d < distance[i] + INCLINE_WINDOW_M);
        let j = ahead.min(distance.len() - 1);
        let run = distance[j] - distance[i];
        if run > 0.0 && ((smooth[j] - smooth[i]) / run).abs() * 100.0 >= STEEP_GRADIENT_PCT {
            steep += distance[i + 1] - distance[i];
        }
    }
    steep / total
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn on(water: f64, road: f64, off_road: f64) -> Facts {
        Facts {
            ground: Ground {
                water,
                road,
                off_road,
                unknown: 1.0 - water - road - off_road,
            },
            start_month: Some(Month::July),
            moving_kmh: Some(4.0),
            steep_share: 0.0,
        }
    }

    fn at(kmh: f64, facts: Facts) -> Facts {
        Facts {
            moving_kmh: Some(kmh),
            ..facts
        }
    }

    fn in_month(month: Month, facts: Facts) -> Facts {
        Facts {
            start_month: Some(month),
            ..facts
        }
    }

    #[test]
    fn us76_a_fast_winter_trip_is_cross_country_skiing_and_a_slow_one_ski_touring() {
        let tracks = on(0.0, 0.8, 0.2);
        assert_eq!(
            suggest(&at(12.0, in_month(Month::February, tracks))),
            Some(ActivityType::CrossCountrySkiing)
        );
        assert_eq!(
            suggest(&at(3.5, in_month(Month::February, tracks))),
            Some(ActivityType::SkiTouring)
        );
    }

    #[test]
    fn us76_a_ski_trip_across_frozen_lakes_is_not_a_kayak_trip() {
        assert_eq!(
            suggest(&at(3.0, in_month(Month::March, on(0.9, 0.0, 0.1)))),
            Some(ActivityType::SkiTouring)
        );
    }

    #[test]
    fn us76_a_track_mostly_on_water_is_kayaking() {
        assert_eq!(suggest(&on(0.7, 0.1, 0.2)), Some(ActivityType::Kayaking));
    }

    #[test]
    fn us76_a_road_trip_is_cycling_when_fast_and_bikepacking_when_slow() {
        assert_eq!(
            suggest(&at(22.0, on(0.0, 0.9, 0.1))),
            Some(ActivityType::Cycling)
        );
        assert_eq!(
            suggest(&at(12.0, on(0.0, 0.9, 0.1))),
            Some(ActivityType::Bikepacking)
        );
    }

    #[test]
    fn us76_an_off_road_trip_is_hiking_or_when_steep_mountaineering() {
        let off_road = on(0.0, 0.2, 0.8);
        assert_eq!(suggest(&off_road), Some(ActivityType::Hiking));
        assert_eq!(
            suggest(&Facts {
                steep_share: 0.2,
                ..off_road
            }),
            Some(ActivityType::Mountaineering)
        );
    }

    #[test]
    fn us76_without_a_majority_there_is_no_suggestion() {
        assert_eq!(suggest(&on(0.3, 0.35, 0.35)), None);
    }

    #[test]
    fn us76_where_the_ground_data_does_not_reach_there_is_no_suggestion() {
        // Even in winter: the data is what the rules stand on.
        assert_eq!(suggest(&on(0.0, 0.2, 0.2)), None);
        assert_eq!(suggest(&in_month(Month::January, on(0.0, 0.2, 0.2))), None);
    }

    #[test]
    fn us76_without_a_speed_a_road_trip_is_cycling_and_a_winter_trip_ski_touring() {
        let untimed = |facts: Facts| Facts {
            moving_kmh: None,
            ..facts
        };
        assert_eq!(
            suggest(&untimed(on(0.0, 0.9, 0.1))),
            Some(ActivityType::Cycling)
        );
        assert_eq!(
            suggest(&untimed(in_month(Month::January, on(0.0, 0.1, 0.9)))),
            Some(ActivityType::SkiTouring)
        );
    }

    #[test]
    fn us76_without_a_date_there_is_no_winter_trip() {
        // A planned trip: no timestamps, so no date and no speed.
        let planned = Facts {
            start_month: None,
            moving_kmh: None,
            ..on(0.0, 0.1, 0.9)
        };
        assert_eq!(suggest(&planned), Some(ActivityType::Hiking));
    }

    #[test]
    fn us76_snowshoeing_and_unspecified_are_never_suggested() {
        for (water, road) in [(0.0, 0.0), (0.6, 0.0), (0.0, 0.6), (0.3, 0.3)] {
            for month in [Month::January, Month::July] {
                for kmh in [2.0, 10.0, 25.0] {
                    let facts = at(kmh, in_month(month, on(water, road, 1.0 - water - road)));
                    let suggested = suggest(&facts);
                    assert_ne!(suggested, Some(ActivityType::SnowShoe));
                    assert_ne!(suggested, Some(ActivityType::Unknown));
                }
            }
        }
    }

    /// A profile through `(distance m, elevation m)` points, 10 s apart.
    fn profile(points: &[(f64, f64)]) -> Profile {
        Profile {
            distance_m: points.iter().map(|p| p.0).collect(),
            elevation_m: points.iter().map(|p| p.1).collect(),
            seconds: (0..points.len()).map(|i| Some(i as i64 * 10)).collect(),
        }
    }

    /// Every 10 m up to `length`, rising `grade` metres per metre.
    fn slope(length: f64, grade: f64) -> Vec<(f64, f64)> {
        (0..=(length / 10.0) as usize)
            .map(|i| (i as f64 * 10.0, i as f64 * 10.0 * grade))
            .collect()
    }

    #[test]
    fn us76_the_moving_speed_leaves_the_breaks_out() {
        // 1 km in 10 minutes, then 10 minutes standing still.
        let mut points: Vec<(f64, f64)> =
            (0..=60).map(|i| (i as f64 * 1000.0 / 60.0, 0.0)).collect();
        points.extend((1..=60).map(|_| (1000.0, 0.0)));

        let kmh = moving_kmh(&profile(&points)).expect("timed");

        assert!((kmh - 6.0).abs() < 0.01, "{kmh}");
    }

    #[test]
    fn us76_an_untimed_track_has_no_moving_speed() {
        let mut untimed = profile(&slope(1000.0, 0.0));
        untimed.seconds.fill(None);
        assert_eq!(moving_kmh(&untimed), None);
    }

    #[test]
    fn us76_the_steep_share_is_the_length_at_the_steep_gradient() {
        // 1 km flat, then 1 km at 40 %.
        let mut points = slope(1000.0, 0.0);
        points.extend(
            slope(1000.0, 0.4)
                .into_iter()
                .skip(1)
                .map(|(d, e)| (d + 1000.0, e)),
        );

        let share = steep_share(&profile(&points));

        // The window smooths the edge; about half either way.
        assert!((share - 0.5).abs() < 0.1, "{share}");
        assert_eq!(steep_share(&profile(&slope(2000.0, 0.2))), 0.0);
        assert!(steep_share(&profile(&slope(2000.0, -0.35))) > 0.9);
    }
}
