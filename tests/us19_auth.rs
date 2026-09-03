//! US-19 — as the owner, my archive is reachable only by me, so putting it on
//! the public internet does not put my trips there.
//!
//! Acceptance criteria, and where each is asserted below:
//!
//! * *a shared password is the only way in; every route except the login
//!   endpoint, the SPA bundle and the redirects answers `401` with JSON and
//!   no data* — `us19_every_route_refuses_an_anonymous_request`, which is a
//!   table over the whole router: deny-by-default is a property only if it
//!   is asserted, and a route added without a thought about auth fails here.
//! * *a route added later is protected without anyone remembering to protect
//!   it* — the same table, plus `us19_an_unknown_api_route_is_gated_too`.
//! * *a correct password opens a session that survives a reload, a redeploy,
//!   and the machine being stopped and woken* — the session is a signature
//!   over its own expiry under a key derived from the password, so it
//!   survives anything that keeps the password: asserted as
//!   `us19_a_session_outlives_the_process_that_issued_it`.
//! * *a wrong one is rate-limited* — `us19_repeated_wrong_passwords_are_locked_out`.
//! * *signing out ends the session; changing the password ends every session
//!   that exists* — `us19_signing_out_ends_the_session` and
//!   `us19_changing_the_password_ends_every_session`.
//!
//! The SPA's own half — showing a login screen rather than a browser dialog,
//! and returning to the screen asked for — is asserted in the UI crate and,
//! for the typing and clicking, in the browser layer (ADR-0012's amendments).
//!
//! The gate's internals (token signing, the allowlist predicate, the failure
//! counter) are unit-tested in `src/server/auth/tests.rs`.

mod common;

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    response::Response,
};
use trip_archive::config;
use trip_archive::models::{ErrorResponse, Identity, Principal, Session};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn anonymous(method: Method, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

fn with_cookie(method: Method, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(
            header::COOKIE,
            format!("{}={token}", config::auth::COOKIE_NAME),
        )
        .body(Body::empty())
        .unwrap()
}

fn login_request(password: &str) -> Request<Body> {
    common::json_request(
        Method::POST,
        "/api/session",
        &serde_json::json!({ "password": password }).to_string(),
    )
}

async fn error_of(response: Response) -> ErrorResponse {
    let body = common::body_string(response).await;
    serde_json::from_str(&body)
        .unwrap_or_else(|e| panic!("a refusal must be JSON: {e}; got {body}"))
}

fn set_cookies(response: &Response) -> Vec<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap().to_string())
        .collect()
}

/// Sign in and return the session the archive issued.
async fn sign_in(app: &axum::Router) -> Session {
    let response = common::send_unauthenticated(app, login_request(common::TEST_PASSWORD)).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the shared password must open a session"
    );
    serde_json::from_str(&common::body_string(response).await).expect("a session as JSON")
}

// ── Deny by default ──────────────────────────────────────────────────────────

/// Every route the router answers, with the response an anonymous request
/// gets. Hand-mirrored from `src/server/http.rs` — axum publishes no list of
/// its routes to iterate — so this is a checklist, not an enforcement: a new
/// route left out of it is still *protected* (that is what deny-by-default
/// buys, and `us19_an_unknown_api_route_is_gated_too` holds the line), but a
/// new *public* one would go unasserted. Add the line when you add the
/// route.
const ROUTES: &[(Method, &str, bool)] = &[
    // (method, path, reachable without a session)
    (Method::POST, "/api/session", true),
    (Method::GET, "/api/session", false),
    (Method::DELETE, "/api/session", false),
    (Method::GET, "/", true),
    (Method::GET, "/import", true),
    (Method::GET, "/trips/1", true),
    (Method::GET, "/komoot/sync", true),
    (Method::POST, "/api/import", false),
    (Method::POST, "/api/import/staged", false),
    (Method::POST, "/api/import/staged/1/confirm", false),
    (Method::DELETE, "/api/import/staged/1", false),
    (Method::GET, "/api/trips", false),
    (Method::GET, "/api/trips/1", false),
    (Method::PATCH, "/api/trips/1", false),
    (Method::DELETE, "/api/trips/1", false),
    (Method::GET, "/api/trips/1/gpx", false),
    (Method::GET, "/api/trips/1/track.geojson", false),
    (Method::GET, "/api/trips/1/photos", false),
    (Method::POST, "/api/trips/1/photos", false),
    (Method::GET, "/api/trips/1/tags", false),
    (Method::POST, "/api/trips/1/tags", false),
    (Method::DELETE, "/api/trips/1/tags/1", false),
    (Method::POST, "/api/trips/tags", false),
    (Method::GET, "/api/tags", false),
    (Method::GET, "/api/komoot/sync", false),
    (Method::POST, "/api/komoot/sync", false),
    (Method::GET, "/media/trips/1/photo.jpg", false),
    (Method::GET, "/app/", true),
    (Method::GET, "/app/assets/app.css", true),
];

