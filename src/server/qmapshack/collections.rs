//! US-89 (ADR-0022's 2026-10-11 amendment): the collections in
//! the QMapShack export. Each collection is a folder placed by its own
//! template; its tracks become track items in their activity colors, its
//! waypoints waypoint items carrying their symbols. They are reconciled as
//! trips are, each kind of record in a `keyqms` namespace of its own, so no
//! removal pass can take another kind's item for a stale one of its own.

use std::collections::HashSet;
use std::future::ready;

use sqlx::SqliteConnection;
use time::OffsetDateTime;

use crate::models::{ExportCollection, ExportCollectionTrack, ExportWaypoint};
use crate::server::archive_client::ArchiveClient;

use super::blob::{self, waypoint::WaypointBlobInput};
use super::config::ExportConfig;
use super::reconcile::{self, record, Desired, Folders, ItemContent, ItemOutcome};
use super::target::ItemType;
use super::{fetch_points, icons, keywords, tags_line, ExportOutcome};

/// `keyqms` namespace of the items made from collection tracks.
pub const TRACK_KEYQMS_PREFIX: &str = "trip-archive:collection-track:";

/// `keyqms` namespace of the items made from collection waypoints.
pub const WAYPOINT_KEYQMS_PREFIX: &str = "trip-archive:waypoint:";

/// The deterministic `items.keyqms` value for a collection track.
pub fn track_keyqms(track_id: i64) -> String {
    format!("{TRACK_KEYQMS_PREFIX}{track_id}")
}

/// The deterministic `items.keyqms` value for a collection waypoint.
pub fn waypoint_keyqms(waypoint_id: i64) -> String {
    format!("{WAYPOINT_KEYQMS_PREFIX}{waypoint_id}")
}

/// Reconcile every collection's tracks and waypoints into its folder,
/// logging and counting each item; a failed item fails alone.
pub(super) async fn export_collections(
    archive: &ArchiveClient,
    conn: &mut SqliteConnection,
    cfg: &ExportConfig,
    folders: &mut Folders,
    collections: &[ExportCollection],
    outcome: &mut ExportOutcome,
) {
    for collection in collections {
        let folder_path = cfg.resolve_collection_folder_path(&collection.name);
        for track in &collection.tracks {
            let result = reconcile_track(
                archive,
                conn,
                folders,
                folder_path.clone(),
                collection,
                track,
            )
            .await;
            let what = format!("collection track {} ({:?})", track.id, track.name);
            record(outcome, &what, result);
        }
        for waypoint in &collection.waypoints {
            let result =
                reconcile_waypoint(conn, folders, folder_path.clone(), collection, waypoint).await;
            let what = format!("waypoint {} ({:?})", waypoint.id, waypoint_name(waypoint));
            record(outcome, &what, result);
        }
    }
}

/// The removal passes for collection items: a track or waypoint missing
/// from the list — deleted alone or with its collection — goes to the trash.
pub(super) async fn remove_stale_items(
    conn: &mut SqliteConnection,
    collections: &[ExportCollection],
    outcome: &mut ExportOutcome,
) -> anyhow::Result<()> {
    let tracks: HashSet<i64> = collections
        .iter()
        .flat_map(|c| &c.tracks)
        .map(|t| t.id)
        .collect();
    reconcile::remove_stale_items(conn, TRACK_KEYQMS_PREFIX, &tracks, outcome).await?;
    let waypoints: HashSet<i64> = collections
        .iter()
        .flat_map(|c| &c.waypoints)
        .map(|w| w.id)
        .collect();
    reconcile::remove_stale_items(conn, WAYPOINT_KEYQMS_PREFIX, &waypoints, outcome).await
}

