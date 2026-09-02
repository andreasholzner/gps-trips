//! US-19 — the gate's own behaviour, below the HTTP layer. The acceptance
//! criteria as the owner meets them (every route refuses an anonymous
//! request; a session survives a redeploy; signing out ends it) are asserted
//! against the real router in `tests/us19_auth.rs`.

use super::*;

const PASSWORD: &str = "correct horse battery staple";

fn an_auth() -> Auth {
    Auth::new(PASSWORD).expect("a non-empty password is accepted")
}

/// A fixed instant to reason from — `now` is a value everywhere in this
/// module (ADR-0012's 2026-07-24 amendment), so no test waits for a clock.
fn a_time() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap()
}

// ── The secret ───────────────────────────────────────────────────────────────

#[test]
fn us19_an_empty_password_is_refused() {
    assert!(
        Auth::new("").is_err(),
        "an empty secret is no secret; the archive must refuse to run with it"
    );
}

#[test]
fn us19_a_whitespace_only_password_is_refused() {
    assert!(Auth::new("   \t ").is_err());
}

#[test]
fn us19_a_password_of_spaces_around_real_characters_is_kept_verbatim() {
    // Trimmed for the emptiness check only: " a " is a password, and it is
    // not the same password as "a".
    let auth = Auth::new(" a ").expect("a padded but non-empty secret is a secret");
    assert!(matches!(
        auth.login(" a ", a_time()),
        LoginOutcome::Granted(_)
    ));
    assert_eq!(auth.login("a", a_time()), LoginOutcome::Refused);
}

/// Env vars are process-global; serialize the tests that touch this one, the
/// way `tests/us10_self_host.rs` does for the assets dir.
static PASSWORD_ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn us19_from_env_refuses_an_unset_password() {
    let _guard = PASSWORD_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var(config::auth::PASSWORD_ENV_VAR);
    assert!(
        Auth::from_env().is_err(),
        "a forgotten environment variable must stop the boot, not open the archive"
    );
}

#[test]
fn us19_from_env_refuses_an_empty_password() {
    let _guard = PASSWORD_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var(config::auth::PASSWORD_ENV_VAR, "");
    let refused = Auth::from_env().is_err();
    std::env::remove_var(config::auth::PASSWORD_ENV_VAR);
    assert!(refused, "an empty value is the same as none");
}

#[test]
fn us19_from_env_accepts_a_real_password() {
    let _guard = PASSWORD_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var(config::auth::PASSWORD_ENV_VAR, PASSWORD);
    let accepted = Auth::from_env().is_ok();
    std::env::remove_var(config::auth::PASSWORD_ENV_VAR);
    assert!(accepted);
}

#[test]
fn us19_the_error_names_the_variable_to_set() {
    // The whole value of this failure is that it says what to do about it.
    assert!(MissingPassword
        .to_string()
        .contains("TRIP_ARCHIVE_PASSWORD"));
}

// ── The token ────────────────────────────────────────────────────────────────

#[test]
fn us19_a_minted_session_verifies_as_the_owner() {
    let auth = an_auth();
    let session = auth.mint(a_time());
    let verified = auth
        .verify(&session.token, a_time())
        .expect("a token just minted must verify");
    assert_eq!(verified.principal, Principal::Owner);
}

#[test]
fn us19_a_session_lasts_the_configured_lifetime() {
    let auth = an_auth();
    let session = auth.mint(a_time());
    let almost = a_time() + config::auth::SESSION_TTL - time::Duration::seconds(1);
    assert!(
        auth.verify(&session.token, almost).is_some(),
        "a session must still be good a second before it expires"
    );
    let after = a_time() + config::auth::SESSION_TTL + time::Duration::seconds(1);
    assert!(
        auth.verify(&session.token, after).is_none(),
        "an expired token must be refused, whatever its signature says"
    );
}

#[test]
fn us19_the_expiry_is_reported_as_rfc3339() {
    // ADR-0009: every timestamp leaving this process is RFC-3339 UTC.
    let session = an_auth().mint(a_time());
    assert!(
        session.expires_at.ends_with('Z') && session.expires_at.contains('T'),
        "got {:?}",
        session.expires_at
    );
}

#[test]
fn us19_a_tampered_expiry_is_refused() {
    let auth = an_auth();
    let session = auth.mint(a_time());
    let (_, signature) = session.token.split_once('.').unwrap();
    // The obvious forgery: keep the signature, push the expiry out.
    let far_future = (a_time() + time::Duration::days(3650)).unix_timestamp();
    assert!(auth
        .verify(&format!("{far_future}.{signature}"), a_time())
        .is_none());
}

