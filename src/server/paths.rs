//! Where a self-hosted deployment lives (US-10, US-45).
//!
//! Three independent concerns:
//! - the **data directory** (DB + photo blobs, ADR-0002): explicit config only, defaulting
//!   to `./data` for the `cargo run` dev workflow.
//! - the **assets directory** (the SPA's built bundle, ADR-0024 — all that is left of
//!   `public/` since US-44 retired the last server-rendered page): resolved relative
//!   to the running binary, not the process's current working directory, so the deployable
//!   unit is "binary + adjacent `public/` folder" that can be started from anywhere
//!   (ADR-0016).
//! - the **listen address**: loopback unless configured, so the same binary serves
//!   `localhost` on a laptop and every interface inside a container (ADR-0023).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use crate::config::server::{BIND_ADDR_ENV_VAR, DEFAULT_BIND_ADDR};
use crate::config::storage::{ASSETS_DIR_ENV_VAR, DATA_DIR_ENV_VAR, DEFAULT_DATA_DIR};

/// Where the SQLite DB and photo blobs live. `TRIP_ARCHIVE_DATA_DIR`, or `./data`.
pub fn data_dir() -> PathBuf {
    std::env::var(DATA_DIR_ENV_VAR)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_DATA_DIR))
}

/// Where the Dioxus SPA's built bundle lives (US-41, ADR-0024): an `app/`
/// folder inside the assets directory, so it ships with the rest of
/// `public/` and needs no configuration of its own.
pub fn spa_dir() -> PathBuf {
    assets_dir().join("app")
}

/// Where the static assets (`public/`) live. Its only content is the SPA
/// bundle [`spa_dir`] points into, but the two stay separate: ADR-0016's
/// resolution rule is about the folder, not about what happens to be in it.
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

/// The address the server listens on (US-45). `TRIP_ARCHIVE_BIND_ADDR`, or
/// loopback when it is unset or empty.
pub fn bind_addr() -> Result<SocketAddr, InvalidBindAddr> {
    resolve_bind_addr(std::env::var(BIND_ADDR_ENV_VAR).ok())
}

/// A configured listen address that is not an `IP:port`.
#[derive(Debug, thiserror::Error)]
#[error("{BIND_ADDR_ENV_VAR} must be an IP:port such as 0.0.0.0:3000, got {0:?}")]
pub struct InvalidBindAddr(String);

/// Pure resolution logic. The default is loopback, so a laptop run is never
/// reachable from the network by accident; a container opts in explicitly.
/// Something set but unparseable refuses the boot rather than falling back:
/// a typo would otherwise leave a deployed server unreachable, or a local one
/// listening somewhere nobody meant.
fn resolve_bind_addr(configured: Option<String>) -> Result<SocketAddr, InvalidBindAddr> {
    match configured.filter(|value| !value.is_empty()) {
        None => Ok(SocketAddr::from(DEFAULT_BIND_ADDR)),
        Some(value) => value.parse().map_err(|_| InvalidBindAddr(value)),
    }
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

    #[test]
    fn us45_listens_on_loopback_when_no_address_is_configured() {
        let loopback: SocketAddr = "127.0.0.1:3000".parse().unwrap();
        assert_eq!(resolve_bind_addr(None).unwrap(), loopback);
        assert_eq!(resolve_bind_addr(Some(String::new())).unwrap(), loopback);
    }

    #[test]
    fn us45_a_configured_address_wins() {
        let got = resolve_bind_addr(Some("0.0.0.0:3000".into())).unwrap();
        assert_eq!(got, "0.0.0.0:3000".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn us45_an_unparseable_address_refuses_the_boot() {
        for bad in ["0.0.0.0", "localhost:3000", "0.0.0.0:http", " "] {
            let err = resolve_bind_addr(Some(bad.into())).unwrap_err();
            assert!(
                err.to_string().contains(BIND_ADDR_ENV_VAR),
                "{bad:?}: {err}"
            );
        }
    }
}
