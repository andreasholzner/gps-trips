//! Photo blob storage abstraction (ADR-0007).
//!
//! All photo file I/O goes through the [`BlobStore`] trait so the backing store
//! can change without touching the import pipeline or the UI. v1 ships
//! [`LocalDisk`] (files under the data dir); a future `OwnCloudWebDav` impl swaps
//! in behind the same trait. The trait is also the seam where that *external*
//! backend gets mocked in tests (ADR-0012) — `LocalDisk` pointed at a `tempdir`
//! is the real collaborator everywhere else.

use std::io;
use std::path::{Path, PathBuf};

/// Stores and retrieves photo blobs by an opaque string key.
///
/// Methods are synchronous: `LocalDisk` is plain filesystem I/O, and callers on
/// the async path run it via `spawn_blocking` so the runtime is never blocked
/// (ADR-0004). The seam stays `Send + Sync` so it can be shared across handlers.
pub trait BlobStore: Send + Sync {
    /// Store `bytes` under `key`, overwriting any existing blob at that key.
    fn put(&self, key: &str, bytes: &[u8]) -> io::Result<()>;

    /// Read back the blob stored under `key`.
    fn get(&self, key: &str) -> io::Result<Vec<u8>>;

    /// Remove the blob stored under `key`. Idempotent: deleting a key that no
    /// longer exists is not an error — the desired end state ("no blob at
    /// that key") already holds.
    fn delete(&self, key: &str) -> io::Result<()>;

    /// The URL a client uses to fetch the blob (consumed by the gallery/map
    /// serving that lands with US-7); for `LocalDisk` this is a path under the
    /// served media prefix.
    fn url_for(&self, key: &str) -> String;
}

/// A [`BlobStore`] backed by a directory on local disk. Keys are stored as
/// relative paths beneath `root`; nested keys (e.g. `trips/3/0001-photo.jpg`)
/// create their parent directories on write.
pub struct LocalDisk {
    root: PathBuf,
}

impl LocalDisk {
    /// A store rooted at `root`. The directory is created lazily on first `put`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Where `key` lives — only ever strictly under `root`. `Path::join`
    /// would follow `..` out of it, and an absolute key would replace it
    /// outright, so any other key is refused rather than resolved.
    fn path_for(&self, key: &str) -> io::Result<PathBuf> {
        if is_safe_key(key) {
            Ok(self.root.join(key))
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("not a blob key: {key:?}"),
            ))
        }
    }
}

/// Whether `key` names a path strictly inside a store's root: `/`-separated
/// plain names, none of them empty, `.` or `..`. Keys arrive from request
/// paths as well as from the database, so this is checked on every access.
pub fn is_safe_key(key: &str) -> bool {
    key.split('/')
        .all(|segment| !matches!(segment, "" | "." | ".."))
}

impl BlobStore for LocalDisk {
    fn put(&self, key: &str, bytes: &[u8]) -> io::Result<()> {
        let path = self.path_for(key)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)
    }

    fn get(&self, key: &str) -> io::Result<Vec<u8>> {
        std::fs::read(self.path_for(key)?)
    }

    fn delete(&self, key: &str) -> io::Result<()> {
        match std::fs::remove_file(self.path_for(key)?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn url_for(&self, key: &str) -> String {
        // Keys are relative; `Path::join` keeps the separators portable, and the
        // `/media` prefix matches where the LocalDisk blobs are served.
        Path::new("/media").join(key).to_string_lossy().into_owned()
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
//
// The BlobStore is an internal collaborator, so it is exercised for real via
// `LocalDisk` pointed at a `tempdir` rather than mocked.

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn local_disk() -> (LocalDisk, tempfile::TempDir) {
        let dir = tempdir().expect("temp dir");
        (LocalDisk::new(dir.path().join("blobs")), dir)
    }

    #[test]
    fn put_then_get_round_trips_the_bytes() {
        let (store, _dir) = local_disk();
        store.put("trips/1/0000-a.jpg", b"the-bytes").unwrap();
        assert_eq!(store.get("trips/1/0000-a.jpg").unwrap(), b"the-bytes");
    }

    #[test]
    fn put_creates_nested_parent_directories() {
        let (store, _dir) = local_disk();
        // Deeply nested key must not fail for a missing parent directory.
        store.put("trips/42/photos/0007-x.png", b"x").unwrap();
        assert_eq!(store.get("trips/42/photos/0007-x.png").unwrap(), b"x");
    }

    #[test]
    fn get_missing_key_is_an_error() {
        let (store, _dir) = local_disk();
        assert!(store.get("does/not/exist").is_err());
    }

    #[test]
    fn url_for_places_the_key_under_the_media_prefix() {
        let (store, _dir) = local_disk();
        assert_eq!(
            store.url_for("trips/1/0000-a.jpg"),
            "/media/trips/1/0000-a.jpg"
        );
    }

    #[test]
    fn a_key_that_leaves_the_root_names_no_blob() {
        let (store, dir) = local_disk();
        store.put("trips/1/0000-a.jpg", b"inside").unwrap();
        std::fs::write(dir.path().join("secret"), b"outside").unwrap();
        let secret = dir.path().join("secret").display().to_string();

        for key in ["../secret", "trips/../../secret", secret.as_str(), "", "."] {
            let err = store.get(key).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{key:?}");
        }
        assert!(store.put("../escaped", b"x").is_err());
        assert!(!dir.path().join("escaped").exists());
        assert!(store.delete("../secret").is_err());
        assert!(dir.path().join("secret").exists());
    }

    #[test]
    fn only_plain_relative_segments_make_a_safe_key() {
        assert!(is_safe_key("trips/1/0000-a.jpg"));
        assert!(is_safe_key("trips/1/thumbs/0000-a..jpg"));
        for bad in ["", "/etc/passwd", "a//b", "a/", "./a", "a/../b", "..", "."] {
            assert!(!is_safe_key(bad), "{bad:?}");
        }
    }

    // ── US-9: delete a trip (and its files) ──────────────────────────────

    #[test]
    fn us9_delete_removes_the_blob_so_a_later_get_fails() {
        let (store, _dir) = local_disk();
        store.put("trips/1/0000-a.jpg", b"the-bytes").unwrap();
        store.delete("trips/1/0000-a.jpg").unwrap();
        assert!(store.get("trips/1/0000-a.jpg").is_err());
    }

    #[test]
    fn us9_delete_of_a_missing_key_is_not_an_error() {
        let (store, _dir) = local_disk();
        assert!(store.delete("does/not/exist").is_ok());
    }
}
