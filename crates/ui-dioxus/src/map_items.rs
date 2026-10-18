//! What the trip list's map draws: the matching trips (US-63/73) and the
//! listed collections' tracks (US-87), together (US-90). The marks, the
//! lines and the legend are worked out the same way for all of them — the
//! views are shared, the records are not (ADR-0028).

use serde::{Deserialize, Serialize};
use trip_archive_types::{ActivityType, BoundingBox, CollectionSummary, TripSummary};

use crate::kinds::Kind;

/// Where an item's track is read from and which page a click on its line
/// opens: a trip and a collection track may share an id, so the two are
/// never told apart by it alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapSource {
    Trips,
    CollectionTracks,
}

/// One trip or collection track on the map.
#[derive(Debug, Clone, PartialEq)]
pub struct MapItem {
    /// Which kind lists it, and so how its line is drawn (US-90).
    pub kind: Kind,
    pub id: i64,
    pub name: String,
    pub activity_type: ActivityType,
    /// `None` for a trip whose track has no points: it gets neither a mark
    /// nor a line.
    pub bbox: Option<BoundingBox>,
}

impl MapItem {
    pub fn source(&self) -> MapSource {
        match self.kind {
            Kind::Recorded | Kind::Planned => MapSource::Trips,
            Kind::Collections => MapSource::CollectionTracks,
        }
    }

    /// What tells it apart from every other item on the map.
    pub fn key(&self) -> (MapSource, i64) {
        (self.source(), self.id)
    }
}

/// Everything the map shows for the list.
#[derive(Debug, Clone, PartialEq)]
pub struct Mapped {
    /// In list order — the trips, then each listed collection's tracks —
    /// which the lines' shades follow (US-72).
    pub items: Vec<MapItem>,
    /// `[lat, lon]` points the map is fitted to besides the items' boxes:
    /// the corners around each collection's waypoints, so fitting takes
    /// them in too.
    pub also_fit: Vec<[f64; 2]>,
    /// The listed collections that have waypoints, and where, so the map
    /// can show those in view with the lines (US-90).
    pub waypointed: Vec<Waypointed>,
}

/// A listed collection's waypoints: whose, and the rectangle around them,
/// `[west, south, east, north]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Waypointed {
    pub collection: i64,
    pub bounds: [f64; 4],
}

impl From<&TripSummary> for MapItem {
    fn from(trip: &TripSummary) -> Self {
        let bbox = match (trip.min_lat, trip.min_lon, trip.max_lat, trip.max_lon) {
            (Some(min_lat), Some(min_lon), Some(max_lat), Some(max_lon)) => Some(BoundingBox {
                min_lat,
                min_lon,
                max_lat,
                max_lon,
            }),
            _ => None,
        };
        MapItem {
            kind: Kind::of_trip(trip.trip_kind),
            id: trip.id,
            name: trip.name.clone(),
            activity_type: trip.activity_type,
            bbox,
        }
    }
}

