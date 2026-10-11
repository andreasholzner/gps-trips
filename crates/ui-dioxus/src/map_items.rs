//! What the trip list's map draws, whichever tab is showing: the matching
//! trips (US-63/73), or the listed collections' tracks (US-87). The marks,
//! the lines and the legend are worked out the same way for both — the views
//! are shared, the records are not (ADR-0028).

use trip_archive_types::{ActivityType, BoundingBox, CollectionSummary, TripSummary};

/// What the map's items are, so their tracks are read from the right place
/// and a click on a line opens the right page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapSource {
    Trips,
    CollectionTracks,
}

/// One trip or collection track on the map.
#[derive(Debug, Clone, PartialEq)]
pub struct MapItem {
    pub id: i64,
    pub name: String,
    pub activity_type: ActivityType,
    /// `None` for a trip whose track has no points: it gets neither a mark
    /// nor a line.
    pub bbox: Option<BoundingBox>,
}

/// Everything the map shows for one tab's list.
#[derive(Debug, Clone, PartialEq)]
pub struct Mapped {
    pub source: MapSource,
    /// In list order, which the lines' shades follow (US-72).
    pub items: Vec<MapItem>,
    /// `[lat, lon]` points the map is fitted to besides the items' marks:
    /// the corners around each collection's waypoints, so "Fit to
    /// collections" takes them in too.
    pub also_fit: Vec<[f64; 2]>,
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
            id: trip.id,
            name: trip.name.clone(),
            activity_type: trip.activity_type,
            bbox,
        }
    }
}

impl Mapped {
    /// The trips the filters match.
    pub fn trips(trips: &[TripSummary]) -> Self {
        Mapped {
            source: MapSource::Trips,
            items: trips.iter().map(MapItem::from).collect(),
            also_fit: Vec::new(),
        }
    }

    /// Every track of the listed collections, collection by collection in
    /// list order, and the corners around their waypoints.
    pub fn collections(collections: &[CollectionSummary]) -> Self {
        let items = collections
            .iter()
            .flat_map(|collection| &collection.tracks)
            .map(|track| MapItem {
                id: track.id,
                name: track.name.clone(),
                activity_type: track.activity_type,
                bbox: Some(BoundingBox {
                    min_lat: track.min_lat,
                    min_lon: track.min_lon,
                    max_lat: track.max_lat,
                    max_lon: track.max_lon,
                }),
            })
            .collect();
        let also_fit = collections
            .iter()
            .filter_map(|collection| collection.waypoint_bounds)
            .flat_map(|[west, south, east, north]| [[south, west], [north, east]])
            .collect();
        Mapped {
            source: MapSource::CollectionTracks,
            items,
            also_fit,
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
        }
    }

    fn a_track(id: i64, activity_type: ActivityType) -> CollectionTrackMark {
        CollectionTrackMark {
            id,
            name: format!("Stage {id}"),
            activity_type,
            min_lat: 46.0,
            min_lon: 9.0,
            max_lat: 46.5,
            max_lon: 9.5,
        }
    }

    fn a_collection(
        tracks: Vec<CollectionTrackMark>,
        waypoint_bounds: Option<[f64; 4]>,
    ) -> CollectionSummary {
        CollectionSummary {
            id: 1,
            name: "Book".to_string(),
            activity_type: None,
            waypoint_count: 0,
            tracks,
            waypoint_bounds,
        }
    }

    #[test]
    fn a_trip_is_drawn_by_its_bounding_box() {
        let mapped = Mapped::trips(&[a_trip(4, Some([59.0, 10.0, 60.0, 11.0])), a_trip(5, None)]);

        assert_eq!(mapped.source, MapSource::Trips);
        assert_eq!(
            mapped.items[0],
            MapItem {
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
        let mapped = Mapped::collections(&[
            a_collection(
                vec![
                    a_track(3, ActivityType::Cycling),
                    a_track(1, ActivityType::Unknown),
                ],
                None,
            ),
            a_collection(vec![a_track(2, ActivityType::Hiking)], None),
        ]);

        assert_eq!(mapped.source, MapSource::CollectionTracks);
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
        let mapped = Mapped::collections(&[
            a_collection(vec![a_track(1, ActivityType::Hiking)], None),
            // Waypoints alone: nothing to mark, still something to fit to.
            a_collection(Vec::new(), Some([9.17, 46.49, 9.84, 46.58])),
        ]);

        assert_eq!(mapped.also_fit, [[46.49, 9.17], [46.58, 9.84]]);
    }
}
