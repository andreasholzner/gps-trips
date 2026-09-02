use serde::{Deserialize, Serialize};

/// The `POST /api/session` body (US-19): the one shared password, as typed.
///
/// There is no username field and no account to name — [ADR-0010] ships no
/// user accounts, and its 2026-09-02 amendment changed only the mechanism
/// under the single secret, not the secret's singularity.
///
/// [ADR-0010]: ../../../docs/adr/0010-single-user-optional-auth.md
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Login {
    pub password: String,
}

/// What `POST /api/session` answers with (US-19): the session, described.
///
/// The token also travels back as an `HttpOnly` cookie, which is what the
/// browser attaches to the plain URL loads a header cannot reach — the photo
/// `<img src>` and the GPX `<a href>`. It is repeated here in the body for
/// the client that has no cookie store to put it in: the Android app's
/// native `reqwest` (US-16), and the host-target screen tests that stand in
/// for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub token: String,
    /// When the session stops being accepted, as RFC-3339 UTC (ADR-0009).
    /// Advisory: the token carries its own expiry and the server re-reads it
    /// on every request, so a client that ignores this field is wrong about
    /// nothing except when to log in again.
    pub expires_at: String,
}

/// Who the archive takes a request to be from — the amendment's *principal*,
/// resolved by the gate on every request and put into the request's
/// extensions, rather than a yes/no.
///
/// A closed set of strings on the wire, so an enum (ADR-0018). Two variants
/// today; sharing (US-53) adds a third — a capability link scoped to named
/// trips — which is exactly why the gate resolves this instead of a boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Principal {
    Owner,
    Anonymous,
}

/// What `GET /api/session` answers with (US-19): who the caller is, for a
/// client deciding between its login screen and the archive.
///
/// Not allowlisted, deliberately — the gate answers `401` for an anonymous
/// caller, so "am I signed in?" is one request whichever the answer is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    pub principal: Principal,
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_login_is_just_the_password() {
        let login: Login = serde_json::from_str(r#"{"password":"hunter2"}"#).unwrap();
        assert_eq!(login.password, "hunter2");
    }

    #[test]
    fn a_session_carries_its_token_and_expiry() {
        let json = serde_json::to_string(&Session {
            token: "42.abc".to_string(),
            expires_at: "2026-12-01T10:00:00Z".to_string(),
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"token":"42.abc","expires_at":"2026-12-01T10:00:00Z"}"#
        );
    }

    #[test]
    fn a_principal_is_a_snake_case_string_on_the_wire() {
        let json = serde_json::to_string(&Identity {
            principal: Principal::Owner,
        })
        .unwrap();
        assert_eq!(json, r#"{"principal":"owner"}"#);

        let parsed: Identity = serde_json::from_str(r#"{"principal":"anonymous"}"#).unwrap();
        assert_eq!(parsed.principal, Principal::Anonymous);
    }
}
