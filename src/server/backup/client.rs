//! US-40: the laptop's half of the backup — pull the deployed archive into a
//! directory laid out like a data directory (`trip-archive.db` + `photos/`),
//! for the borg jobs to archive and for a restore to run on.
//!
//! The order of the steps is what keeps a failed run harmless:
//!
//! 1. the snapshot lands beside the old database under a temporary name;
//! 2. the photos it names that the directory lacks are fetched, each written
//!    under a temporary name and renamed, so a half-written photo never
//!    counts as held;
//! 3. the database is swapped in with a rename;
//! 4. only then are photos the snapshot no longer names removed.
//!
//! A photo the server no longer holds is reported rather than failing the
//! run: the rest of the archive still gets its backup.
//!
//! Until step 3 the previous backup is untouched but for new photo files; from
//! step 3 on the directory holds the new one. Photos never change once stored
//! (trip ids are never reused, photos are never replaced), so one already held
//! is never fetched again.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use reqwest::{StatusCode, Url};
use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions, Connection};
use tokio::io::AsyncWriteExt;

use crate::config::storage::{BLOBS_SUBDIR, DB_FILENAME};
use crate::models::{Login, Session};

/// What one run needs. No `Debug`: it carries the password.
pub struct Options {
    /// The archive's base URL, e.g. `https://<app>.fly.dev`.
    pub url: String,
    pub password: String,
    /// The backup directory. It must exist: it is an external disk that is
    /// only there while mounted.
    pub target: PathBuf,
}

/// What a run did, for the command to report.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Photo files fetched because the directory did not hold them yet.
    pub fetched: usize,
    /// Photo files the directory already held.
    pub kept: usize,
    /// Files under `photos/` the snapshot no longer names.
    pub removed: usize,
    /// Keys the snapshot names that the server answered 404 for — damage
    /// on the server, for the command to report as errors.
    pub missing: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("{0} does not exist — is the backup disk mounted?")]
    NoTarget(PathBuf),
    #[error("the archive refused the password")]
    Refused,
    #[error("the archive is refusing sign-ins after too many failed attempts; try again later")]
    LockedOut,
    #[error("{url} answered {status}")]
    Status { url: Url, status: StatusCode },
    #[error("{0} is not a valid archive URL")]
    Url(String),
    #[error("could not reach the archive: {0}")]
    Request(#[from] reqwest::Error),
    #[error("the snapshot names a photo outside the photo directory: {0:?}")]
    UnsafeKey(String),
    #[error("could not read the snapshot: {0}")]
    Snapshot(#[from] sqlx::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Pull the archive at `options.url` into `options.target`.
pub async fn run(options: &Options) -> Result<Report, BackupError> {
    if !options.target.is_dir() {
        return Err(BackupError::NoTarget(options.target.clone()));
    }
    let base = Url::parse(&options.url).map_err(|_| BackupError::Url(options.url.clone()))?;
    let http = reqwest::Client::new();
    let token = sign_in(&http, &base, &options.password).await?;

    let partial = Partial(options.target.join(format!(".{DB_FILENAME}.partial")));
    download_snapshot(&http, &base, &token, &partial.0).await?;
    let keys = photo_keys(&partial.0).await?;

    let photos = options.target.join(BLOBS_SUBDIR);
    let mut report = Report::default();
    for key in &keys {
        let path = photos.join(key);
        if path.is_file() {
            report.kept += 1;
        } else {
            match fetch_photo(&http, &base, &token, key, &path).await? {
                Fetched::Stored => report.fetched += 1,
                Fetched::NotFound => report.missing.push(key.clone()),
            }
        }
    }

    swap_in(&partial, &options.target.join(DB_FILENAME))?;
    report.removed = remove_unnamed(&photos, &keys)?;
    Ok(report)
}

/// The password from the output of `command`, run by `sh` — for a password
/// manager such as `kwallet-query`. Exactly one trailing newline is dropped,
/// the one such tools end their output with; anything else is the password.
pub fn password_from_command(command: &str) -> std::io::Result<String> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .stderr(std::process::Stdio::inherit())
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "the password command failed ({})",
            output.status
        )));
    }
    let mut password = String::from_utf8(output.stdout)
        .map_err(|_| std::io::Error::other("the password command printed no text"))?;
    if password.ends_with('\n') {
        password.pop();
        if password.ends_with('\r') {
            password.pop();
        }
    }
    Ok(password)
}

/// A file that is deleted unless it has been moved into place.
struct Partial(PathBuf);

impl Drop for Partial {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn sign_in(
    http: &reqwest::Client,
    base: &Url,
    password: &str,
) -> Result<String, BackupError> {
    let url = endpoint(base, &["api", "session"])?;
    let response = http
        .post(url.clone())
        .json(&Login {
            password: password.to_owned(),
        })
        .send()
        .await?;
    match response.status() {
        StatusCode::UNAUTHORIZED => Err(BackupError::Refused),
        StatusCode::TOO_MANY_REQUESTS => Err(BackupError::LockedOut),
        status if !status.is_success() => Err(BackupError::Status { url, status }),
        _ => Ok(response.json::<Session>().await?.token),
    }
}

async fn download_snapshot(
    http: &reqwest::Client,
    base: &Url,
    token: &str,
    to: &Path,
) -> Result<(), BackupError> {
    let url = endpoint(base, &["api", "backup", "database"])?;
    let mut response = checked(http.get(url.clone()).bearer_auth(token).send().await?, url)?;
    let mut file = tokio::fs::File::create(to).await?;
    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?;
    }
    file.sync_all().await?;
    Ok(())
}

