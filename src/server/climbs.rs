//! Climbs (US-81): the stretches of a track whose elevation rises from a low
//! point to a high point, significant for the trip's activity, and the time
//! spent moving on them — what the climbing rate is worked out from.
//!
//! Pure, so it is tested directly (ADR-0012); `repo::moving_time` stores a
//! trip's figures, and the climbs themselves are found again whenever they
//! are asked for, under the trip's current activity.

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::config::{self, climbs::ClimbRule};
use crate::models::{ActivityType, Climb, INCLINE_WINDOW_M};

/// A track's elevation profile, point by point in track order, as its
/// stored blob carries it.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub distance_m: Vec<f64>,
    pub elevation_m: Vec<f64>,
    /// Unix seconds; `None` for a point the GPX gave no time.
    pub seconds: Vec<Option<i64>>,
}

impl Profile {
    /// The profile of a stored track blob (`geojson::build_track_geojson`);
    /// `None` for one that cannot be read, or whose arrays disagree.
    pub fn from_geojson(geojson: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(geojson).ok()?;
        let properties = &value["properties"];
        let numbers = |name: &str| -> Option<Vec<f64>> {
            properties[name]
                .as_array()?
                .iter()
                .map(|n| n.as_f64())
                .collect()
        };
        let distance_m = numbers("cumulative_distance_m")?;
        let elevation_m = numbers("elevation_m")?;
        let seconds: Vec<Option<i64>> = properties["timestamps"]
            .as_array()?
            .iter()
            .map(|timestamp| {
                let parsed = OffsetDateTime::parse(timestamp.as_str()?, &Rfc3339).ok()?;
                Some(parsed.unix_timestamp())
            })
            .collect();
        (distance_m.len() == elevation_m.len() && distance_m.len() == seconds.len()).then_some(
            Self {
                distance_m,
                elevation_m,
                seconds,
            },
        )
    }
}

/// The elevation at each point averaged over [`INCLINE_WINDOW_M`] of track
/// centred on it — the distance US-79's incline is measured over.
pub fn smoothed(distance_m: &[f64], elevation_m: &[f64]) -> Vec<f64> {
    let half = INCLINE_WINDOW_M / 2.0;
    let mut prefix = Vec::with_capacity(elevation_m.len() + 1);
    prefix.push(0.0);
    for elevation in elevation_m {
        prefix.push(prefix[prefix.len() - 1] + elevation);
    }
    distance_m
        .iter()
        .map(|&d| {
            let from = distance_m.partition_point(|&m| m < d - half);
            let to = distance_m.partition_point(|&m| m <= d + half);
            (prefix[to] - prefix[from]) / (to - from) as f64
        })
        .collect()
}

/// The climbs of `profile` under `rule`, in track order, each with its
/// moving time under `min_speed_kmh` (US-77).
///
/// A climb runs from a low point to the highest point after it, and ends
/// once the smoothed elevation falls more than `rule.max_dip_m` below that
/// highest point; it counts if it gains enough at a steep enough gradient.
pub fn find(profile: &Profile, rule: ClimbRule, min_speed_kmh: f64) -> Vec<Climb> {
    let smooth = smoothed(&profile.distance_m, &profile.elevation_m);
    let mut climbs = Vec::new();
    if smooth.is_empty() {
        return climbs;
    }
    let (mut low, mut high) = (0, 0);
    for k in 1..smooth.len() {
        if smooth[k] > smooth[high] {
            high = k;
        } else if smooth[high] - smooth[k] > rule.max_dip_m {
            climbs.extend(climb(profile, &smooth, low, high, rule, min_speed_kmh));
            (low, high) = (k, k);
        } else if high == low || smooth[k] < smooth[low] {
            // Not yet climbing, or fallen below where it started without
            // ever rising by a dip's worth: the low point moves on.
            (low, high) = (k, k);
        }
    }
    climbs.extend(climb(profile, &smooth, low, high, rule, min_speed_kmh));
    climbs
}

/// The stretch from `low` to `high` as a climb, if it is significant.
fn climb(
    profile: &Profile,
    smooth: &[f64],
    low: usize,
    high: usize,
    rule: ClimbRule,
    min_speed_kmh: f64,
) -> Option<Climb> {
    let gain_m = smooth[high] - smooth[low];
    let length_m = profile.distance_m[high] - profile.distance_m[low];
    let steep_enough = rule
        .min_gradient_pct
        .is_none_or(|min| gain_m / length_m * 100.0 >= min);
    (length_m > 0.0 && gain_m >= rule.min_gain_m && steep_enough).then(|| Climb {
        start_m: profile.distance_m[low],
        end_m: profile.distance_m[high],
        gain_m,
        moving_secs: moving_secs(profile, low, high, min_speed_kmh),
    })
}

/// The seconds between consecutive timed points from `low` to `high` whose
/// speed reaches `min_speed_kmh`, in track order; `None` when no two
/// consecutive points there carry times.
fn moving_secs(profile: &Profile, low: usize, high: usize, min_speed_kmh: f64) -> Option<i64> {
    let mut timed = false;
    let mut moving = 0;
    for i in low..high {
        let (Some(from), Some(to)) = (profile.seconds[i], profile.seconds[i + 1]) else {
            continue;
        };
        timed = true;
        let secs = to - from;
        let metres = profile.distance_m[i + 1] - profile.distance_m[i];
        if secs > 0 && metres / secs as f64 * 3.6 >= min_speed_kmh {
            moving += secs;
        }
    }
    timed.then_some(moving)
}

/// The climbs of a stored track blob under `activity`'s rule and moving
/// threshold; none for an activity without climbs, or a blob without a
/// readable profile.
pub fn of_track(geojson: &str, activity: ActivityType) -> Vec<Climb> {
    let (Some(rule), Some(profile)) = (
        config::climbs::rule(activity),
        Profile::from_geojson(geojson),
    ) else {
        return Vec::new();
    };
    find(&profile, rule, config::moving_time::min_speed_kmh(activity))
}

/// What a trip's climbing rate is worked out from: its climbs' heights
/// added up, and their moving time. A climb with no moving time adds to
/// neither, so it cannot make the rate infinite.
pub fn totals(climbs: &[Climb]) -> (f64, i64) {
    climbs
        .iter()
        .filter_map(|climb| Some((climb.gain_m, climb.moving_secs.filter(|secs| *secs > 0)?)))
        .fold((0.0, 0), |(gain, secs), (g, s)| (gain + g, secs + s))
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