#[test]
fn us19_a_tampered_signature_is_refused() {
    let auth = an_auth();
    let session = auth.mint(a_time());
    let (expiry, signature) = session.token.split_once('.').unwrap();
    let flipped: String = signature
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i == 0 && c == 'a' {
                'b'
            } else if i == 0 {
                'a'
            } else {
                c
            }
        })
        .collect();
    assert!(auth
        .verify(&format!("{expiry}.{flipped}"), a_time())
        .is_none());
}

#[test]
fn us19_a_malformed_token_is_refused_rather_than_panicking() {
    let auth = an_auth();
    for token in ["", ".", "nonsense", "abc.def", "17", "17.", ".deadbeef"] {
        assert!(
            auth.verify(token, a_time()).is_none(),
            "{token:?} must be refused"
        );
    }
}

#[test]
fn us19_rotating_the_password_revokes_every_existing_session() {
    // The consequence ADR-0010's amendment accepts by name: the signing key
    // is derived from the password, so changing it is the revocation.
    let session = an_auth().mint(a_time());
    let rotated = Auth::new("a different secret entirely").unwrap();
    assert!(rotated.verify(&session.token, a_time()).is_none());
}

// ── Logging in ───────────────────────────────────────────────────────────────

#[test]
fn us19_the_right_password_opens_a_session() {
    let auth = an_auth();
    let LoginOutcome::Granted(session) = auth.login(PASSWORD, a_time()) else {
        panic!("the shared password must open a session");
    };
    assert!(auth.verify(&session.token, a_time()).is_some());
}

#[test]
fn us19_a_wrong_password_is_refused() {
    assert_eq!(an_auth().login("guess", a_time()), LoginOutcome::Refused);
}

#[test]
fn us19_repeated_failures_lock_logins_out() {
    let auth = an_auth();
    for attempt in 1..config::auth::LOGIN_FAILURE_LIMIT {
        assert_eq!(
            auth.login("guess", a_time()),
            LoginOutcome::Refused,
            "attempt {attempt} is below the limit and should merely fail"
        );
    }
    assert_eq!(auth.login("guess", a_time()), LoginOutcome::Refused);

    // The next attempt meets the lockout the previous one earned — and so
    // does a *correct* password, which is the whole point of a lockout.
    let LoginOutcome::LockedOut(wait) = auth.login(PASSWORD, a_time()) else {
        panic!(
            "logins must be locked out after {} consecutive failures",
            config::auth::LOGIN_FAILURE_LIMIT
        );
    };
    assert_eq!(wait, config::auth::LOGIN_LOCKOUT);
}

#[test]
fn us19_the_lockout_ends_on_its_own() {
    let auth = an_auth();
    for _ in 0..config::auth::LOGIN_FAILURE_LIMIT {
        auth.login("guess", a_time());
    }
    let after = a_time() + config::auth::LOGIN_LOCKOUT + std::time::Duration::from_secs(1);
    assert!(matches!(
        auth.login(PASSWORD, after),
        LoginOutcome::Granted(_)
    ));
}

#[test]
fn us19_a_success_resets_the_failure_count() {
    let auth = an_auth();
    for _ in 0..config::auth::LOGIN_FAILURE_LIMIT - 1 {
        auth.login("guess", a_time());
    }
    assert!(matches!(
        auth.login(PASSWORD, a_time()),
        LoginOutcome::Granted(_)
    ));
    // A fresh run of failures, not the tail of the old one: the owner
    // mistyping now and again must never accumulate into a lockout.
    for _ in 0..config::auth::LOGIN_FAILURE_LIMIT - 1 {
        assert_eq!(auth.login("guess", a_time()), LoginOutcome::Refused);
    }
}

#[test]
fn us19_surviving_a_lockout_takes_a_full_run_of_failures_again() {
    let auth = an_auth();
    for _ in 0..config::auth::LOGIN_FAILURE_LIMIT {
        auth.login("guess", a_time());
    }
    let after = a_time() + config::auth::LOGIN_LOCKOUT + std::time::Duration::from_secs(1);
    assert_eq!(auth.login("guess", after), LoginOutcome::Refused);
    assert_eq!(
        auth.login("guess", after),
        LoginOutcome::Refused,
        "one failure after a lockout must not re-lock on its own"
    );
}

