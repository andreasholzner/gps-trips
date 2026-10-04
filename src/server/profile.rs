//! A track's elevation profile, point by point in track order, read back
//! from its stored blob — what moving time (US-77) and climbs (US-81) are
//! both worked out from, so the two agree on every point.
//!
//! Pure, so it is tested directly (ADR-0012).

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::models::INCLINE_WINDOW_M;

/// Distance, elevation and time at each point of a track, in track order.
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
    ///
    /// The blob stores a point the GPX gave no elevation as `0.0`, which
    /// read as a real reading would drop the profile to sea level and back.
    /// So an elevation of exactly `0.0` is taken as missing and filled in
    /// from the points either side of it, by distance — unless every point
    /// reads `0.0`, a track without elevations, which stays flat. A real
    /// reading of exactly 0 m is rare, and filling it in from its
    /// neighbours changes next to nothing.
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
        if distance_m.len() != elevation_m.len() || distance_m.len() != seconds.len() {
            return None;
        }
        let elevation_m = filled_in(&distance_m, &elevation_m);
        Some(Self {
            distance_m,
            elevation_m,
            seconds,
        })
    }

    /// The elevation at each point averaged over [`INCLINE_WINDOW_M`] of
    /// track centred on it — the distance US-79's incline is measured over.
    pub fn smoothed(&self) -> Vec<f64> {
        let half = INCLINE_WINDOW_M / 2.0;
        let distance_m = &self.distance_m;
        let mut prefix = Vec::with_capacity(self.elevation_m.len() + 1);
        prefix.push(0.0);
        for elevation in &self.elevation_m {
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
}

/// `elevation_m` with each `0.0` — the blob's stand-in for a missing
/// elevation — replaced by the elevation interpolated by distance between
/// the nearest real readings either side, or the nearest one at either end
/// of the track. Unchanged when no reading is real.
fn filled_in(distance_m: &[f64], elevation_m: &[f64]) -> Vec<f64> {
    let real: Vec<usize> = (0..elevation_m.len())
        .filter(|&i| elevation_m[i] != 0.0)
        .collect();
    if real.is_empty() {
        return elevation_m.to_vec();
    }
    (0..elevation_m.len())
        .map(|i| {
            if elevation_m[i] != 0.0 {
                return elevation_m[i];
            }
            let after = real.partition_point(|&r| r < i);
            match (after.checked_sub(1).map(|b| real[b]), real.get(after)) {
                (Some(before), Some(&next)) => {
                    let span = distance_m[next] - distance_m[before];
                    let share = if span > 0.0 {
                        (distance_m[i] - distance_m[before]) / span
                    } else {
                        0.0
                    };
                    elevation_m[before] + (elevation_m[next] - elevation_m[before]) * share
                }
                (Some(before), None) => elevation_m[before],
                (None, Some(&next)) => elevation_m[next],
                (None, None) => unreachable!("there is a real reading"),
            }
        })
        .collect()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::geojson::build_track_geojson;
    use crate::server::gpx::{parse_gpx, TrackPoint};

    fn approx(actual: f64, expected: f64, within: f64) {
        assert!(
            (actual - expected).abs() <= within,
            "{actual} is not {expected} ± {within}"
        );
    }

    /// Points 10 m apart going north, at `elevations` (`None` for a point
    /// the GPX gave no elevation).
    fn blob(elevations: &[Option<f64>]) -> String {
        const DEGREES_PER_10_M: f64 = 10.0 / 111_195.0;
        let points: Vec<TrackPoint> = elevations
            .iter()
            .enumerate()
            .map(|(i, ele)| TrackPoint {
                lat: 60.0 + i as f64 * DEGREES_PER_10_M,
                lon: 10.0,
                ele: *ele,
                time: None,
            })
            .collect();
        build_track_geojson(&points)
    }

    #[test]
    fn us81_a_stored_track_is_read_back_point_by_point() {
        let track = parse_gpx(include_bytes!("../../tests/fixtures/sample.gpx")).unwrap();
        let blob = build_track_geojson(&track.points);

        let profile = Profile::from_geojson(&blob).expect("a profile");

        assert_eq!(profile.distance_m.len(), track.points.len());
        assert_eq!(profile.elevation_m.len(), track.points.len());
        assert!(profile.seconds.iter().all(Option::is_some));
    }

    #[test]
    fn us81_a_missing_elevation_is_filled_in_from_either_side() {
        let blob = blob(&[Some(1500.0), Some(1510.0), None, Some(1530.0), None]);

        let profile = Profile::from_geojson(&blob).unwrap();

        approx(profile.elevation_m[2], 1520.0, 0.1);
        // At the end of the track, the last real reading.
        assert_eq!(profile.elevation_m[4], 1530.0);
    }

    #[test]
    fn us81_a_track_without_elevations_stays_flat() {
        let blob = blob(&[None, None, None]);

        let profile = Profile::from_geojson(&blob).unwrap();

        assert_eq!(profile.elevation_m, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn us81_the_elevation_is_averaged_over_the_incline_window() {
        // A steady 10 % slope with ±3 m of jitter on every other point.
        let elevations: Vec<Option<f64>> = (0..=100)
            .map(|i| Some(500.0 + i as f64 + if i % 2 == 0 { 3.0 } else { -3.0 }))
            .collect();
        let profile = Profile::from_geojson(&blob(&elevations)).unwrap();

        let smoothed = profile.smoothed();

        // Mid-slope, the slope stays and the jitter shrinks to a fifth: the
        // window holds five points, which never cancel out entirely.
        approx(smoothed[50], 550.0, 3.0 / 5.0 + 0.01);
        approx(smoothed[51], 551.0, 3.0 / 5.0 + 0.01);
    }
}
