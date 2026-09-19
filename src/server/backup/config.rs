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

use reqwest::Url;
use serde::Deserialize;

/// The config file's place under the user's config directory.
const CONFIG_FILE: &str = "trip-archive/backup.toml";

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
    #[error("url {0:?} is not an https URL")]
    BadUrl(String),
    #[error("target {0:?} must be an absolute path")]
    RelativeTarget(PathBuf),
    #[error("password_command must not be empty; leave it out to be asked instead")]
    EmptyPasswordCommand,
    #[error("neither XDG_CONFIG_HOME nor HOME is set, so there is no default config file; pass --config")]
    NoConfigDir,
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
        // https only: the password is sent to it.
        let is_https =
            Url::parse(&config.url).is_ok_and(|url| url.scheme() == "https" && url.has_host());
        if !is_https {
            return Err(ConfigError::BadUrl(config.url));
        }
        // Relative would mean relative to wherever the command happens to be
        // run from — a backup landing somewhere nobody looks.
        if !config.target.is_absolute() {
            return Err(ConfigError::RelativeTarget(config.target));
        }
        if config
            .password_command
            .as_deref()
            .is_some_and(|command| command.trim().is_empty())
        {
            return Err(ConfigError::EmptyPasswordCommand);
        }
        Ok(config)
    }
}

/// `$XDG_CONFIG_HOME/trip-archive/backup.toml`, or `~/.config/…` when that is unset.
pub fn default_path() -> Result<PathBuf, ConfigError> {
    resolve_default_path(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

fn resolve_default_path(
    xdg_config_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Result<PathBuf, ConfigError> {
    // An empty XDG_CONFIG_HOME counts as unset, as the XDG spec says.
    let config_dir = match (xdg_config_home.filter(|dir| !dir.is_empty()), home) {
        (Some(dir), _) => PathBuf::from(dir),
        (None, Some(home)) if !home.is_empty() => PathBuf::from(home).join(".config"),
        _ => return Err(ConfigError::NoConfigDir),
    };
    Ok(config_dir.join(CONFIG_FILE))
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
        // The password travels to it: never in the clear.
        for url in [
            "example.fly.dev",
            "ftp://example.fly.dev",
            "http://example.fly.dev",
            "http://127.0.0.1:3000",
            "https://",
            "",
        ] {
            let text = FULL.replace("https://example.fly.dev", url);
            assert!(
                matches!(
                    BackupConfig::from_toml_str(&text),
                    Err(ConfigError::BadUrl(_))
                ),
                "{url:?}"
            );
        }
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
            Err(ConfigError::EmptyPasswordCommand)
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

    #[test]
    fn us40_the_default_path_follows_xdg_then_home() {
        let path = |xdg: Option<&str>, home: Option<&str>| {
            resolve_default_path(xdg.map(Into::into), home.map(Into::into))
        };
        assert_eq!(
            path(Some("/xdg"), Some("/home/o")).unwrap(),
            PathBuf::from("/xdg/trip-archive/backup.toml")
        );
        assert_eq!(
            path(Some(""), Some("/home/o")).unwrap(),
            PathBuf::from("/home/o/.config/trip-archive/backup.toml")
        );
        assert_eq!(
            path(None, Some("/home/o")).unwrap(),
            PathBuf::from("/home/o/.config/trip-archive/backup.toml")
        );
        assert!(matches!(path(None, None), Err(ConfigError::NoConfigDir)));
    }
}
