//! Reconciling one exporter-owned item at a time (US-37, ADR-0022) — the part
//! trips and collection items (US-89) share: find or create its folder,
//! compare it with what the target holds, write only what differs, and the
//! removal pass that trashes items whose record is gone.

use std::collections::{HashMap, HashSet};
use std::future::Future;

use anyhow::Context;
use sqlx::SqliteConnection;

use super::target::{self, ItemType, NewItem};
use super::ExportOutcome;

/// The folders resolved so far in a run, by path, under the target's root.
pub(super) struct Folders {
    root: i64,
    ids: HashMap<Vec<String>, i64>,
}

impl Folders {
    pub(super) fn new(root: i64) -> Self {
        Self {
            root,
            ids: HashMap::new(),
        }
    }

    /// The folder at `path`, created where missing.
    async fn id(
        &mut self,
        conn: &mut SqliteConnection,
        path: &[String],
    ) -> Result<i64, sqlx::Error> {
        if let Some(id) = self.ids.get(path) {
            return Ok(*id);
        }
        let id = target::ensure_folder_path(conn, self.root, path).await?;
        self.ids.insert(path.to_vec(), id);
        Ok(id)
    }
}

/// What an item should look like as far as change detection compares it
/// (ADR-0022 as amended): its name and comment, and for a waypoint, whose
/// blob is built from the list alone, its hash.
pub(super) struct Desired<'a> {
    pub keyqms: &'a str,
    pub name: &'a str,
    pub comment: &'a str,
    pub hash: Option<&'a str>,
}

/// The rest of an item's row, built only when the item is written — for a
/// track, that means fetching its geometry.
pub(super) struct ItemContent {
    pub item_type: ItemType,
    pub icon: &'static [u8],
    pub date: String,
    pub data: Vec<u8>,
    pub hash: String,
}

impl ItemContent {
    fn new_item<'a>(&'a self, desired: &Desired<'a>) -> NewItem<'a> {
        NewItem {
            item_type: self.item_type,
            keyqms: desired.keyqms,
            name: desired.name,
            icon: self.icon,
            date: &self.date,
            comment: desired.comment,
            data: &self.data,
            hash: &self.hash,
        }
    }
}

/// What reconciliation did for one item.
pub(super) enum ItemOutcome {
    Inserted { folder: Vec<String> },
    Updated { content: bool, relinked: bool },
    Skipped,
}

/// Reconcile one item against the target: insert it if it was never
/// exported, rewrite and/or re-link it if what is compared or its folder
/// differs, skip it otherwise. `content` is only awaited for a write, so an
/// unchanged track's geometry is never fetched. Any error fails this item
/// only.
pub(super) async fn reconcile_item(
    conn: &mut SqliteConnection,
    folders: &mut Folders,
    folder_path: Vec<String>,
    desired: &Desired<'_>,
    content: impl Future<Output = anyhow::Result<ItemContent>>,
) -> anyhow::Result<ItemOutcome> {
    let folder_id = folders
        .id(conn, &folder_path)
        .await
        .context("creating folders")?;
    let state = target::get_item_state(conn, desired.keyqms)
        .await
        .context("looking up the existing item")?;
    let Some(state) = state else {
        let content = content.await?;
        target::insert_item(conn, &content.new_item(desired), folder_id)
            .await
            .context("inserting the item")?;
        return Ok(ItemOutcome::Inserted {
            folder: folder_path,
        });
    };

    let content_changed = state.name != desired.name
        || state.comment.as_deref() != Some(desired.comment)
        || desired.hash.is_some_and(|hash| hash != state.hash);
    let placement_changed = state.folder_ids != [folder_id];
    if !content_changed && !placement_changed {
        return Ok(ItemOutcome::Skipped);
    }
    if content_changed {
        let content = content.await?;
        target::update_item(conn, state.id, &content.new_item(desired))
            .await
            .context("updating the item")?;
    }
    if placement_changed {
        target::set_item_folder(conn, state.id, folder_id)
            .await
            .context("re-linking the item")?;
    }
    Ok(ItemOutcome::Updated {
        content: content_changed,
        relinked: placement_changed,
    })
}

/// Log and count what reconciling `what` (e.g. `trip 3 ("Tur")`) came to.
pub(super) fn record(outcome: &mut ExportOutcome, what: &str, result: anyhow::Result<ItemOutcome>) {
    match result {
        Ok(ItemOutcome::Inserted { folder }) => {
            tracing::info!("inserted {what} under {}", folder.join("/"));
            outcome.inserted += 1;
        }
        Ok(ItemOutcome::Updated { content, relinked }) => {
            tracing::info!("updated {what}: content changed: {content}, re-linked: {relinked}");
            outcome.updated += 1;
        }
        Ok(ItemOutcome::Skipped) => {
            tracing::info!("skipping {what}: unchanged");
            outcome.skipped += 1;
        }
        Err(e) => {
            tracing::error!("failed to export {what}: {e:#}");
            outcome.failed += 1;
        }
    }
}

/// Removal pass for one `keyqms` namespace (US-37, US-89): every item in it
/// whose record id is not in `live_ids` is unlinked, letting QMapShack's
/// trigger move it to the trash. Already-trashed items are left alone (and
/// not re-counted); anything outside the namespace is never even considered
/// (ADR-0022 scoping). Per-item failures are counted, not fatal.
pub(super) async fn remove_stale_items(
    conn: &mut SqliteConnection,
    prefix: &str,
    live_ids: &HashSet<i64>,
    outcome: &mut ExportOutcome,
) -> anyhow::Result<()> {
    let items = target::list_exporter_items(conn, prefix)
        .await
        .context("listing exported items")?;
    for item in items {
        // An unparseable id suffix can't match any record — it is an orphan
        // of this exporter's namespace and gets cleaned up the same.
        let id = item.keyqms[prefix.len()..].parse::<i64>().ok();
        if id.is_some_and(|id| live_ids.contains(&id)) {
            continue;
        }
        if !item.linked {
            tracing::debug!("stale item {} is already in the trash", item.keyqms);
            continue;
        }
        match target::unlink_item(conn, item.id).await {
            Ok(()) => {
                tracing::info!(
                    "removed item {} (gone from the archive): moved to QMapShack's trash",
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