/// A collection track is compared as a trip is — name, comment, folder; a
/// geometry replaced in place (US-88) shows in the comment's stats and
/// bounds. Its geometry is fetched only for a write.
async fn reconcile_track(
    archive: &ArchiveClient,
    conn: &mut SqliteConnection,
    folders: &mut Folders,
    folder_path: Vec<String>,
    collection: &ExportCollection,
    track: &ExportCollectionTrack,
) -> anyhow::Result<ItemOutcome> {
    let keyqms = track_keyqms(track.id);
    let comment = track_comment(collection, track);
    let desired = Desired {
        keyqms: &keyqms,
        name: &track.name,
        comment: &comment,
        hash: None,
    };
    let content = async {
        // A track deleted since the list was read answers 404: this track
        // fails, the next run's list no longer holds it.
        let id = track.id.to_string();
        let path = ["api", "collection-tracks", &id, "track.geojson"];
        let mut points = fetch_points(archive, &path).await?;
        // A collection's timestamps are not when anyone went (US-86).
        for point in &mut points {
            point.time = None;
        }
        let (data, hash) = blob::build_track_item(
            &blob::TrackBlobInput {
                keyqms: &keyqms,
                name: &track.name,
                desc: Some(&comment),
                trk_type: track.activity_type.as_str(),
                color: icons::color(track.activity_type),
                keywords: &keywords(&track.tags),
                points: &points,
            },
            OffsetDateTime::now_utc(),
        );
        Ok(ItemContent {
            item_type: ItemType::Track,
            icon: icons::icon_png(track.activity_type),
            date: collection.created_at.clone(),
            data,
            hash,
        })
    };
    reconcile::reconcile_item(conn, folders, folder_path, &desired, content).await
}

/// A waypoint's blob comes from the list alone and holds no time, so it is
/// built every run and compared by its hash.
async fn reconcile_waypoint(
    conn: &mut SqliteConnection,
    folders: &mut Folders,
    folder_path: Vec<String>,
    collection: &ExportCollection,
    waypoint: &ExportWaypoint,
) -> anyhow::Result<ItemOutcome> {
    let keyqms = waypoint_keyqms(waypoint.id);
    let name = waypoint_name(waypoint);
    let comment = waypoint_comment(collection, waypoint);
    let (data, hash) = blob::waypoint::build_waypoint_item(
        &WaypointBlobInput {
            keyqms: &keyqms,
            name: &name,
            desc: waypoint.description.as_deref(),
            lat: waypoint.lat,
            lon: waypoint.lon,
            ele: waypoint.ele,
            sym: waypoint.symbol.as_deref(),
        },
        OffsetDateTime::now_utc(),
    );
    let desired = Desired {
        keyqms: &keyqms,
        name: &name,
        comment: &comment,
        hash: Some(&hash),
    };
    let content = ItemContent {
        item_type: ItemType::Waypoint,
        icon: icons::waypoint_icon_png(),
        date: collection.created_at.clone(),
        data,
        hash: hash.clone(),
    };
    reconcile::reconcile_item(conn, folders, folder_path, &desired, ready(Ok(content))).await
}

/// A waypoint's item name: its own, or — `items.name` may not be empty —
/// its place in the collection.
fn waypoint_name(waypoint: &ExportWaypoint) -> String {
    match waypoint.name.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => format!("Waypoint {}", waypoint.position),
    }
}

/// Plain-text summary for a collection track's `items.comment` and
/// `trk.desc`: what the archive holds about it beyond name and geometry,
/// and the stats and bounds that tell a replaced geometry.
fn track_comment(collection: &ExportCollection, track: &ExportCollectionTrack) -> String {
    let mut lines: Vec<String> = tags_line(&track.tags).into_iter().collect();
    lines.extend([
        format!("{} · {}", collection.name, track.activity_type.label()),
        format!(
            "{:.1} km · ↑{} m · ↓{} m",
            track.distance_m / 1000.0,
            track.ascent_m.round() as i64,
            track.descent_m.round() as i64
        ),
        format!(
            "Bounds: {:.6}, {:.6} – {:.6}, {:.6}",
            track.min_lat, track.min_lon, track.max_lat, track.max_lon
        ),
    ]);
    let description = collection.description.trim();
    if !description.is_empty() {
        lines.push(description.to_string());
    }
    lines.push(format!(
        "Exported from trip-archive (collection {}, track {})",
        collection.id, track.id
    ));
    lines.join("\n")
}