#[tokio::test]
async fn us19_every_route_refuses_an_anonymous_request() {
    let (app, _dir) = common::test_app().await;

    for (method, path, public) in ROUTES {
        let response = common::send_unauthenticated(&app, anonymous(method.clone(), path)).await;
        if *public {
            assert_ne!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "{method} {path} is allowlisted and must not be refused"
            );
            continue;
        }
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} answered an anonymous request with something other than 401"
        );
    }
}

#[tokio::test]
async fn us19_a_refusal_is_json_and_carries_no_data() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;

    let response =
        common::send_unauthenticated(&app, anonymous(Method::GET, &format!("/api/trips/{id}")))
            .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // The raw body, deliberately: this test is the one that asserts the
    // *shape* every other test now reads through `common::error_message`.
    let body = common::body_string(response).await;
    assert!(
        !body.contains("Oslo"),
        "a refusal must say nothing about the trip it refused; got {body}"
    );
    let error: ErrorResponse = serde_json::from_str(&body).expect("a refusal must be JSON");
    assert!(!error.error.is_empty());
}

#[tokio::test]
async fn us19_an_unknown_api_route_is_gated_too() {
    // The point of deny-by-default: a path nobody has written a rule for is
    // refused, not served. (404 would leak which paths exist; it also would
    // not be a refusal.)
    let (app, _dir) = common::test_app().await;
    let response =
        common::send_unauthenticated(&app, anonymous(Method::GET, "/api/something/new")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn us19_the_bookmarks_reach_a_login_screen_rather_than_a_json_refusal() {
    // The redirects are allowlisted so an old bookmark lands on the SPA,
    // which shows its own login screen — not on a JSON body in the address
    // bar. They carry a `Location` and nothing else.
    let (app, _dir) = common::test_app().await;
    for (path, target) in [
        ("/", "/app/"),
        ("/import", "/app/import"),
        ("/trips/7", "/app/trips/7"),
        ("/komoot/sync", "/app/komoot/sync"),
    ] {
        let response = common::send_unauthenticated(&app, anonymous(Method::GET, path)).await;
        assert!(
            response.status().is_redirection(),
            "{path} must still redirect for an anonymous visitor; got {}",
            response.status()
        );
        assert_eq!(response.headers()["location"], target);
    }
}

// ── Signing in ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn us19_the_shared_password_opens_a_session() {
    let (app, _dir) = common::test_app().await;
    let response = common::send_unauthenticated(&app, login_request(common::TEST_PASSWORD)).await;

    assert_eq!(response.status(), StatusCode::OK);
    let cookies = set_cookies(&response);
    let cookie = cookies
        .iter()
        .find(|c| c.starts_with(config::auth::COOKIE_NAME))
        .unwrap_or_else(|| panic!("signing in must set the session cookie; got {cookies:?}"));
    // The attributes the mechanism rests on: unreadable by script, HTTPS
    // only, and withheld from cross-site form posts (the CSRF answer).
    assert!(cookie.contains("HttpOnly"), "got {cookie}");
    assert!(cookie.contains("Secure"), "got {cookie}");
    assert!(cookie.contains("SameSite=Lax"), "got {cookie}");

    let session: Session = serde_json::from_str(&common::body_string(response).await)
        .expect("the token also travels in the body, for a client with no cookie store");
    assert!(!session.token.is_empty());
}

#[tokio::test]
async fn us19_a_wrong_password_opens_nothing() {
    let (app, _dir) = common::test_app().await;
    let response = common::send_unauthenticated(&app, login_request("not the password")).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(
        set_cookies(&response).is_empty(),
        "a refused login must set no cookie"
    );
    assert!(!error_of(response).await.error.is_empty());
}

#[tokio::test]
async fn us19_the_session_cookie_reaches_the_archive() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    let session = sign_in(&app).await;

    let response = common::send_unauthenticated(
        &app,
        with_cookie(Method::GET, &format!("/api/trips/{id}"), &session.token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn us19_the_same_token_works_as_a_bearer_header() {
    // ADR-0010's amendment: the Android app's native client (US-16) has no
    // cookie store to put this in, so the token is accepted both ways.
    let (app, _dir) = common::test_app().await;
    let session = sign_in(&app).await;

    let request = Request::builder()
        .uri("/api/trips")
        .header(header::AUTHORIZATION, format!("Bearer {}", session.token))
        .body(Body::empty())
        .unwrap();
    let response = common::send_unauthenticated(&app, request).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn us19_the_archive_says_who_the_caller_is() {
    let (app, _dir) = common::test_app().await;
    let session = sign_in(&app).await;

    let response = common::send_unauthenticated(
        &app,
        with_cookie(Method::GET, "/api/session", &session.token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let identity: Identity =
        serde_json::from_str(&common::body_string(response).await).expect("an identity as JSON");
    assert_eq!(identity.principal, Principal::Owner);
}

#[tokio::test]
async fn us19_a_forged_or_expired_token_is_refused() {
    let (app, _dir) = common::test_app().await;
    let expired = common::test_auth()
        .mint(time::OffsetDateTime::now_utc() - config::auth::SESSION_TTL - time::Duration::days(1))
        .token;

    for token in ["", "nonsense", "9999999999.deadbeef", expired.as_str()] {
        let response =
            common::send_unauthenticated(&app, with_cookie(Method::GET, "/api/trips", token)).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{token:?} must not reach the archive"
        );
    }
}

// ── Staying signed in ────────────────────────────────────────────────────────

#[tokio::test]
async fn us19_a_session_outlives_the_process_that_issued_it() {
    // "Survives a reload, a redeploy, and the machine being stopped and
    // woken" (ADR-0023's scale-to-zero instance): the session is a signature
    // over its own expiry, not a row in memory, so a *different* server
    // holding the same password accepts it. Two routers, one password.
    let (first, _dir) = common::test_app().await;
    let session = sign_in(&first).await;

    let (restarted, _dir2) = common::test_app().await;
    let response = common::send_unauthenticated(
        &restarted,
        with_cookie(Method::GET, "/api/trips", &session.token),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a restart must not sign the owner out"
    );
}

#[tokio::test]
async fn us19_a_session_past_its_halfway_mark_is_renewed() {
    // The sliding half of the lifetime: the phone is the primary client, and
    // a login screen there is friction, not security.
    let (app, _dir) = common::test_app().await;
    let ageing = common::test_auth()
        .mint(
            time::OffsetDateTime::now_utc()
                - config::auth::SESSION_REFRESH_AFTER
                - time::Duration::days(1),
        )
        .token;

    let response =
        common::send_unauthenticated(&app, with_cookie(Method::GET, "/api/trips", &ageing)).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        set_cookies(&response)
            .iter()
            .any(|c| c.starts_with(config::auth::COOKIE_NAME)),
        "a session more than halfway through its life must come back renewed"
    );
}

#[tokio::test]
async fn us19_a_fresh_session_is_not_renewed_on_every_request() {
    let (app, _dir) = common::test_app().await;
    let session = sign_in(&app).await;

    let response =
        common::send_unauthenticated(&app, with_cookie(Method::GET, "/api/trips", &session.token))
            .await;
    assert!(
        set_cookies(&response).is_empty(),
        "a session nowhere near expiry needs no new cookie on every request"
    );
}

// ── Ending a session ─────────────────────────────────────────────────────────

#[tokio::test]
async fn us19_signing_out_ends_the_session() {
    let (app, _dir) = common::test_app().await;
    let session = sign_in(&app).await;

    let response = common::send_unauthenticated(
        &app,
        with_cookie(Method::DELETE, "/api/session", &session.token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(
        set_cookies(&response)
            .iter()
            .any(|c| c.contains("Max-Age=0")),
        "signing out must clear the cookie the browser holds"
    );
}

#[tokio::test]
async fn us19_changing_the_password_ends_every_session() {
    // The consequence ADR-0010's amendment accepts by name: the signing key
    // is derived from the password, so rotating the secret *is* the
    // revocation — and the phone is logged out.
    let (app, _dir) = common::test_app().await;
    let session = sign_in(&app).await;

    let (rotated, _dir2) = common::test_app_with_password("a brand new password").await;
    let response = common::send_unauthenticated(
        &rotated,
        with_cookie(Method::GET, "/api/trips", &session.token),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "a token signed under the old password must stop being accepted"
    );
}

// ── Rate limiting ────────────────────────────────────────────────────────────

#[tokio::test]
async fn us19_repeated_wrong_passwords_are_locked_out() {
    // One secret on the public internet, and on a scale-to-zero machine each
    // attempt is also a wake-up the owner pays for (ADR-0023).
    let (app, _dir) = common::test_app().await;

    for attempt in 1..=config::auth::LOGIN_FAILURE_LIMIT {
        let response = common::send_unauthenticated(&app, login_request("guess")).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "attempt {attempt} should be refused, not yet locked out"
        );
    }

    let response = common::send_unauthenticated(&app, login_request("guess")).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after = response.headers()[header::RETRY_AFTER]
        .to_str()
        .unwrap()
        .parse::<u64>()
        .expect("Retry-After must be a number of seconds the client can act on");
    assert!(retry_after > 0 && retry_after <= config::auth::LOGIN_LOCKOUT.as_secs());
}

#[tokio::test]
async fn us19_the_lockout_holds_even_for_the_right_password() {
    let (app, _dir) = common::test_app().await;
    for _ in 0..config::auth::LOGIN_FAILURE_LIMIT {
        common::send_unauthenticated(&app, login_request("guess")).await;
    }

    let response = common::send_unauthenticated(&app, login_request(common::TEST_PASSWORD)).await;
    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "a lockout that the right password walks through is not a lockout"
    );
}

#[tokio::test]
async fn us19_a_success_clears_the_failures_behind_it() {
    let (app, _dir) = common::test_app().await;
    for _ in 0..config::auth::LOGIN_FAILURE_LIMIT - 1 {
        common::send_unauthenticated(&app, login_request("guess")).await;
    }
    sign_in(&app).await;

    // A mistyped password now and again must never accumulate into one.
    for _ in 0..config::auth::LOGIN_FAILURE_LIMIT - 1 {
        let response = common::send_unauthenticated(&app, login_request("guess")).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn us19_signing_out_of_a_long_lived_session_still_ends_it() {
    // The gate renews a cookie past its halfway mark — including on the way
    // out. Both headers reach the browser, the later one wins, and the owner
    // who pressed "Sign out" stays signed in. A session older than 45 days is
    // exactly the one most worth being able to end.
    let (app, _dir) = common::test_app().await;
    let ageing = common::test_auth()
        .mint(
            time::OffsetDateTime::now_utc()
                - config::auth::SESSION_REFRESH_AFTER
                - time::Duration::days(1),
        )
        .token;

    let response =
        common::send_unauthenticated(&app, with_cookie(Method::DELETE, "/api/session", &ageing))
            .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let cookies = set_cookies(&response);
    assert!(
        cookies.iter().all(|c| c.contains("Max-Age=0")),
        "signing out must leave no live session cookie behind; got {cookies:?}"
    );
}

#[tokio::test]
async fn us19_signing_in_again_over_a_long_lived_session_issues_one_cookie() {
    // Same root cause from the other side: the login's own cookie must not be
    // followed by the gate's renewal of the one it replaces.
    let (app, _dir) = common::test_app().await;
    let ageing = common::test_auth()
        .mint(
            time::OffsetDateTime::now_utc()
                - config::auth::SESSION_REFRESH_AFTER
                - time::Duration::days(1),
        )
        .token;
    let mut request = login_request(common::TEST_PASSWORD);
    request.headers_mut().insert(
        header::COOKIE,
        format!("{}={ageing}", config::auth::COOKIE_NAME)
            .parse()
            .unwrap(),
    );

    let response = common::send_unauthenticated(&app, request).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        set_cookies(&response).len(),
        1,
        "the handler already said what the session is; the gate must not say it again"
    );
}

// ── Methods ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us19_a_head_request_reaches_the_public_routes_too() {
    // A browser, a link preview, and a platform health check (US-45) all ask
    // this way. `HEAD` is a `GET` without the body, and axum answers it from
    // the same handler, so the gate must read it the same way — otherwise the
    // archive's front door refuses to say it is there.
    let (app, _dir) = common::test_app().await;

    for path in ["/", "/app/", "/import"] {
        let response = common::send_unauthenticated(&app, anonymous(Method::HEAD, path)).await;
        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "HEAD {path} is the same public route as GET {path}"
        );
    }
}

#[tokio::test]
async fn us19_a_head_request_to_a_private_route_is_still_refused() {
    let (app, _dir) = common::test_app().await;
    let response = common::send_unauthenticated(&app, anonymous(Method::HEAD, "/api/trips")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
