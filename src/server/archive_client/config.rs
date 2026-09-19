//! What the laptop commands' config files share (US-40, US-51): the archive's
//! URL and an optional password command, validated the same way, and one
//! place to find each command's file. Every command keeps a file of its own.

use std::ffi::OsString;
use std::path::PathBuf;

use reqwest::Url;

/// The directory under the user's config directory holding the files.
const CONFIG_SUBDIR: &str = "trip-archive";

#[derive(Debug, thiserror::Error)]
pub enum ArchiveConfigError {
    #[error("url {0:?} is not an https URL")]
    BadUrl(String),
    #[error("password_command must not be empty; leave it out to be asked instead")]
    EmptyPasswordCommand,
    #[error("neither XDG_CONFIG_HOME nor HOME is set, so there is no default config file; pass --config")]
    NoConfigDir,
}

/// Check the archive half of a config: an https `url` — the password is sent
/// to it — and, when present, a non-blank `password_command`.
pub fn validate(url: &str, password_command: Option<&str>) -> Result<(), ArchiveConfigError> {
    let is_https = Url::parse(url).is_ok_and(|url| url.scheme() == "https" && url.has_host());
    if !is_https {
        return Err(ArchiveConfigError::BadUrl(url.to_owned()));
    }
    if password_command.is_some_and(|command| command.trim().is_empty()) {
        return Err(ArchiveConfigError::EmptyPasswordCommand);
    }
    Ok(())
}

/// `$XDG_CONFIG_HOME/trip-archive/<file_name>`, or `~/.config/…` when that is unset.
pub fn default_path(file_name: &str) -> Result<PathBuf, ArchiveConfigError> {
    resolve_default_path(
        file_name,
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

fn resolve_default_path(
    file_name: &str,
    xdg_config_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf, ArchiveConfigError> {
    // An empty XDG_CONFIG_HOME counts as unset, as the XDG spec says.
    let config_dir = match (xdg_config_home.filter(|dir| !dir.is_empty()), home) {
        (Some(dir), _) => PathBuf::from(dir),
        (None, Some(home)) if !home.is_empty() => PathBuf::from(home).join(".config"),
        _ => return Err(ArchiveConfigError::NoConfigDir),
    };
    Ok(config_dir.join(CONFIG_SUBDIR).join(file_name))
}

#[cfg(test)]
mod tests {
    use super::*;

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
            assert!(
                matches!(validate(url, None), Err(ArchiveConfigError::BadUrl(_))),
                "{url:?}"
            );
        }
        assert!(validate("https://example.fly.dev", None).is_ok());
    }

    #[test]
    fn us40_an_empty_password_command_is_an_error() {
        assert!(matches!(
            validate("https://example.fly.dev", Some("  ")),
            Err(ArchiveConfigError::EmptyPasswordCommand)
        ));
        assert!(validate("https://example.fly.dev", Some("pass show archive")).is_ok());
    }

    #[test]
    fn us40_the_default_path_follows_xdg_then_home() {
        let path = |xdg: Option<&str>, home: Option<&str>| {
            resolve_default_path("backup.toml", xdg.map(Into::into), home.map(Into::into))
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
        assert!(matches!(
            path(None, None),
            Err(ArchiveConfigError::NoConfigDir)
        ));
    }
}