impl Mapped {
    /// The matching trips, then every track of the listed collections —
    /// collection by collection in list order — and the corners around
    /// their waypoints.
    pub fn of(trips: &[TripSummary], collections: &[CollectionSummary]) -> Self {
        let tracks = collections
            .iter()
            .flat_map(|collection| &collection.tracks)
            .map(|track| MapItem {
                kind: Kind::Collections,
                id: track.id,
                name: track.name.clone(),
                activity_type: track.activity_type,
                bbox: Some(BoundingBox {
                    min_lat: track.min_lat,
                    min_lon: track.min_lon,
                    max_lat: track.max_lat,
                    max_lon: track.max_lon,
                }),
            });
        let waypointed: Vec<Waypointed> = collections
            .iter()
            .filter_map(|collection| {
                Some(Waypointed {
                    collection: collection.id,
                    bounds: collection.waypoint_bounds?,
                })
            })
            .collect();
        let also_fit = waypointed
            .iter()
            .flat_map(|w| {
                let [west, south, east, north] = w.bounds;
                [[south, west], [north, east]]
            })
            .collect();
        Mapped {
            items: trips.iter().map(MapItem::from).chain(tracks).collect(),
            also_fit,
            waypointed,
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use trip_archive_types::{CollectionTrackMark, TripKind};

    fn a_trip(id: i64, bbox: Option<[f64; 4]>) -> TripSummary {
        let [min_lat, min_lon, max_lat, max_lon] = match bbox {
            Some(corners) => corners.map(Some),
            None => [None; 4],
        };
        TripSummary {
            tags: Vec::new(),
            id,
            name: format!("Trip {id}"),
            activity_type: ActivityType::Hiking,
            start_time: None,
            start_date: None,
            distance_m: 1_000.0,
            ascent_m: None,
            duration_secs: None,
            trip_kind: TripKind::Recorded,
            privacy_status: None,
            min_lat,
            min_lon,
            max_lat,
            max_lon,
            has_recorded_trip: false,
        }
    }

    fn a_track(id: i64, activity_type: ActivityType) -> CollectionTrackMark {
        CollectionTrackMark {
            tags: Vec::new(),
            id,
            name: format!("Stage {id}"),
            activity_type,
            distance_m: 40_000.0,
            ascent_m: 800.0,
            min_lat: 46.0,
            min_lon: 9.0,
            max_lat: 46.5,
            max_lon: 9.5,
            has_recorded_trip: false,
        }
    }

    fn a_collection(
        tracks: Vec<CollectionTrackMark>,
        waypoint_bounds: Option<[f64; 4]>,
    ) -> CollectionSummary {
        CollectionSummary {
            tags: Vec::new(),
            id: 1,
            name: "Book".to_string(),
            activity_type: None,
            waypoint_count: 0,
            track_count: tracks.len() as i64,
            tracks,
            waypoint_bounds,
            tracks_with_recorded_trip: 0,
        }
    }

    #[test]
    fn a_trip_is_drawn_by_its_bounding_box() {
        let mapped = Mapped::of(
            &[a_trip(4, Some([59.0, 10.0, 60.0, 11.0])), a_trip(5, None)],
            &[],
        );

        assert_eq!(mapped.items[0].source(), MapSource::Trips);
        assert_eq!(
            mapped.items[0],
            MapItem {
                kind: Kind::Recorded,
                id: 4,
                name: "Trip 4".to_string(),
                activity_type: ActivityType::Hiking,
                bbox: Some(BoundingBox {
                    min_lat: 59.0,
                    min_lon: 10.0,
                    max_lat: 60.0,
                    max_lon: 11.0,
                }),
            }
        );
        assert_eq!(mapped.items[1].bbox, None);
        assert!(mapped.also_fit.is_empty());
    }

    #[test]
    fn us87_every_track_of_every_listed_collection_is_drawn_in_list_order() {
        let mapped = Mapped::of(
            &[],
            &[
                a_collection(
                    vec![
                        a_track(3, ActivityType::Cycling),
                        a_track(1, ActivityType::Unknown),
                    ],
                    None,
                ),
                a_collection(vec![a_track(2, ActivityType::Hiking)], None),
            ],
        );

        assert!(mapped
            .items
            .iter()
            .all(|item| item.source() == MapSource::CollectionTracks));
        let drawn: Vec<_> = mapped
            .items
            .iter()
            .map(|item| (item.id, item.name.as_str(), item.activity_type))
            .collect();
        assert_eq!(
            drawn,
            [
                (3, "Stage 3", ActivityType::Cycling),
                (1, "Stage 1", ActivityType::Unknown),
                (2, "Stage 2", ActivityType::Hiking),
            ]
        );
        assert_eq!(
            mapped.items[0].bbox,
            Some(BoundingBox {
                min_lat: 46.0,
                min_lon: 9.0,
                max_lat: 46.5,
                max_lon: 9.5,
            })
        );
    }

    #[test]
    fn us87_fitting_to_collections_takes_in_their_waypoints() {
        let mapped = Mapped::of(
            &[],
            &[
                a_collection(vec![a_track(1, ActivityType::Hiking)], None),
                // Waypoints alone: nothing to mark, still something to fit to.
                a_collection(Vec::new(), Some([9.17, 46.49, 9.84, 46.58])),
            ],
        );

        assert_eq!(mapped.also_fit, [[46.49, 9.17], [46.58, 9.84]]);
        assert_eq!(
            mapped.waypointed,
            [Waypointed {
                collection: 1,
                bounds: [9.17, 46.49, 9.84, 46.58],
            }],
            "US-90: whose waypoints lie where"
        );
    }

    #[test]
    fn us90_trips_and_collection_tracks_are_mapped_together_trips_first() {
        let planned = TripSummary {
            trip_kind: TripKind::Planned,
            ..a_trip(7, Some([59.0, 10.0, 60.0, 11.0]))
        };
        let mapped = Mapped::of(
            &[a_trip(4, Some([59.0, 10.0, 60.0, 11.0])), planned],
            &[a_collection(vec![a_track(7, ActivityType::Cycling)], None)],
        );

        let drawn: Vec<_> = mapped
            .items
            .iter()
            .map(|item| (item.kind, item.key()))
            .collect();
        assert_eq!(
            drawn,
            [
                (Kind::Recorded, (MapSource::Trips, 4)),
                (Kind::Planned, (MapSource::Trips, 7)),
                (Kind::Collections, (MapSource::CollectionTracks, 7)),
            ],
            "a trip and a track sharing an id are told apart"
        );
    }
}
