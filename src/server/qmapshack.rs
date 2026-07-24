//! US-36/US-37 (ADR-0022): one-way export of the whole trip archive into a
//! QMapShack-compatible SQLite database, run by the `qmapshack_export` CLI
//! binary. Each run reconciles the target to the archive's current state:
//! new trips are inserted, edited trips updated/re-linked, deleted trips
//! moved to QMapShack's trash — never touching items outside the exporter's
//! `keyqms` namespace. The byte-level format targeted here is documented in
//! `docs/qmapshack-format.md`; the architectural decisions (namespaced
//! `keyqms` identity, owner-configured folder mapping, cheap-column change
//! detection, rolling backups, version gate, per-item best-effort
//! execution) in ADR-0022.
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

use std::collections::{HashMap, HashSet};

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
    pub updated: u64,
    pub removed: u64,
    pub skipped: u64,
    pub failed: u64,
}

/// What reconciliation did for one trip (US-37).
enum TripOutcome {
    Inserted { folder: Vec<String> },
    Updated { content: bool, relinked: bool },
    Skipped,
}

/// Reconcile the QMapShack database configured in `cfg` to the archive's
/// current state: insert new trips, update/re-link changed ones, trash
/// removed ones (US-37). Setup errors (config/target/gate/backup) return
/// `Err` before any item is written; a failure listing the target's items
/// for the removal pass is also fatal, but happens after the per-trip
/// writes (all idempotent — the next run heals). Per-trip and per-removal
/// failures are logged, counted, and skipped.
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
        match reconcile_trip(
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
            Ok(TripOutcome::Inserted { folder }) => {
                tracing::info!(
                    "inserted trip {} ({:?}) under {}",
                    trip.id,
                    trip.name,
                    folder.join("/")
                );
                outcome.inserted += 1;
            }
            Ok(TripOutcome::Updated { content, relinked }) => {
                tracing::info!(
                    "updated trip {} ({:?}): content changed: {content}, re-linked: {relinked}",
                    trip.id,
                    trip.name
                );
                outcome.updated += 1;
            }
            Ok(TripOutcome::Skipped) => {
                tracing::info!("skipping trip {} ({:?}): unchanged", trip.id, trip.name);
                outcome.skipped += 1;
            }
            Err(e) => {
                tracing::error!("failed to export trip {} ({:?}): {e:#}", trip.id, trip.name);
                outcome.failed += 1;
            }
        }
    }

    // US-37 removal pass: exporter-owned items whose trip no longer exists
    // in the snapshot lose their folder links (→ QMapShack's trash).
    let archive_ids: HashSet<i64> = trips.iter().map(|t| t.id).collect();
    remove_stale_items(&mut target_conn, &archive_ids, &mut outcome).await?;

    // Read-only snapshot — dropping the tx rolls it back.
    drop(archive_tx);
    tracing::info!(
        "export finished: {} inserted, {} updated, {} removed, {} skipped, {} failed",
        outcome.inserted,
        outcome.updated,
        outcome.removed,
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

/// Reconcile one trip against the target (US-37): insert it if it was never
/// exported, rewrite/re-link it if the cheap columns (name, comment
/// summary) or the resolved folder placement differ (ADR-0022 as amended),
/// skip it otherwise — without ever reading its geometry. Any error here
/// fails this trip only.
async fn reconcile_trip(
    archive_tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    target_conn: &mut SqliteConnection,
    cfg: &ExportConfig,
    folder_ids: &mut HashMap<Vec<String>, i64>,
    root: i64,
    trip: &ExportTrip,
    keyqms: &str,
) -> anyhow::Result<TripOutcome> {
    let tags = repo::list_trip_tags_in_tx(archive_tx, trip.id)
        .await
        .context("reading tags")?;
    let summary = item_comment(trip, &tags);

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

    let state = target::get_item_state(target_conn, keyqms)
        .await
        .context("looking up the existing item")?;
    let Some(state) = state else {
        let built = build_item_blob(archive_tx, trip, keyqms, &summary, &tags).await?;
        target::insert_item(
            target_conn,
            &built.as_new_item(trip, keyqms, &summary),
            folder_id,
        )
        .await
        .context("inserting the item")?;
        return Ok(TripOutcome::Inserted {
            folder: folder_path,
        });
    };

    // The comment summary encodes every other exported best-effort field
    // (activity, kind, tags, stats, tz), so these two column compares plus
    // the placement compare cover everything mutable in the archive.
    let content_changed =
        state.name != trip.name || state.comment.as_deref() != Some(summary.as_str());
    let placement_changed = state.folder_ids != [folder_id];
    if !content_changed && !placement_changed {
        return Ok(TripOutcome::Skipped);
    }
    if content_changed {
        let built = build_item_blob(archive_tx, trip, keyqms, &summary, &tags).await?;
        target::update_item(
            target_conn,
            state.id,
            &built.as_new_item(trip, keyqms, &summary),
        )
        .await
        .context("updating the item")?;
    }
    if placement_changed {
        target::set_item_folder(target_conn, state.id, folder_id)
            .await
            .context("re-linking the item")?;
    }
    Ok(TripOutcome::Updated {
        content: content_changed,
        relinked: placement_changed,
    })
}

/// The owned parts of an `items` row that require reading geometry and
/// building the blob — deferred until a write is actually needed.
struct BuiltItem {
    data: Vec<u8>,
    hash: String,
    date: String,
}

impl BuiltItem {
    fn as_new_item<'a>(
        &'a self,
        trip: &'a ExportTrip,
        keyqms: &'a str,
        summary: &'a str,
    ) -> target::NewItem<'a> {
        target::NewItem {
            keyqms,
            name: &trip.name,
            icon: icons::icon_png(trip.activity_type),
            date: &self.date,
            comment: summary,
            data: &self.data,
            hash: &self.hash,
        }
    }
}