/// `items.comment` for a waypoint: its description, if any, and where it
/// came from.
fn waypoint_comment(collection: &ExportCollection, waypoint: &ExportWaypoint) -> String {
    let source = format!(
        "Exported from trip-archive (collection {}, waypoint {})",
        collection.id, waypoint.id
    );
    match waypoint.description.as_deref().map(str::trim) {
        Some(description) if !description.is_empty() => format!("{description}\n{source}"),
        _ => source,
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ActivityType, Tag};

    fn collection(description: &str) -> ExportCollection {
        ExportCollection {
            id: 3,
            name: "Jotunheimen".to_string(),
            description: description.to_string(),
            created_at: "2026-10-01T10:00:00Z".to_string(),
            tracks: Vec::new(),
            waypoints: Vec::new(),
        }
    }

    fn track() -> ExportCollectionTrack {
        ExportCollectionTrack {
            id: 12,
            name: "Besseggen".to_string(),
            activity_type: ActivityType::Hiking,
            distance_m: 14_049.0,
            ascent_m: 1_100.4,
            descent_m: 1_099.6,
            min_lat: 61.4,
            min_lon: 8.6,
            max_lat: 61.512_345_6,
            max_lon: 8.8,
            tags: Vec::new(),
        }
    }

    fn waypoint(name: Option<&str>, description: Option<&str>) -> ExportWaypoint {
        ExportWaypoint {
            id: 40,
            position: 2,
            name: name.map(String::from),
            description: description.map(String::from),
            lat: 61.48,
            lon: 8.62,
            ele: None,
            symbol: None,
        }
    }

    #[test]
    fn us89_keyqms_namespaces_are_apart_from_each_other_and_the_trips() {
        assert_eq!(track_keyqms(12), "trip-archive:collection-track:12");
        assert_eq!(waypoint_keyqms(12), "trip-archive:waypoint:12");
        for prefix in [TRACK_KEYQMS_PREFIX, WAYPOINT_KEYQMS_PREFIX] {
            assert!(!prefix.starts_with(super::super::KEYQMS_PREFIX));
            assert!(!super::super::KEYQMS_PREFIX.starts_with(prefix));
        }
        assert!(
            track_keyqms(i64::MAX).len() < 64,
            "stays under the 64-char hedge"
        );
    }

    #[test]
    fn us89_a_track_comment_carries_its_collection_stats_and_bounds() {
        assert_eq!(
            track_comment(&collection("50 Touren"), &track()),
            "Jotunheimen · Hiking\n\
             14.0 km · ↑1100 m · ↓1100 m\n\
             Bounds: 61.400000, 8.600000 – 61.512346, 8.800000\n\
             50 Touren\n\
             Exported from trip-archive (collection 3, track 12)"
        );
    }

    #[test]
    fn us96_a_tagged_track_comment_starts_with_its_tags() {
        let mut tagged = track();
        tagged.tags = ["fjell", "telt"]
            .into_iter()
            .enumerate()
            .map(|(id, name)| Tag {
                id: id as i64,
                name: name.to_string(),
            })
            .collect();
        assert_eq!(
            track_comment(&collection(""), &tagged),
            "Tags: fjell, telt\n\
             Jotunheimen · Hiking\n\
             14.0 km · ↑1100 m · ↓1100 m\n\
             Bounds: 61.400000, 8.600000 – 61.512346, 8.800000\n\
             Exported from trip-archive (collection 3, track 12)"
        );
    }

    #[test]
    fn us89_a_track_comment_leaves_out_an_empty_description() {
        let comment = track_comment(&collection("  "), &track());
        assert!(
            comment.ends_with("8.800000\nExported from trip-archive (collection 3, track 12)"),
            "{comment}"
        );
    }

    #[test]
    fn us89_a_replaced_geometry_changes_the_track_comment() {
        let mut moved = track();
        moved.max_lat += 0.000_01;
        assert_ne!(
            track_comment(&collection(""), &track()),
            track_comment(&collection(""), &moved)
        );
    }

    #[test]
    fn us89_an_unnamed_waypoint_goes_by_its_place() {
        assert_eq!(waypoint_name(&waypoint(Some("Gjendebu"), None)), "Gjendebu");
        assert_eq!(waypoint_name(&waypoint(None, None)), "Waypoint 2");
        assert_eq!(waypoint_name(&waypoint(Some(" "), None)), "Waypoint 2");
    }

    #[test]
    fn us89_a_waypoint_comment_starts_with_its_description() {
        assert_eq!(
            waypoint_comment(&collection(""), &waypoint(None, Some("DNT-hytte"))),
            "DNT-hytte\nExported from trip-archive (collection 3, waypoint 40)"
        );
        assert_eq!(
            waypoint_comment(&collection(""), &waypoint(None, None)),
            "Exported from trip-archive (collection 3, waypoint 40)"
        );
    }
}
