//! Filesystem locations for a self-hosted deployment (US-10).
//!
//! Two independent concerns:
//! - the **data directory** (DB + photo blobs, ADR-0002): explicit config only, defaulting
//!   to `./data` for the `cargo run` dev workflow.
//! - the **assets directory** (vendored map/chart JS/CSS, ADR-0005/0006): resolved relative
//!   to the running binary, not the process's current working directory, so the deployable
//!   unit is "binary + adjacent `public/` folder" that can be started from anywhere
//!   (ADR-0016).

use std::path::{Path, PathBuf};

use crate::config::storage::{ASSETS_DIR_ENV_VAR, DATA_DIR_ENV_VAR, DEFAULT_DATA_DIR};

/// Where the SQLite DB and photo blobs live. `TRIP_ARCHIVE_DATA_DIR`, or `./data`.
pub fn data_dir() -> PathBuf {
    std::env::var(DATA_DIR_ENV_VAR)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_DATA_DIR))
}

/// Where the vendored static assets (`public/`) live.
pub fn assets_dir() -> PathBuf {
    resolve_assets_dir(
        std::env::var(ASSETS_DIR_ENV_VAR).ok(),
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf)),
    )
}

/// Pure resolution logic, unit-tested without touching real env vars or `current_exe`.
///
/// Order: an explicit `TRIP_ARCHIVE_ASSETS_DIR` always wins; otherwise prefer `public/`
/// next to the executable (the real deployment layout); otherwise fall back to a
/// CWD-relative `public` (the `cargo run` dev workflow, where the exe lives under
/// `target/debug/` but `public/` sits at the repo root).
fn resolve_assets_dir(env_override: Option<String>, exe_dir: Option<PathBuf>) -> PathBuf {
    if let Some(dir) = env_override {
        return PathBuf::from(dir);
    }
    if let Some(dir) = exe_dir {
        let candidate = dir.join("public");
        if candidate.is_dir() {
            return candidate;
        }
    }
    PathBuf::from("public")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us10_env_override_wins_regardless_of_exe_dir() {
        let exe_dir = tempfile::tempdir().unwrap();
        let got = resolve_assets_dir(
            Some("/custom/assets".into()),
            Some(exe_dir.path().to_path_buf()),
        );
        assert_eq!(got, PathBuf::from("/custom/assets"));
    }

    #[test]
    fn us10_prefers_exe_relative_public_dir_when_present() {
        let exe_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(exe_dir.path().join("public")).unwrap();
        let got = resolve_assets_dir(None, Some(exe_dir.path().to_path_buf()));
        assert_eq!(got, exe_dir.path().join("public"));
    }

    #[test]
    fn us10_falls_back_to_cwd_relative_public_when_exe_relative_missing() {
        let exe_dir = tempfile::tempdir().unwrap(); // no "public" subdir
        let got = resolve_assets_dir(None, Some(exe_dir.path().to_path_buf()));
        assert_eq!(got, PathBuf::from("public"));
    }

    #[test]
    fn us10_falls_back_to_cwd_relative_public_when_exe_dir_unknown() {
        let got = resolve_assets_dir(None, None);
        assert_eq!(got, PathBuf::from("public"));
    }
}