// ── The allowlist ────────────────────────────────────────────────────────────

#[test]
fn us19_the_allowlist_holds_exactly_what_it_must() {
    let public = [
        (Method::POST, "/api/session"),
        (Method::GET, "/app"),
        (Method::GET, "/app/"),
        (Method::GET, "/app/assets/app.css"),
        (Method::GET, "/app/trips/42"),
        (Method::GET, "/"),
        (Method::GET, "/import"),
        (Method::GET, "/komoot/sync"),
        (Method::GET, "/trips/42"),
    ];
    for (method, path) in public {
        assert!(
            is_public(&method, path),
            "{method} {path} must be reachable"
        );
    }
}

#[test]
fn us19_everything_else_is_private() {
    let private = [
        (Method::GET, "/api/trips"),
        (Method::GET, "/api/trips/42"),
        (Method::GET, "/api/session"),
        (Method::DELETE, "/api/session"),
        (Method::GET, "/api/tags"),
        (Method::GET, "/media/trips/1/photo.jpg"),
        (Method::GET, "/api/trips/42/gpx"),
        (Method::POST, "/api/import"),
        (Method::POST, "/api/trips/tags"),
        // A path that merely starts like an allowlisted one.
        (Method::GET, "/apple"),
        (Method::GET, "/trips/42/photos"),
        // The login endpoint is public for POST only.
        (Method::GET, "/api/sessions"),
        (Method::PATCH, "/api/session"),
    ];
    for (method, path) in private {
        assert!(!is_public(&method, path), "{method} {path} must be gated");
    }
}

// ── Where the token is read from ─────────────────────────────────────────────

fn headers(name: header::HeaderName, value: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(name, value.parse().unwrap());
    headers
}

#[test]
fn us19_the_session_cookie_is_found_among_others() {
    let map = headers(
        header::COOKIE,
        &format!(
            "theme=dark; {}=the-token; other=x",
            config::auth::COOKIE_NAME
        ),
    );
    assert_eq!(cookie_token(&map), Some("the-token"));
}

#[test]
fn us19_no_session_cookie_is_no_token() {
    assert_eq!(cookie_token(&headers(header::COOKIE, "theme=dark")), None);
    assert_eq!(cookie_token(&HeaderMap::new()), None);
}

#[test]
fn us19_a_bearer_token_is_read_whatever_the_scheme_case() {
    assert_eq!(
        bearer_token(&headers(header::AUTHORIZATION, "Bearer the-token")),
        Some("the-token")
    );
    assert_eq!(
        bearer_token(&headers(header::AUTHORIZATION, "bearer the-token")),
        Some("the-token")
    );
    assert_eq!(
        bearer_token(&headers(header::AUTHORIZATION, "Basic dXNlcjpwdw==")),
        None,
        "basic auth is what the amendment replaced; it is not a second way in"
    );
}

#[test]
fn us19_a_cookie_and_a_bearer_are_the_same_token_read_two_ways() {
    let auth = an_auth();
    let session = auth.mint(a_time());
    let by_cookie = resolve(
        &auth,
        &headers(
            header::COOKIE,
            &format!("{}={}", config::auth::COOKIE_NAME, session.token),
        ),
        a_time(),
    );
    let by_bearer = resolve(
        &auth,
        &headers(header::AUTHORIZATION, &format!("Bearer {}", session.token)),
        a_time(),
    );
    assert!(matches!(by_cookie, Some((_, Credential::Cookie))));
    assert!(matches!(by_bearer, Some((_, Credential::Bearer))));
}

#[test]
fn us19_an_unsigned_request_resolves_to_nobody() {
    assert!(resolve(&an_auth(), &HeaderMap::new(), a_time()).is_none());
}

#[test]
fn us19_the_cookie_is_httponly_secure_and_samesite_lax() {
    // Each attribute earns its place in `session_cookie`'s doc comment; this
    // is the assertion that none of them quietly goes missing.
    let cookie = session_cookie("the-token", std::time::Duration::from_secs(60));
    assert!(cookie.starts_with(&format!("{}=the-token;", config::auth::COOKIE_NAME)));
    assert!(cookie.contains("Max-Age=60"));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("Secure"));
    assert!(cookie.contains("SameSite=Lax"));
    assert!(cookie.contains("Path=/"));
}

#[test]
fn us19_signing_out_clears_the_cookie() {
    assert!(cleared_cookie().contains("Max-Age=0"));
}
