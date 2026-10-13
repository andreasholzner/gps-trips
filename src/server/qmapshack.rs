//! US-36/US-37/US-89 (ADR-0022): one-way export of the whole trip archive —
//! trips and collections (see `collections`) — into a
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
//! The archive is read over its HTTP API, as any client reads it (US-51,
//! ADR-0022's 2026-09-19 amendment): one consistent list of every trip and
//! one of every collection (see `repo::export`), then a track's geometry only
//! when that track is written. A record that changes after the lists were
//! read is caught up by the next run.

pub mod backup;
pub mod blob;
pub mod collections;
pub mod config;
#[cfg(any(test, feature = "test-support"))]
pub mod decode;
pub mod icons;
pub mod qtstream;
mod reconcile;
pub mod target;

use std::collections::HashSet;

use anyhow::Context;
use sqlx::SqliteConnection;
use time::OffsetDateTime;

use crate::models::{ExportCollection, ExportTrip, Tag};
use crate::server::archive_client::ArchiveClient;

use self::config::ExportConfig;
use self::reconcile::{
    reconcile_item, record, remove_stale_items, Desired, Folders, ItemContent, ItemOutcome,
};

/// `versioninfo.version` this exporter targets (QMapShack `DB_VERSION`,
/// stable since 2016). A target database with any other value is refused
/// untouched — migrating is QMapShack's own job.
pub const DB_VERSION: &str = "6";

/// Version tag of the `QMTrk` track blob this exporter writes (QMapShack
/// `VER_TRK`, stable since 2020).
pub const VER_TRK: u8 = 7;

/// Version tag of the `QMWpt` waypoint blob this exporter writes (QMapShack
/// `VER_WPT`, US-89).
pub const VER_WPT: u8 = 4;

/// Namespace prefix for `items.keyqms` marking rows this exporter owns
/// (ADR-0022): reconciliation must never touch items without this prefix.
pub const KEYQMS_PREFIX: &str = "trip-archive:trip:";

/// The deterministic `items.keyqms` value for a trip.
pub fn keyqms(trip_id: i64) -> String {
    format!("{KEYQMS_PREFIX}{trip_id}")
}

/// Per-run tallies, one count per per-item decision taken — trips and
/// collection items alike (every decision is also logged individually).
/// `failed > 0` maps to a non-zero exit code in the binary while still
/// letting the run complete (ADR-0022).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExportOutcome {
    pub inserted: u64,
    pub updated: u64,
    pub removed: u64,
    pub skipped: u64,
    pub failed: u64,
}

