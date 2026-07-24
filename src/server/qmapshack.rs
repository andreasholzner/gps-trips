//! US-36 (ADR-0022): one-way export of the whole trip archive into a
//! QMapShack-compatible SQLite database, run by the `qmapshack_export` CLI
//! binary. The byte-level format targeted here is documented in
//! `docs/qmapshack-format.md`; the architectural decisions (namespaced
//! `keyqms` identity, owner-configured folder mapping, rolling backups,
//! version gate, per-item best-effort execution) in ADR-0022.
//!
//! Consistency: instead of the in-process US-26 lock (unreachable from a
//! separate CLI process), the whole run reads the archive through one open
//! transaction — a single WAL snapshot (see `repo::export`).

pub mod backup;
pub mod blob;
pub mod config;
#[cfg(any(test, feature = "test-support"))]
pub mod decode;
pub mod icons;
pub mod qtstream;
pub mod target;

use std::collections::HashMap;

use anyhow::Context;
use sqlx::{SqliteConnection, SqlitePool};
use time::OffsetDateTime;

use crate::models::Tag;
use crate::server::repo::{self, ExportTrip};

use self::config::ExportConfig;

/// `versioninfo.version` this exporter targets (QMapShack `DB_VERSION`,
/// stable since 2016). A target database with any other value is refused
/// untouched — migrating is QMapShack's own job.
pub const DB_VERSION: &str = "6";

/// Version tag of the `QMTrk` track blob this exporter writes (QMapShack
/// `VER_TRK`, stable since 2020).
pub const VER_TRK: u8 = 7;

/// Namespace prefix for `items.keyqms` marking rows this exporter owns
/// (ADR-0022): reconciliation must never touch items without this prefix.
pub const KEYQMS_PREFIX: &str = "trip-archive:trip:";

/// The deterministic `items.keyqms` value for a trip.
pub fn keyqms(trip_id: i64) -> String {
    format!("{KEYQMS_PREFIX}{trip_id}")
}

/// Per-run tallies, one count per per-trip decision taken (every decision is
/// also logged individually). `failed > 0` maps to a non-zero exit code in
/// the binary while still letting the run complete (ADR-0022).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExportOutcome {
    pub inserted: u64,
    pub skipped: u64,
    pub failed: u64,
}

/// What reconciliation decided for one trip. US-36 only inserts or skips;
/// US-37 will add update/remove without restructuring the loop.
enum ItemAction {
    Insert,
    Skip,
}

/// Export every trip in the archive into the QMapShack database configured
/// in `cfg`. Fatal errors (config/target/gate/backup) return `Err` before
/// any item is written; per-trip failures are logged, counted, and skipped.
pub async fn run_export(archive: &SqlitePool, cfg: &ExportConfig) -> anyhow::Result<ExportOutcome> {
    let mut target_conn = open_target(cfg).await?;

    let root = target::root_folder_id(&mut target_conn).await?;

    // One transaction for the whole run: a consistent WAL snapshot of the
    // archive, regardless of what the server commits while we run.
    let mut archive_tx = archive.begin().await.context("opening archive snapshot")?;
    let trips = repo::list_trips_for_export(&mut archive_tx)
        .await
        .context("listing trips")?;
    tracing::info!(
        "exporting {} trip(s) to {}",
        trips.len(),
        cfg.target_db.display()
    );

    let mut outcome = ExportOutcome::default();
    let mut folder_ids: HashMap<Vec<String>, i64> = HashMap::new();

    for trip in &trips {
        let key = keyqms(trip.id);
        let action = if target::item_exists(&mut target_conn, &key)
            .await
            .context("looking up existing items")?
        {
            ItemAction::Skip
        } else {
            ItemAction::Insert
        };
        match action {
            ItemAction::Skip => {
                tracing::info!(
                    "skipping trip {} ({:?}): already exported",
                    trip.id,
                    trip.name
                );
                outcome.skipped += 1;
            }
            ItemAction::Insert => {
                match insert_trip_item(
                    &mut archive_tx,
                    &mut target_conn,
                    cfg,
                    &mut folder_ids,
                    root,
                    trip,
                    &key,
                )
                .await
                {
                    Ok(folder) => {
                        tracing::info!(
                            "inserted trip {} ({:?}) under {}",
                            trip.id,
                            trip.name,
                            folder.join("/")
                        );
                        outcome.inserted += 1;
                    }
                    Err(e) => {
                        tracing::error!(
                            "failed to export trip {} ({:?}): {e:#}",
                            trip.id,
                            trip.name
                        );
                        outcome.failed += 1;
                    }
                }
            }
        }
    }

    // Read-only snapshot — dropping the tx rolls it back.
    drop(archive_tx);
    tracing::info!(
        "export finished: {} inserted, {} skipped, {} failed",
        outcome.inserted,
        outcome.skipped,
        outcome.failed
    );
    Ok(outcome)
}