/// Every photo blob the snapshot names: originals and thumbnails. Opened as
/// immutable, so reading it leaves no `-wal`/`-shm` files beside it.
async fn photo_keys(snapshot: &Path) -> Result<Vec<String>, BackupError> {
    let mut conn = SqliteConnectOptions::new()
        .filename(snapshot)
        .immutable(true)
        .connect()
        .await?;
    let keys: Vec<String> = sqlx::query_scalar(
        "SELECT blob_key FROM photo \
         UNION SELECT thumbnail_key FROM photo WHERE thumbnail_key IS NOT NULL \
         ORDER BY 1",
    )
    .fetch_all(&mut conn)
    .await?;
    conn.close().await?;
    for key in &keys {
        validate_key(key)?;
    }
    Ok(keys)
}

/// A key becomes both a path under the backup directory and URL segments, so
/// it may only name a path strictly inside it: `/`-separated plain names, none
/// of them empty, `.` or `..`.
fn validate_key(key: &str) -> Result<(), BackupError> {
    let plain_names = key
        .split('/')
        .all(|segment| !matches!(segment, "" | "." | ".."));
    if plain_names {
        Ok(())
    } else {
        Err(BackupError::UnsafeKey(key.to_owned()))
    }
}

/// Whether a photo arrived, or the server does not hold it.
enum Fetched {
    Stored,
    NotFound,
}

async fn fetch_photo(
    http: &reqwest::Client,
    base: &Url,
    token: &str,
    key: &str,
    to: &Path,
) -> Result<Fetched, BackupError> {
    let segments: Vec<&str> = std::iter::once("media").chain(key.split('/')).collect();
    let url = endpoint(base, &segments)?;
    let response = http.get(url.clone()).bearer_auth(token).send().await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(Fetched::NotFound);
    }
    let response = checked(response, url)?;
    let bytes = response.bytes().await?;
    if let Some(parent) = to.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let partial = Partial(partial_name(to));
    tokio::fs::write(&partial.0, &bytes).await?;
    std::fs::rename(&partial.0, to)?;
    Ok(Fetched::Stored)
}

/// Replace the backup's database with the snapshot. A WAL or shared-memory
/// file beside the old database (left by a restore rehearsal run on the
/// backup) goes first: SQLite would replay it into the new file.
fn swap_in(snapshot: &Partial, database: &Path) -> std::io::Result<()> {
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = database.as_os_str().to_owned();
        sidecar.push(suffix);
        match std::fs::remove_file(sidecar) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    std::fs::rename(&snapshot.0, database)
}

/// Remove every file under `photos` that `keys` does not name — photos of
/// deleted trips, and temporary files an interrupted run left — then the
/// directories that leaves empty. Returns how many files went.
fn remove_unnamed(photos: &Path, keys: &[String]) -> std::io::Result<usize> {
    let named: HashSet<PathBuf> = keys.iter().map(|key| photos.join(key)).collect();
    fn sweep(dir: &Path, named: &HashSet<PathBuf>, removed: &mut usize) -> std::io::Result<bool> {
        let mut empty = true;
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                if sweep(&path, named, removed)? {
                    std::fs::remove_dir(&path)?;
                } else {
                    empty = false;
                }
            } else if named.contains(&path) {
                empty = false;
            } else {
                std::fs::remove_file(&path)?;
                *removed += 1;
            }
        }
        Ok(empty)
    }
    let mut removed = 0;
    if photos.is_dir() {
        sweep(photos, &named, &mut removed)?;
    }
    Ok(removed)
}

fn partial_name(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".partial");
    PathBuf::from(name)
}

/// `base` with `segments` appended as path segments, each percent-encoded.
fn endpoint(base: &Url, segments: &[&str]) -> Result<Url, BackupError> {
    let mut url = base.clone();
    url.path_segments_mut()
        .map_err(|_| BackupError::Url(base.to_string()))?
        .pop_if_empty()
        .extend(segments);
    Ok(url)
}

fn checked(response: reqwest::Response, url: Url) -> Result<reqwest::Response, BackupError> {
    let status = response.status();
    if status.is_success() {
        Ok(response)
    } else {
        Err(BackupError::Status { url, status })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us40_a_key_may_only_descend_into_the_photo_directory() {
        assert!(validate_key("trips/1/0000-a.jpg").is_ok());
        assert!(validate_key("trips/1/thumbs/0000-a.jpg").is_ok());
        for bad in [
            "",
            "/etc/passwd",
            "../outside.jpg",
            "trips/../../x",
            "trips/./a",
        ] {
            assert!(
                matches!(validate_key(bad), Err(BackupError::UnsafeKey(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn us40_the_password_command_keeps_everything_but_its_last_newline() {
        let from = |cmd| password_from_command(cmd).unwrap();
        assert_eq!(from("printf 'a#b c'"), "a#b c");
        assert_eq!(from("printf ' spaced \\n'"), " spaced ");
        assert_eq!(from("printf 'crlf\\r\\n'"), "crlf");
        assert_eq!(from("printf 'two\\n\\n'"), "two\n");
    }

    #[test]
    fn us40_a_failing_password_command_is_an_error() {
        assert!(password_from_command("exit 3").is_err());
    }

    #[test]
    fn us40_endpoints_keep_the_base_path_and_encode_segments() {
        let base = Url::parse("https://example.test/").unwrap();
        assert_eq!(
            endpoint(&base, &["media", "trips", "1", "0000-a b.jpg"])
                .unwrap()
                .as_str(),
            "https://example.test/media/trips/1/0000-a%20b.jpg"
        );
    }
}
