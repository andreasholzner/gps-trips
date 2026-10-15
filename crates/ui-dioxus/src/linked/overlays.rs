//! What the page's map draws of the rows ticked to be shown (US-99): each
//! trip's or track's line in its activity's color and its kind's line style
//! (US-90), each waypoint as its icon. Read from the archive per row; one
//! that cannot be read costs its own line, not the others.

use trip_archive_types::ActivityType;

use super::{Row, RowIcon, Shown};
use crate::activity_color;
use crate::activity_icon;
use crate::api::{self, ApiClient};
use crate::interop::{MapWaypoint, OverlayLine, Overlays};
use crate::kinds::Kind;
use crate::track::{self, Track};

/// A ticked row's line, from its track.
pub fn overlay_line(row: &Row, track: &Track) -> Option<OverlayLine> {
    let Shown::Line(kind, _) = row.shown else {
        return None;
    };
    let activity = match row.icon {
        RowIcon::Activity(activity) => activity,
        RowIcon::Symbol(_) => ActivityType::Unknown,
    };
    Some(OverlayLine {
        name: row.name.clone(),
        color: activity_color::color(activity),
        icon: activity_icon::map_icon(activity),
        to: row.to.clone(),
        dash: kind.dash(),
        points: track::polyline(track),
    })
}

/// The overlays of `ticked`, in their order, each read from `archive`.
pub async fn overlays_of(archive: &ApiClient, ticked: Vec<Row>) -> Overlays {
    let mut overlays = Overlays::default();
    for row in ticked {
        let read = match row.shown {
            Shown::Line(Kind::Collections, id) => api::get_collection_track_geometry(archive, id)
                .await
                .map(|track| overlays.lines.extend(overlay_line(&row, &track))),
            Shown::Line(_, id) => api::get_track(archive, id)
                .await
                .map(|track| overlays.lines.extend(overlay_line(&row, &track))),
            Shown::Waypoint(id) => api::get_waypoint(archive, id)
                .await
                .map(|detail| overlays.waypoints.push(MapWaypoint::from(&detail.waypoint))),
        };
        if let Err(err) = read {
            dioxus::logger::tracing::error!("could not read {} for the map: {err}", row.name);
        }
    }
    overlays
}