/// Fetch the trip's geometry from the snapshot and build the `items.data`
/// blob (full rewrite semantics — a fresh single-event history, ADR-0022).
async fn build_item_blob(
    archive_tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    trip: &ExportTrip,
    keyqms: &str,
    summary: &str,
    tags: &[Tag],
) -> anyhow::Result<BuiltItem> {
    let geojson = repo::get_track_geojson_in_tx(archive_tx, trip.id)
        .await
        .context("reading track geometry")?
        .context("trip has no track geometry")?;
    let points = blob::points_from_geojson(&geojson)?;
    let keywords: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();

    let (data, hash) = blob::build_track_item(
        &blob::TrackBlobInput {
            keyqms,
            name: &trip.name,
            desc: Some(summary),
            trk_type: trip.activity_type.as_str(),
            color: icons::color(trip.activity_type),
            keywords: &keywords,
            points: &points,
        },
        OffsetDateTime::now_utc(),
    );

    let date = match &trip.start_time {
        Some(t) => t.clone(),
        None => OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .expect("RFC-3339 formatting of a valid OffsetDateTime never fails"),
    };
    Ok(BuiltItem { data, hash, date })
}

/// US-37 removal pass: every item in the exporter's `keyqms` namespace
/// whose trip id is gone from the archive snapshot is unlinked, letting
/// QMapShack's trigger move it to the trash. Already-trashed items are left
/// alone (and not re-counted); anything outside the namespace is never even
/// considered (ADR-0022 scoping). Per-item failures are counted, not fatal.
async fn remove_stale_items(
    target_conn: &mut SqliteConnection,
    archive_ids: &HashSet<i64>,
    outcome: &mut ExportOutcome,
) -> anyhow::Result<()> {
    let items = target::list_exporter_items(target_conn, KEYQMS_PREFIX)
        .await
        .context("listing exported items")?;
    for item in items {
        // An unparseable id suffix can't match any archive trip — it is an
        // orphan of this exporter's namespace and gets cleaned up the same.
        let trip_id = item.keyqms[KEYQMS_PREFIX.len()..].parse::<i64>().ok();
        if trip_id.is_some_and(|id| archive_ids.contains(&id)) {
            continue;
        }
        if !item.linked {
            tracing::debug!("stale item {} is already in the trash", item.keyqms);
            continue;
        }
        match target::unlink_item(target_conn, item.id).await {
            Ok(()) => {
                tracing::info!(
                    "removed item {} (trip gone from the archive): moved to QMapShack's trash",
                    item.keyqms
                );
                outcome.removed += 1;
            }
            Err(e) => {
                tracing::error!("failed to remove item {}: {e:#}", item.keyqms);
                outcome.failed += 1;
            }
        }
    }
    Ok(())
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
