//! Rolling backup of the target database before each write run (ADR-0022):
//! a timestamped copy next to the target, with retention keeping enough
//! history to cover at least one month of runs but never fewer than three
//! backups. Timestamps live in the filename (not mtime — copying or moving
//! the files must not change what they mean).

use std::path::{Path, PathBuf};

use anyhow::Context;
use time::format_description::FormatItem;
use time::macros::format_description;
use time::{OffsetDateTime, PrimitiveDateTime};

/// Filename timestamp format (UTC): `20260723T140501`.
const TIMESTAMP_FORMAT: &[FormatItem<'static>] =
    format_description!("[year][month][day]T[hour][minute][second]");

/// Minimum number of backups always kept, regardless of age.
const MIN_KEPT: usize = 3;

/// Retention window: everything younger than this is kept.
const RETENTION: time::Duration = time::Duration::days(31);

/// Copy `target` to `<stem>.backup-<timestamp>.db` in the same directory,
/// then prune backups no longer needed for retention. A failed copy is
/// fatal (the caller must not write without a backup); a failed prune only
/// warns — the guarantee is about *keeping* backups, not deleting them.
pub fn create_backup_and_prune(target: &Path, now: OffsetDateTime) -> anyhow::Result<PathBuf> {
    let stem = target
        .file_stem()
        .and_then(|s| s.to_str())
        .context("target path has no usable file name")?;
    let dir = target.parent().context("target path has no directory")?;

    let timestamp = now
        .to_offset(time::UtcOffset::UTC)
        .format(&TIMESTAMP_FORMAT)
        .expect("formatting a valid OffsetDateTime never fails");
    let backup_path = dir.join(format!("{stem}.backup-{timestamp}.db"));
    std::fs::copy(target, &backup_path).with_context(|| {
        format!(
            "backing up {} to {}",
            target.display(),
            backup_path.display()
        )
    })?;
    tracing::info!(
        "backed up {} to {}",
        target.display(),
        backup_path.display()
    );

    for name in backups_to_delete(&list_backups(dir, stem)?, now) {
        let path = dir.join(&name);
        match std::fs::remove_file(&path) {
            Ok(()) => tracing::info!("pruned old backup {}", path.display()),
            Err(e) => tracing::warn!("could not prune old backup {}: {e}", path.display()),
        }
    }
    Ok(backup_path)
}

/// All parseable `<stem>.backup-<timestamp>.db` files in `dir`. Files that
/// match the pattern but whose timestamp doesn't parse are warned about and
/// excluded — never listed, so never deleted.
fn list_backups(dir: &Path, stem: &str) -> anyhow::Result<Vec<(String, OffsetDateTime)>> {
    let prefix = format!("{stem}.backup-");
    let mut backups = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("listing {}", dir.display()))? {
        let name = entry?.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(timestamp) = name
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix(".db"))
        else {
            continue;
        };
        match PrimitiveDateTime::parse(timestamp, &TIMESTAMP_FORMAT) {
            Ok(parsed) => backups.push((name.to_string(), parsed.assume_utc())),
            Err(_) => tracing::warn!(
                "ignoring {name}: looks like a backup but its timestamp doesn't parse"
            ),
        }
    }
    Ok(backups)
}