/// Reconcile the QMapShack database configured in `cfg` to the current
/// state of the archive at `archive_url`: insert new trips and collection
/// items, update/re-link changed ones, trash removed ones (US-37, US-89).
/// The complete trip and collection lists are read first (US-51): a run
/// that cannot get them — refused, unreachable, an error, a truncated or
/// malformed answer — fails before the target is opened, let alone backed
/// up, and so never takes a record's absence for its deletion. Setup errors
/// (target/gate/backup) likewise return `Err` before any item is written; a
/// failure listing the target's items for a removal pass is also fatal, but
/// happens after the per-item writes (all idempotent — the next run heals).
/// Per-item and per-removal failures are logged, counted, and skipped.
pub async fn run_export(
    archive_url: &str,
    password: &str,
    cfg: &ExportConfig,
) -> anyhow::Result<ExportOutcome> {
    let archive = ArchiveClient::sign_in(archive_url, password)
        .await
        .context("signing in to the archive")?;
    let trips: Vec<ExportTrip> = archive
        .get_ok(&["api", "export", "trips"])
        .await
        .context("listing trips")?
        .json()
        .await
        .context("reading the trip list")?;
    let collections: Vec<ExportCollection> = archive
        .get_ok(&["api", "export", "collections"])
        .await
        .context("listing collections")?
        .json()
        .await
        .context("reading the collection list")?;

    let mut target_conn = open_target(cfg).await?;
    let mut folders = Folders::new(target::root_folder_id(&mut target_conn).await?);

    tracing::info!(
        "exporting {} trip(s) and {} collection(s) to {}",
        trips.len(),
        collections.len(),
        cfg.target_db.display()
    );

    let mut outcome = ExportOutcome::default();
    for trip in &trips {
        let result = reconcile_trip(&archive, &mut target_conn, cfg, &mut folders, trip).await;
        record(
            &mut outcome,
            &format!("trip {} ({:?})", trip.id, trip.name),
            result,
        );
    }
    collections::export_collections(
        &archive,
        &mut target_conn,
        cfg,
        &mut folders,
        &collections,
        &mut outcome,
    )
    .await;

    // Removal passes: exporter-owned items whose record no longer exists in
    // the lists lose their folder links (→ QMapShack's trash).
    let trip_ids: HashSet<i64> = trips.iter().map(|t| t.id).collect();
    remove_stale_items(&mut target_conn, KEYQMS_PREFIX, &trip_ids, &mut outcome).await?;
    collections::remove_stale_items(&mut target_conn, &collections, &mut outcome).await?;

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

/// Reconcile one trip against the target (US-37): change detection compares
/// its name, its comment summary — which encodes every other exported
/// best-effort field (activity, kind, tags, stats, tz) — and its resolved
/// folder (ADR-0022 as amended), never its geometry.
async fn reconcile_trip(
    archive: &ArchiveClient,
    target_conn: &mut SqliteConnection,
    cfg: &ExportConfig,
    folders: &mut Folders,
    trip: &ExportTrip,
) -> anyhow::Result<ItemOutcome> {
    let keyqms = keyqms(trip.id);
    let summary = item_comment(trip);
    let folder_path = cfg.resolve_folder_path(
        trip.activity_type,
        trip.trip_kind,
        local_start_date(trip).as_deref(),
    );
    let desired = Desired {
        keyqms: &keyqms,
        name: &trip.name,
        comment: &summary,
        hash: None,
    };
    let content = build_trip_content(archive, trip, &keyqms, &summary);
    reconcile_item(target_conn, folders, folder_path, &desired, content).await
}

/// Fetch the trip's geometry from the archive and build its item content
/// (full rewrite semantics — a fresh single-event history, ADR-0022).
async fn build_trip_content(
    archive: &ArchiveClient,
    trip: &ExportTrip,
    keyqms: &str,
    summary: &str,
) -> anyhow::Result<ItemContent> {
    // A trip deleted since the list was read answers 404: this trip fails,
    // the next run's list no longer holds it.
    let id = trip.id.to_string();
    let points = fetch_points(archive, &["api", "trips", &id, "track.geojson"]).await?;
    let keywords = keywords(&trip.tags);

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
    Ok(ItemContent {
        item_type: target::ItemType::Track,
        icon: icons::icon_png(trip.activity_type),
        date,
        data,
        hash,
    })
}

/// Fetch a track's stored geometry from the archive at `path`.
async fn fetch_points(
    archive: &ArchiveClient,
    path: &[&str],
) -> anyhow::Result<Vec<blob::TrackPointIn>> {
    let geojson = archive
        .get_ok(path)
        .await
        .context("fetching track geometry")?
        .text()
        .await
        .context("reading track geometry")?;
    Ok(blob::points_from_geojson(&geojson)?)
}

/// Best-effort plain-text summary for `items.comment` (FTS-indexed by
/// QMapShack's trigger) and `trk.desc` — every attribute the archive holds
/// beyond name/geometry, since QMapShack has no structured slots for them
/// (ADR-0022 field scope; trips have no free-text description of their own).
/// The day the trip started on where it started — the date its page and
/// the archive's list give it, rather than UTC's.
fn local_start_date(trip: &ExportTrip) -> Option<String> {
    crate::server::repo::local_start_date(trip.start_time.as_deref(), trip.tz_name.as_deref())
}

fn item_comment(trip: &ExportTrip) -> String {
    let mut head = Vec::new();
    if let Some(start) = &trip.start_time {
        // The local date; stored rows are full RFC-3339, but never let a
        // malformed one panic — the date degrades to whatever is there.
        head.push(
            local_start_date(trip).unwrap_or_else(|| start.get(..10).unwrap_or(start).to_string()),
        );
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

    let mut lines: Vec<String> = tags_line(&trip.tags).into_iter().collect();
    lines.push(head.join(" · "));
    lines.push(stats.join(" · "));
    if let Some(tz) = &trip.tz_name {
        lines.push(format!("Timezone: {tz}"));
    }
    lines.push(format!("Exported from trip-archive (trip {})", trip.id));
    lines.join("\n")
}

/// The line that opens an item's comment (US-96): how its owner organized
/// it, before its figures. `None` without tags.
fn tags_line(tags: &[Tag]) -> Option<String> {
    if tags.is_empty() {
        return None;
    }
    let names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
    Some(format!("Tags: {}", names.join(", ")))
}

/// An item's QMapShack keywords: its tags, already in name order.
fn keywords(tags: &[Tag]) -> Vec<String> {
    tags.iter().map(|t| t.name.clone()).collect()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ActivityType, Tag, TripKind};

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
            tags: Vec::new(),
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
        let mut tagged = trip(Some("2024-06-01T08:00:00Z"));
        tagged.tags = vec![tag("fjell"), tag("telt")];
        let comment = item_comment(&tagged);
        assert_eq!(
            comment,
            "Tags: fjell, telt\n\
             2024-06-01 · Hiking · recorded\n\
             12.3 km · ↑400 m · ↓380 m · 3 h 20 min\n\
             Timezone: Europe/Oslo\n\
             Exported from trip-archive (trip 42)"
        );
    }

    #[test]
    fn a_trip_is_dated_where_it_started_not_in_utc() {
        // Half an hour into the new year in Oslo, still the old one in UTC —
        // the date and year folder must agree with the trip's own page.
        let trip = trip(Some("2024-12-31T23:30:00Z"));
        assert_eq!(local_start_date(&trip).as_deref(), Some("2025-01-01"));
        assert!(
            item_comment(&trip).starts_with("2025-01-01 · "),
            "{}",
            item_comment(&trip)
        );
    }

    #[test]
    fn item_comment_survives_a_malformed_short_start_time() {
        // Repo-written rows are full RFC-3339, but a corrupted/hand-edited
        // row must fail that one trip at worst — never panic the whole run.
        let comment = item_comment(&trip(Some("2024")));
        assert!(comment.starts_with("2024 · Hiking"), "{comment}");
    }

    #[test]
    fn item_comment_omits_what_the_trip_does_not_have() {
        let mut bare = trip(None);
        bare.ascent_m = None;
        bare.descent_m = None;
        bare.duration_secs = None;
        bare.tz_name = None;
        let comment = item_comment(&bare);
        assert_eq!(
            comment,
            "Hiking · recorded\n12.3 km\nExported from trip-archive (trip 42)"
        );
    }
}
