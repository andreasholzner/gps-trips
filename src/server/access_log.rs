//! US-70 — the access log: one line per request on stdout, saying who made
//! it, so the owner can see how the archive is used and tell a share's
//! requests from their own.
//!
//! Layered outside the gate, which names the caller on every response
//! ([`Caller`]); this records that answer rather than deciding again. What
//! never reaches a line: a share's token (the link's whole credential —
//! blanked out of the path), the session cookie, the `Authorization` header,
//! any request body, and the query string.

use std::net::SocketAddr;
use std::time::Instant;

use axum::{
    extract::{ConnectInfo, Request},
    http::{header, HeaderMap},
    middleware::Next,
    response::Response,
};

use crate::config;
use crate::server::auth::Caller;

/// The log target, so `RUST_LOG` can raise or silence the access log on its
/// own: `trip_archive::access=debug` adds the bundle's files.
pub const TARGET: &str = "trip_archive::access";

/// The header Fly's proxy puts the client's address in (ADR-0023). Nothing
/// else is trusted for it — `X-Forwarded-For` is whatever a client says.
const CLIENT_IP_HEADER: &str = "fly-client-ip";

/// Where `dx` puts the bundle's content-hashed files. Requests for them say
/// nothing about how the archive is used, and are most of a page load's.
const BUNDLE_ASSETS_PREFIX: &str = "/app/assets/";

/// The SPA's own route for a share's screens (US-53): `/app/s/<token>…`.
const SHARE_PAGE_PREFIX: &str = "/app/s/";

/// What the log says about one request.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessRecord {
    pub method: String,
    /// The path with any share token blanked out ([`redact`]).
    pub path: String,
    pub status: u16,
    pub duration_ms: u64,
    pub caller: Caller,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

impl AccessRecord {
    /// Whether this is one of the bundle's content-hashed files.
    pub fn is_bundle_file(&self) -> bool {
        self.path.starts_with(BUNDLE_ASSETS_PREFIX)
    }

    /// The line on stdout. The user agent and the label are quoted with
    /// `Debug`, which escapes quotes and line breaks: a user agent is
    /// whatever a client sends, and must not be able to forge a line.
    pub fn line(&self) -> String {
        let who = match &self.caller {
            Caller::Owner => "who=owner".to_string(),
            Caller::Anonymous => "who=anonymous".to_string(),
            Caller::UnknownLink => "who=unknown-link".to_string(),
            Caller::Share { id, label: None } => format!("who=share share={id}"),
            Caller::Share {
                id,
                label: Some(label),
            } => format!("who=share share={id} label={label:?}"),
        };
        format!(
            "{} {} {} {}ms {who} ip={} agent={:?}",
            self.method,
            self.path,
            self.status,
            self.duration_ms,
            self.ip.as_deref().unwrap_or("-"),
            self.user_agent.as_deref().unwrap_or("-"),
        )
    }
}

/// The middleware: time the request, then log it with the caller the gate
/// named. A response the gate never saw — none today — is logged as
/// anonymous rather than not at all.
pub async fn log(request: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = request.method().to_string();
    let path = redact(request.uri().path());
    let ip = client_ip(request.headers(), request.extensions().get());
    let user_agent = request
        .headers()
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    let response = next.run(request).await;

    let record = AccessRecord {
        method,
        path,
        status: response.status().as_u16(),
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        caller: response
            .extensions()
            .get::<Caller>()
            .cloned()
            .unwrap_or(Caller::Anonymous),
        ip,
        user_agent,
    };
    if record.is_bundle_file() {
        tracing::debug!(target: TARGET, "{}", record.line());
    } else {
        tracing::info!(target: TARGET, "{}", record.line());
    }
    response
}