/// Which backups to delete under the retention policy: keep the union of
/// {backups younger than one month} and {the `MIN_KEPT` newest overall},
/// delete everything else. Pure — unit-tested directly.
fn backups_to_delete(backups: &[(String, OffsetDateTime)], now: OffsetDateTime) -> Vec<String> {
    let mut by_age: Vec<&(String, OffsetDateTime)> = backups.iter().collect();
    by_age.sort_by_key(|(_, ts)| std::cmp::Reverse(*ts));

    let cutoff = now - RETENTION;
    by_age
        .into_iter()
        .enumerate()
        .filter(|(rank, (_, ts))| *rank >= MIN_KEPT && *ts < cutoff)
        .map(|(_, (name, _))| name.clone())
        .collect()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    const NOW: OffsetDateTime = datetime!(2026-07-23 12:00:00 UTC);

    fn backup(name: &str, ts: OffsetDateTime) -> (String, OffsetDateTime) {
        (name.to_string(), ts)
    }

    #[test]
    fn never_deletes_below_the_minimum_of_three() {
        let backups = [
            backup("a", datetime!(2020-01-01 00:00:00 UTC)),
            backup("b", datetime!(2021-01-01 00:00:00 UTC)),
            backup("c", datetime!(2022-01-01 00:00:00 UTC)),
        ];
        assert!(
            backups_to_delete(&backups, NOW).is_empty(),
            "three ancient backups all survive"
        );
    }

    #[test]
    fn keeps_everything_within_the_month_window() {
        let backups = [
            backup("a", NOW - time::Duration::days(1)),
            backup("b", NOW - time::Duration::days(10)),
            backup("c", NOW - time::Duration::days(20)),
            backup("d", NOW - time::Duration::days(28)),
            backup("e", NOW - time::Duration::days(30)),
        ];
        assert!(
            backups_to_delete(&backups, NOW).is_empty(),
            "frequent runs keep more than three to span the month"
        );
    }

    #[test]
    fn prunes_ancient_backups_beyond_the_three_newest() {
        let backups = [
            backup("newest", NOW - time::Duration::days(100)),
            backup("mid", NOW - time::Duration::days(200)),
            backup("old", NOW - time::Duration::days(300)),
            backup("older", NOW - time::Duration::days(400)),
            backup("oldest", NOW - time::Duration::days(500)),
        ];
        let mut doomed = backups_to_delete(&backups, NOW);
        doomed.sort();
        assert_eq!(doomed, ["older", "oldest"]);
    }

    #[test]
    fn keeps_the_union_of_recent_and_three_newest() {
        // 2 recent + 3 ancient: the three newest are the 2 recent + the
        // newest ancient one, so only the two oldest go.
        let backups = [
            backup("recent1", NOW - time::Duration::days(2)),
            backup("recent2", NOW - time::Duration::days(5)),
            backup("ancient1", NOW - time::Duration::days(90)),
            backup("ancient2", NOW - time::Duration::days(120)),
            backup("ancient3", NOW - time::Duration::days(150)),
        ];
        let mut doomed = backups_to_delete(&backups, NOW);
        doomed.sort();
        assert_eq!(doomed, ["ancient2", "ancient3"]);
    }

    #[test]
    fn create_backup_copies_and_prunes_but_spares_unparseable_names() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("Touren.db");
        std::fs::write(&target, b"database bytes").unwrap();

        // Two recent, three ancient, one unparseable pattern-lookalike.
        let mk = |name: &str| std::fs::write(dir.path().join(name), b"old").unwrap();
        mk("Touren.backup-20260722T090000.db");
        mk("Touren.backup-20260701T090000.db");
        mk("Touren.backup-20240101T090000.db");
        mk("Touren.backup-20230101T090000.db");
        mk("Touren.backup-20220101T090000.db");
        mk("Touren.backup-yesterday.db");

        let created = create_backup_and_prune(&target, NOW).expect("backup succeeds");
        assert_eq!(created, dir.path().join("Touren.backup-20260723T120000.db"));
        assert_eq!(std::fs::read(&created).unwrap(), b"database bytes");

        let survivors: Vec<String> = {
            let mut names: Vec<String> = std::fs::read_dir(dir.path())
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        };
        // The just-created backup counts toward the three-newest set, so all
        // three ancient backups fall outside the union and are pruned.
        assert_eq!(
            survivors,
            [
                "Touren.backup-20260701T090000.db",
                "Touren.backup-20260722T090000.db",
                "Touren.backup-20260723T120000.db",
                "Touren.backup-yesterday.db", // unparseable is never deleted
                "Touren.db",
            ]
        );
    }

    #[test]
    fn create_backup_fails_when_the_target_cannot_be_copied() {
        let dir = tempfile::tempdir().unwrap();
        let err = create_backup_and_prune(&dir.path().join("missing.db"), NOW)
            .expect_err("no target to back up");
        assert!(err.to_string().contains("missing.db"));
    }
}
