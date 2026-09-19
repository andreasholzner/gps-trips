//! The `backup` command's configuration (US-40): what does not change from
//! one run to the next, in one TOML file on the laptop.
//!
//! ```toml
//! url = "https://<app>.fly.dev"
//! target = "/run/media/owner/backup-disk/trip-archive"
//! password_command = "kwallet-query -r trip-archive kdewallet"   # optional
//! ```
//!
//! Without `password_command` the password is asked for on every run. The
//! file holds no secret — the password is never written to it — but it does
//! hold the archive's hostname, which the public repository keeps out of
//! sight. Validated at load (validate at the boundary): a loaded config is
//! one a run can start from.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::server::archive_client::config::{self as archive, ArchiveConfigError};

/// The config file's name in the user's config directory.
const CONFIG_FILE: &str = "backup.toml";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read config file {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("config file {path} is not valid: {source}")]
    Toml {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("target {0:?} must be an absolute path")]
    RelativeTarget(PathBuf),
    #[error(transparent)]
    Archive(#[from] ArchiveConfigError),
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupConfig {
    /// The archive's base URL, https only.
    pub url: String,
    /// The backup directory — on the external disk, so only there when it is mounted.
    pub target: PathBuf,
    /// A command printing the password, run by `sh`; asked for when absent.
    pub password_command: Option<String>,
}

impl BackupConfig {
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_toml_str(&text).map_err(|e| match e {
            ConfigError::Toml { source, .. } => ConfigError::Toml {
                path: path.to_path_buf(),
                source,
            },
            other => other,
        })
    }

    fn from_toml_str(text: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(text).map_err(|source| ConfigError::Toml {
            path: PathBuf::new(),
            source,
        })?;
        archive::validate(&config.url, config.password_command.as_deref())?;
        // Relative would mean relative to wherever the command happens to be
        // run from — a backup landing somewhere nobody looks.
        if !config.target.is_absolute() {
            return Err(ConfigError::RelativeTarget(config.target));
        }
        Ok(config)
    }
}

/// `$XDG_CONFIG_HOME/trip-archive/backup.toml`, or `~/.config/…` when that is unset.
pub fn default_path() -> Result<PathBuf, ConfigError> {
    Ok(archive::default_path(CONFIG_FILE)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"
        url = "https://example.fly.dev"
        target = "/mnt/backup/trip-archive"
        password_command = "kwallet-query -r trip-archive kdewallet"
    "#;

    #[test]
    fn us40_a_full_config_loads() {
        let config = BackupConfig::from_toml_str(FULL).unwrap();
        assert_eq!(
            config,
            BackupConfig {
                url: "https://example.fly.dev".into(),
                target: "/mnt/backup/trip-archive".into(),
                password_command: Some("kwallet-query -r trip-archive kdewallet".into()),
            }
        );
    }

    #[test]
    fn us40_the_password_command_is_optional() {
        let config = BackupConfig::from_toml_str(
            r#"
            url = "https://example.fly.dev"
            target = "/mnt/backup/trip-archive"
            "#,
        )
        .unwrap();
        assert_eq!(config.password_command, None);
    }

    #[test]
    fn us40_a_mistyped_key_is_an_error_not_ignored() {
        let text = FULL.replace("password_command", "pasword_command");
        assert!(matches!(
            BackupConfig::from_toml_str(&text),
            Err(ConfigError::Toml { .. })
        ));
    }

    #[test]
    fn us40_the_url_must_be_https() {
        let text = FULL.replace("https://example.fly.dev", "http://example.fly.dev");
        assert!(matches!(
            BackupConfig::from_toml_str(&text),
            Err(ConfigError::Archive(ArchiveConfigError::BadUrl(_)))
        ));
    }

    #[test]
    fn us40_the_target_must_be_absolute() {
        let text = FULL.replace("/mnt/backup/trip-archive", "backup/trip-archive");
        assert!(matches!(
            BackupConfig::from_toml_str(&text),
            Err(ConfigError::RelativeTarget(_))
        ));
    }

    #[test]
    fn us40_an_empty_password_command_is_an_error() {
        let text = FULL.replace("kwallet-query -r trip-archive kdewallet", "  ");
        assert!(matches!(
            BackupConfig::from_toml_str(&text),
            Err(ConfigError::Archive(
                ArchiveConfigError::EmptyPasswordCommand
            ))
        ));
    }

    #[test]
    fn us40_a_missing_file_names_the_path() {
        let err = BackupConfig::from_file(Path::new("/nonexistent/backup.toml")).unwrap_err();
        assert!(
            err.to_string().contains("/nonexistent/backup.toml"),
            "{err}"
        );
    }
}