/// Open the target for writing: gate first, back up before any write, and
/// bootstrap a full new database if the file doesn't exist yet (ADR-0022).
async fn open_target(cfg: &ExportConfig) -> anyhow::Result<SqliteConnection> {
    let path = &cfg.target_db;
    if !path.exists() {
        tracing::info!("target {} does not exist, creating it", path.display());
        let root_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .context("target_db path has no usable file name")?;
        return Ok(target::create_and_bootstrap(path, root_name).await?);
    }

    // Gate before backup: a target we refuse to write must stay untouched.
    let mut conn = target::open_existing(path).await?;
    target::check_version(&mut conn)
        .await
        .with_context(|| format!("target {}", path.display()))?;
    drop(conn);

    backup::create_backup_and_prune(path, OffsetDateTime::now_utc())?;
    Ok(target::open_existing(path).await?)
}

/// Fetch one trip's geometry and tags from the snapshot, build the item and
/// write it. Any error here fails this trip only.
async fn insert_trip_item(
    archive_tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    target_conn: &mut SqliteConnection,
    cfg: &ExportConfig,
    folder_ids: &mut HashMap<Vec<String>, i64>,
    root: i64,
    trip: &ExportTrip,
    keyqms: &str,
) -> anyhow::Result<Vec<String>> {
    let geojson = repo::get_track_geojson_in_tx(archive_tx, trip.id)
        .await
        .context("reading track geometry")?
        .context("trip has no track geometry")?;
    let points = blob::points_from_geojson(&geojson)?;
    let tags = repo::list_trip_tags_in_tx(archive_tx, trip.id)
        .await
        .context("reading tags")?;
    let keywords: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();

    let summary = item_comment(trip, &tags);
    let (data, hash) = blob::build_track_item(
        &blob::TrackBlobInput {
            keyqms,
            name: &trip.name,
            desc: Some(&summary),
            trk_type: trip.activity_type.as_str(),
            color: icons::color(trip.activity_type),
            keywords: &keywords,
            points: &points,
        },
        OffsetDateTime::now_utc(),
    );

    let folder_path = cfg.resolve_folder_path(
        trip.activity_type,
        trip.trip_kind,
        trip.start_time.as_deref(),
    );
    let folder_id = match folder_ids.get(&folder_path) {
        Some(id) => *id,
        None => {
            let id = target::ensure_folder_path(target_conn, root, &folder_path)
                .await
                .context("creating folders")?;
            folder_ids.insert(folder_path.clone(), id);
            id
        }
    };

    let now;
    let date = match &trip.start_time {
        Some(t) => t.as_str(),
        None => {
            now = OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .expect("RFC-3339 formatting of a valid OffsetDateTime never fails");
            now.as_str()
        }
    };
    target::insert_item(
        target_conn,
        &target::NewItem {
            keyqms,
            name: &trip.name,
            icon: icons::icon_png(trip.activity_type),
            date,
            comment: &summary,
            data: &data,
            hash: &hash,
        },
        folder_id,
    )
    .await
    .context("inserting the item")?;
    Ok(folder_path)
}

