//! The laptop commands' way into a deployed archive (US-40, US-51): signing
//! in with the shared password, then authenticated requests. `backup` and
//! `qmapshack_export` share it, so both reach the archive — and get the
//! password — the same way.

pub mod config;

use reqwest::{StatusCode, Url};

use crate::models::{Login, Session};

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
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
}

/// A signed-in session with one archive. No `Debug`: it carries the token.
pub struct ArchiveClient {
    http: reqwest::Client,
    base: Url,
    token: String,
}

impl ArchiveClient {
    /// Sign in to the archive at `url` (its base URL, e.g. `https://<app>.fly.dev`).
    pub async fn sign_in(url: &str, password: &str) -> Result<Self, ClientError> {
        let base = Url::parse(url).map_err(|_| ClientError::Url(url.to_owned()))?;
        let http = reqwest::Client::new();
        let session_url = endpoint(&base, &["api", "session"])?;
        let response = http
            .post(session_url.clone())
            .json(&Login {
                password: password.to_owned(),
            })
            .send()
            .await?;
        let token = match response.status() {
            StatusCode::UNAUTHORIZED => return Err(ClientError::Refused),
            StatusCode::TOO_MANY_REQUESTS => return Err(ClientError::LockedOut),
            status if !status.is_success() => {
                return Err(ClientError::Status {
                    url: session_url,
                    status,
                })
            }
            _ => response.json::<Session>().await?.token,
        };
        Ok(Self { http, base, token })
    }

    /// GET the path `segments` below the base URL, whatever it answers.
    pub async fn get(&self, segments: &[&str]) -> Result<reqwest::Response, ClientError> {
        let url = endpoint(&self.base, segments)?;
        Ok(self.http.get(url).bearer_auth(&self.token).send().await?)
    }

    /// GET the path `segments` below the base URL; anything but a success is
    /// an error.
    pub async fn get_ok(&self, segments: &[&str]) -> Result<reqwest::Response, ClientError> {
        checked(self.get(segments).await?)
    }
}

/// `response` if it is a success, the status it answered otherwise.
pub fn checked(response: reqwest::Response) -> Result<reqwest::Response, ClientError> {
    let status = response.status();
    if status.is_success() {
        Ok(response)
    } else {
        Err(ClientError::Status {
            url: response.url().clone(),
            status,
        })
    }
}

/// The password from `command` if there is one, else asked for, unechoed.
pub fn read_password(command: Option<&str>) -> std::io::Result<String> {
    match command {
        Some(command) => password_from_command(command),
        None => rpassword::prompt_password("Archive password: "),
    }
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

/// `base` with `segments` appended as path segments, each percent-encoded.
fn endpoint(base: &Url, segments: &[&str]) -> Result<Url, ClientError> {
    let mut url = base.clone();
    url.path_segments_mut()
        .map_err(|_| ClientError::Url(base.to_string()))?
        .pop_if_empty()
        .extend(segments);
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

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
