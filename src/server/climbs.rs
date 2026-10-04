//! Climbs (US-81): the stretches of a track whose elevation rises from a low
//! point to a high point, significant for the trip's activity, and the time
//! spent moving on them — what the climbing rate is worked out from.
//!
//! Pure, so it is tested directly (ADR-0012); `repo::moving_time` stores a
//! trip's figures, and the climbs themselves are found again whenever they
//! are asked for, under the trip's current activity.

use crate::config::{
    self,
    climbs::{ClimbRule, TRIM_WINDOW_M},
};
use crate::models::{ActivityType, Climb};
use crate::server::moving_time;
use crate::server::profile::Profile;

#[cfg(test)]
use crate::models::INCLINE_WINDOW_M;

/// Less than any rise worth the name, more than the averaging's rounding.
const ROUNDING_M: f64 = 1e-6;

/// The climbs of `profile` under `rule`, in track order, each with its
/// moving time under `min_speed_kmh` (US-77).
///
/// On the smoothed elevation, a climb runs from a low point to the highest
/// point after it, and ends once the elevation falls below that highest
/// point by more than `rule.max_dip_m` or `rule.max_dip_share` of the height
/// gained so far, whichever is more — at its top, not where the fall ended
/// it. Its gentle ends are then trimmed ([`trimmed`]), and it counts if it
/// still gains enough at a steep enough gradient.
pub fn find(profile: &Profile, rule: ClimbRule, min_speed_kmh: f64) -> Vec<Climb> {
    let smooth = profile.smoothed();
    let steps = moving_time::steps(profile, min_speed_kmh);
    let mut climbs = Vec::new();
    if smooth.is_empty() {
        return climbs;
    }
    let mut found = |low, high| {
        climbs.extend(climb(profile, &smooth, &steps, low, high, rule));
    };
    let (mut low, mut high) = (0, 0);
    for k in 1..smooth.len() {
        let gained = smooth[high] - smooth[low];
        let allowed = rule.max_dip_m.max(rule.max_dip_share * gained);
        // A real rise moves the top on, not the rounding of the average:
        // on a flat summit that would carry it along the flat.
        if smooth[k] > smooth[high] + ROUNDING_M {
            high = k;
        } else if smooth[high] - smooth[k] > allowed {
            found(low, high);
            (low, high) = (k, k);
        } else if high == low || smooth[k] < smooth[low] {
            // Not yet climbing, or fallen below where it started without
            // ever rising by a dip's worth: the low point moves on.
            (low, high) = (k, k);
        }
    }
    found(low, high);
    climbs
}

/// The stretch from `low` to `high` as a climb, trimmed, if it is
/// significant: its net height and average gradient decide that; its height
/// as reported is its rises added up, so a drop within it and the height
/// won back both count as they were climbed.
fn climb(
    profile: &Profile,
    smooth: &[f64],
    steps: &[Option<(i64, bool)>],
    low: usize,
    high: usize,
    rule: ClimbRule,
) -> Option<Climb> {
    let (low, high) = trimmed(&profile.distance_m, smooth, low, high, rule)?;
    let net_m = smooth[high] - smooth[low];
    let length_m = profile.distance_m[high] - profile.distance_m[low];
    let steep_enough = rule
        .min_gradient_pct
        .is_none_or(|min| net_m / length_m * 100.0 >= min);
    if length_m <= 0.0 || net_m < rule.min_gain_m || !steep_enough {
        return None;
    }
    let gain_m = smooth[low..=high]
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).max(0.0))
        .sum();
    Some(Climb {
        start_m: profile.distance_m[low],
        end_m: profile.distance_m[high],
        gain_m,
        moving_secs: moving_time::sum(profile, steps, low, high).map(|moving| moving.secs),
    })
}

/// `low..=high` with a gentle end cut off where it is longer than
/// `rule.trim_tolerance_m`. An end is gentle up to the first point, seen
/// from that end, where the slope over [`TRIM_WINDOW_M`] reaches
/// `rule.trim_gradient_pct`: a gentle approach or a plateau is not part of
/// the hill, but a summit that flattens out within the tolerance is. `None`
/// when the slope reaches it nowhere — a gentle rise, and no climb.
fn trimmed(
    distance_m: &[f64],
    smooth: &[f64],
    low: usize,
    high: usize,
    rule: ClimbRule,
) -> Option<(usize, usize)> {
    let steep = |from: usize, to: usize| {
        let run = distance_m[to] - distance_m[from];
        run > 0.0 && (smooth[to] - smooth[from]) / run * 100.0 >= rule.trim_gradient_pct
    };
    // The point the window reaches from `p`, forward or back, kept within
    // the climb.
    let ahead = |p: usize| {
        (p..=high)
            .find(|&q| distance_m[q] >= distance_m[p] + TRIM_WINDOW_M)
            .unwrap_or(high)
    };
    let behind = |p: usize| {
        (low..=p)
            .rev()
            .find(|&q| distance_m[q] <= distance_m[p] - TRIM_WINDOW_M)
            .unwrap_or(low)
    };

    let start = (low..high).find(|&p| steep(p, ahead(p)))?;
    let low = if distance_m[start] - distance_m[low] > rule.trim_tolerance_m {
        start
    } else {
        low
    };
    let end = ((low + 1)..=high).rev().find(|&p| steep(behind(p), p))?;
    let high = if distance_m[high] - distance_m[end] > rule.trim_tolerance_m {
        end
    } else {
        high
    };
    (low < high).then_some((low, high))
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