/// Best-effort plain-text summary for `items.comment` (FTS-indexed by
/// QMapShack's trigger) and `trk.desc` — every attribute the archive holds
/// beyond name/geometry, since QMapShack has no structured slots for them
/// (ADR-0022 field scope; trips have no free-text description of their own).
fn item_comment(trip: &ExportTrip, tags: &[Tag]) -> String {
    let mut head = Vec::new();
    if let Some(start) = &trip.start_time {
        // Stored rows are full RFC-3339, but never let a malformed one
        // panic — the date prefix degrades to whatever is there.
        head.push(start.get(..10).unwrap_or(start).to_string());
    }
    head.push(trip.activity_type.label().to_string());
    head.push(trip.trip_kind.as_str().to_string());

    let mut stats = vec![format!("{:.1} km", trip.distance_m / 1000.0)];
    if let Some(ascent) = trip.ascent_m {
        stats.push(format!("↑{} m", ascent.round() as i64));
    }
    if let Some(descent) = trip.descent_m {
        stats.push(format!("↓{} m", descent.round() as i64));
    }
    if let Some(secs) = trip.duration_secs {
        stats.push(format!("{} h {} min", secs / 3600, (secs % 3600) / 60));
    }

    let mut lines = vec![head.join(" · "), stats.join(" · ")];
    if !tags.is_empty() {
        let names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
        lines.push(format!("Tags: {}", names.join(", ")));
    }
    if let Some(tz) = &trip.tz_name {
        lines.push(format!("Timezone: {tz}"));
    }
    lines.push(format!("Exported from trip-archive (trip {})", trip.id));
    lines.join("\n")
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ActivityType, TripKind};

    fn trip(start: Option<&str>) -> ExportTrip {
        ExportTrip {
            id: 42,
            name: "Tur".to_string(),
            activity_type: ActivityType::Hiking,
            trip_kind: TripKind::Recorded,
            start_time: start.map(String::from),
            tz_name: Some("Europe/Oslo".to_string()),
            distance_m: 12_345.0,
            ascent_m: Some(400.4),
            descent_m: Some(380.0),
            duration_secs: Some(3 * 3600 + 20 * 60),
        }
    }

    fn tag(name: &str) -> Tag {
        Tag {
            id: 1,
            name: name.to_string(),
        }
    }

    #[test]
    fn keyqms_is_namespaced_and_deterministic() {
        assert_eq!(keyqms(42), "trip-archive:trip:42");
        assert!(keyqms(i64::MAX).len() < 64, "stays under the 64-char hedge");
    }

    #[test]
    fn item_comment_summarizes_every_best_effort_field() {
        let comment = item_comment(
            &trip(Some("2024-06-01T08:00:00Z")),
            &[tag("fjell"), tag("telt")],
        );
        assert_eq!(
            comment,
            "2024-06-01 · Hiking · recorded\n\
             12.3 km · ↑400 m · ↓380 m · 3 h 20 min\n\
             Tags: fjell, telt\n\
             Timezone: Europe/Oslo\n\
             Exported from trip-archive (trip 42)"
        );
    }

    #[test]
    fn item_comment_survives_a_malformed_short_start_time() {
        // Repo-written rows are full RFC-3339, but a corrupted/hand-edited
        // row must fail that one trip at worst — never panic the whole run.
        let comment = item_comment(&trip(Some("2024")), &[]);
        assert!(comment.starts_with("2024 · Hiking"), "{comment}");
    }

    #[test]
    fn item_comment_omits_what_the_trip_does_not_have() {
        let mut bare = trip(None);
        bare.ascent_m = None;
        bare.descent_m = None;
        bare.duration_secs = None;
        bare.tz_name = None;
        let comment = item_comment(&bare, &[]);
        assert_eq!(
            comment,
            "Hiking · recorded\n12.3 km\nExported from trip-archive (trip 42)"
        );
    }
}
