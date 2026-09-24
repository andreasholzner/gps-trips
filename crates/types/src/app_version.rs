use serde::{Deserialize, Serialize};

/// The version this build was made from (US-68): the deployed commit's date
/// and short hash, e.g. `2026-09-24 · e45c25a`, which `scripts/deploy.sh`
/// passes in as `TRIP_ARCHIVE_VERSION`. A build without it — a local run, CI
/// — is `dev`.
///
/// Defined here so the server and the SPA each carry the value they were
/// built with from one definition; comparing the two is how the SPA tells
/// that the server has been deployed since it loaded.
pub const VERSION: &str = match option_env!("TRIP_ARCHIVE_VERSION") {
    Some(version) => version,
    None => "dev",
};

/// What `GET /api/version` answers with (US-68): the server's [`VERSION`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppVersion {
    pub version: String,
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_one_field() {
        let json = serde_json::to_string(&AppVersion {
            version: "2026-09-24 · e45c25a".into(),
        })
        .unwrap();
        assert_eq!(json, r#"{"version":"2026-09-24 · e45c25a"}"#);
    }
}