/// The client's address: the one Fly's proxy saw, else the connection's own
/// peer (a laptop run, where there is no proxy).
fn client_ip(headers: &HeaderMap, peer: Option<&ConnectInfo<SocketAddr>>) -> Option<String> {
    headers
        .get(CLIENT_IP_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .or_else(|| peer.map(|ConnectInfo(addr)| addr.ip().to_string()))
}

/// `path` with a share's token blanked out, wherever one sits: the data
/// routes' `/s/<token>/…` and the screens' `/app/s/<token>…`.
pub fn redact(path: &str) -> String {
    for prefix in [config::share::PATH_PREFIX, SHARE_PAGE_PREFIX] {
        if let Some(rest) = path.strip_prefix(prefix) {
            let after_token = rest.find('/').map_or("", |i| &rest[i..]);
            if rest.len() > after_token.len() {
                return format!("{prefix}…{after_token}");
            }
        }
    }
    path.to_string()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us70_a_token_is_blanked_out_wherever_it_sits() {
        let token = "ab".repeat(32);
        for (path, redacted) in [
            (format!("/s/{token}/api/share"), "/s/…/api/share"),
            (
                format!("/s/{token}/media/trips/1/a.jpg"),
                "/s/…/media/trips/1/a.jpg",
            ),
            (format!("/s/{token}"), "/s/…"),
            (format!("/app/s/{token}"), "/app/s/…"),
            (format!("/app/s/{token}/trips/3"), "/app/s/…/trips/3"),
            // Whatever the segment holds, encoded or not, is the token.
            ("/s/a%2Fb%20c/api/share".to_string(), "/s/…/api/share"),
        ] {
            assert_eq!(redact(&path), redacted, "{path}");
        }
    }

    #[test]
    fn us70_a_path_without_a_token_is_logged_as_it_is() {
        for path in [
            "/api/trips",
            "/app/",
            "/app/trips/3",
            "/s/",
            "/app/s/",
            "/",
            "/sx/abc",
        ] {
            assert_eq!(redact(path), path);
        }
    }

    fn record(caller: Caller) -> AccessRecord {
        AccessRecord {
            method: "GET".to_string(),
            path: "/api/trips".to_string(),
            status: 200,
            duration_ms: 12,
            caller,
            ip: Some("203.0.113.9".to_string()),
            user_agent: Some("Firefox".to_string()),
        }
    }

    #[test]
    fn us70_the_line_says_who_made_the_request() {
        assert_eq!(
            record(Caller::Owner).line(),
            r#"GET /api/trips 200 12ms who=owner ip=203.0.113.9 agent="Firefox""#
        );
        assert!(record(Caller::Share {
            id: 7,
            label: Some("For Kari".to_string())
        })
        .line()
        .contains(r#"who=share share=7 label="For Kari" ip="#));
        assert!(record(Caller::Share { id: 7, label: None })
            .line()
            .contains("who=share share=7 ip="));
        assert!(record(Caller::UnknownLink)
            .line()
            .contains("who=unknown-link"));
    }

    #[test]
    fn us70_a_user_agent_cannot_forge_a_line() {
        let forged = AccessRecord {
            user_agent: Some("x\"\nGET /api/trips 200 1ms who=owner".to_string()),
            ip: None,
            ..record(Caller::Anonymous)
        };
        let line = forged.line();
        assert!(!line.contains('\n'), "{line}");
        assert!(
            line.ends_with(r#"agent="x\"\nGET /api/trips 200 1ms who=owner""#),
            "{line}"
        );
        assert!(line.contains("ip=- "), "{line}");
    }

    #[test]
    fn us70_only_the_bundles_hashed_files_count_as_bundle_files() {
        let with_path = |path: &str| AccessRecord {
            path: path.to_string(),
            ..record(Caller::Anonymous)
        };
        assert!(with_path("/app/assets/ui-dioxus-dxh01.js").is_bundle_file());
        assert!(!with_path("/app/").is_bundle_file());
        assert!(!with_path("/app/manifest.webmanifest").is_bundle_file());
    }
}
