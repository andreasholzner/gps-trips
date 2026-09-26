//! US-70 — as the owner, I see how my archive is used, and above all how the
//! links I shared are used.
//!
//! Acceptance criteria, and where each is asserted below:
//!
//! * *who made each request — the owner, a share (its id and label), an
//!   anonymous caller, or an unknown link* — `us70_every_response_names_its_caller`
//!   and its siblings, read off the response the gate hands back: what the
//!   access log records is what the gate decided, not a second guess.
//! * *every request is written to stdout as one line: method, path, status,
//!   duration, who made it, IP address and user agent* —
//!   `us70_each_request_is_one_line_saying_who_made_it` and
//!   `us70_the_ip_is_the_one_the_platform_saw`.
//! * *a share's token never appears, nor a cookie, an `Authorization`
//!   header, a request body or the password* —
//!   `us70_no_credential_reaches_the_log`.
//! * *the bundle's content-hashed files are not logged at the level the
//!   rest is* — `us70_bundle_files_are_logged_only_when_debugging`.
//! * *every request except the bundle's files is stored: time, method, path,
//!   status, duration, who made it, the share's label and the user agent —
//!   never an IP address* — `us70_each_request_is_stored_with_who_made_it`,
//!   `us70_no_ip_address_and_no_token_is_stored` and
//!   `us70_bundle_files_are_not_stored`.
//! * *loading a share's page counts as that share, not as an anonymous
//!   caller* — `us70_loading_a_shares_page_counts_as_the_share`; the owner
//!   previewing their own link stays the owner, so it counts as no opening.
//! * *the Shares screen shows, per active share, how often its link was
//!   opened, when last, and on which devices* —
//!   `us70_the_owner_sees_how_often_a_link_was_opened` (the screen itself is
//!   asserted in the UI crate).
//! * *a stopped share's records stay, label included* —
//!   `us70_a_stopped_shares_records_keep_its_label`.

use crate::common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    response::Response,
    Router,
};
use std::sync::{Arc, Mutex};

use tracing::level_filters::LevelFilter;
use trip_archive::models::CreatedShare;
use trip_archive::server::auth::Caller;
use trip_archive::server::{db, http, state::AppState, storage::LocalDisk};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Share `trip_ids` as the owner, labelled `label`, and return the share.
async fn share(app: &Router, trip_ids: &[i64], label: Option<&str>) -> CreatedShare {
    let body = serde_json::json!({ "trip_ids": trip_ids, "label": label });
    let response = common::send(
        app,
        common::json_request(Method::POST, "/api/shares", &body.to_string()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    serde_json::from_str(&common::body_string(response).await).expect("CreatedShare JSON")
}

/// The share's id, as the owner's list reports it.
async fn share_id(app: &Router, token: &str) -> i64 {
    let listed: Vec<serde_json::Value> =
        serde_json::from_str(&common::body_string(common::get(app, "/api/shares").await).await)
            .unwrap();
    listed
        .iter()
        .find(|share| share["token"] == token)
        .expect("the share is listed")["id"]
        .as_i64()
        .unwrap()
}

/// A request as a stranger makes it: no session.
async fn visit(app: &Router, uri: &str) -> Response {
    common::send_unauthenticated(
        app,
        Request::builder().uri(uri).body(Body::empty()).unwrap(),
    )
    .await
}

fn caller(response: &Response) -> Caller {
    response
        .extensions()
        .get::<Caller>()
        .cloned()
        .expect("the gate names the caller on every response")
}

/// Everything logged at `level` or above while the returned guard lives.
/// The tests run on a current-thread runtime, so the access log — written
/// by the router's own middleware, on the test's thread — lands here.
fn capture(level: LevelFilter) -> (tracing::subscriber::DefaultGuard, Arc<Mutex<Vec<u8>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let writer = {
        let log = Arc::clone(&log);
        move || Writer(Arc::clone(&log))
    };
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(writer)
        .with_ansi(false)
        .finish();
    (tracing::subscriber::set_default(subscriber), log)
}

struct Writer(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Writer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The access-log lines in `log`, one per request.
fn access_lines(log: &Arc<Mutex<Vec<u8>>>) -> Vec<String> {
    String::from_utf8(log.lock().unwrap().clone())
        .unwrap()
        .lines()
        .filter(|line| line.contains("trip_archive::access"))
        .map(str::to_string)
        .collect()
}

fn request_with(uri: &str, headers: &[(&str, &str)]) -> Request<Body> {
    let mut request = Request::builder().uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    request.body(Body::empty()).unwrap()
}

// ── Who made the request ─────────────────────────────────────────────────────

#[tokio::test]
async fn us70_every_response_names_its_caller() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    let created = share(&app, &[id], Some("For Kari")).await;
    let share_id = share_id(&app, &created.token).await;

    let owner = common::get(&app, "/api/trips").await;
    assert_eq!(caller(&owner), Caller::Owner);

    let recipient = visit(&app, &format!("/s/{}/api/share", created.token)).await;
    assert_eq!(recipient.status(), StatusCode::OK);
    assert_eq!(
        caller(&recipient),
        Caller::Share {
            id: share_id,
            label: Some("For Kari".to_string()),
        }
    );

    let refused = visit(&app, "/api/trips").await;
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(caller(&refused), Caller::Anonymous);

    let public = visit(&app, "/").await;
    assert_eq!(caller(&public), Caller::Anonymous);
}

#[tokio::test]
async fn us70_a_link_that_opens_nothing_is_named_as_such() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    let created = share(&app, &[id], None).await;
    let share_id = share_id(&app, &created.token).await;
    common::delete(&app, &format!("/api/shares/{share_id}")).await;

    for uri in [
        format!("/s/{}/api/share", "0".repeat(64)),
        format!("/s/{}/api/share", created.token),
    ] {
        let response = visit(&app, &uri).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(caller(&response), Caller::UnknownLink, "{uri}");
    }
}

#[tokio::test]
async fn us70_a_share_without_a_label_is_named_by_its_id_alone() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    let created = share(&app, &[id], None).await;
    let share_id = share_id(&app, &created.token).await;

    let response = visit(&app, &format!("/s/{}/api/share", created.token)).await;
    assert_eq!(
        caller(&response),
        Caller::Share {
            id: share_id,
            label: None,
        }
    );
}

/// An app whose state the test keeps, to flush the access log and read the
/// table it writes.
async fn app_with_state() -> (Router, AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let pool = db::create_pool(&dir.path().join("test.db")).await.unwrap();
    let store = std::sync::Arc::new(LocalDisk::new(dir.path().join("blobs")));
    let state = AppState::new(pool, store, None, common::test_auth());
    (http::router(state.clone()), state, dir)
}

/// Every stored record, oldest first, as
/// `(method, path, status, caller, share_id, share_label, user_agent)`.
type Stored = (
    String,
    String,
    i64,
    String,
    Option<i64>,
    Option<String>,
    Option<String>,
);

async fn stored(state: &AppState) -> Vec<Stored> {
    state.access_log.flush().await;
    sqlx::query_as(
        r#"SELECT l.method, l.path, l.status, l.caller, l.share_id, l.share_label, u.value
           FROM access_log l LEFT JOIN user_agent u ON u.id = l.user_agent_id
           ORDER BY l.id"#,
    )
    .fetch_all(&state.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn us70_loading_a_shares_page_counts_as_the_share() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    let created = share(&app, &[id], Some("For Kari")).await;
    let the_share = Caller::Share {
        id: share_id(&app, &created.token).await,
        label: Some("For Kari".to_string()),
    };

    for uri in [
        format!("/app/s/{}", created.token),
        format!("/app/s/{}/trips/{id}", created.token),
    ] {
        assert_eq!(caller(&visit(&app, &uri).await), the_share, "{uri}");
    }
    let unknown = visit(&app, &format!("/app/s/{}", "0".repeat(64))).await;
    assert_eq!(caller(&unknown), Caller::UnknownLink);

    let preview = common::get(&app, &format!("/app/s/{}", created.token)).await;
    assert_eq!(caller(&preview), Caller::Owner);
}

// ── The line on stdout ───────────────────────────────────────────────────────

#[tokio::test]
async fn us70_each_request_is_one_line_saying_who_made_it() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    let created = share(&app, &[id], Some("For Kari")).await;
    let share_id = share_id(&app, &created.token).await;

    let (_guard, log) = capture(LevelFilter::INFO);
    common::get(&app, "/api/trips").await;
    common::send_unauthenticated(
        &app,
        request_with(
            &format!("/s/{}/api/trips/{id}", created.token),
            &[("user-agent", "Mozilla/5.0 (iPhone)")],
        ),
    )
    .await;
    visit(&app, "/api/trips").await;
    visit(&app, &format!("/s/{}/api/share", "0".repeat(64))).await;

    let lines = access_lines(&log);
    assert_eq!(lines.len(), 4, "{lines:#?}");
    assert!(lines[0].contains("GET /api/trips 200 "), "{}", lines[0]);
    assert!(lines[0].contains("who=owner"), "{}", lines[0]);
    assert!(
        lines[1].contains(&format!("GET /s/…/api/trips/{id} 200 ")),
        "{}",
        lines[1]
    );
    assert!(
        lines[1].contains(&format!(r#"who=share share={share_id} label="For Kari""#)),
        "{}",
        lines[1]
    );
    assert!(
        lines[1].contains(r#"agent="Mozilla/5.0 (iPhone)""#),
        "{}",
        lines[1]
    );
    assert!(lines[2].contains("GET /api/trips 401 "), "{}", lines[2]);
    assert!(lines[2].contains("who=anonymous"), "{}", lines[2]);
    assert!(lines[3].contains("GET /s/…/api/share 404 "), "{}", lines[3]);
    assert!(lines[3].contains("who=unknown-link"), "{}", lines[3]);
}

#[tokio::test]
async fn us70_the_ip_is_the_one_the_platform_saw() {
    let (app, _dir) = common::test_app().await;

    let (_guard, log) = capture(LevelFilter::INFO);
    common::send_unauthenticated(&app, request_with("/", &[("fly-client-ip", "203.0.113.9")]))
        .await;

    let lines = access_lines(&log);
    assert!(lines[0].contains("ip=203.0.113.9"), "{}", lines[0]);
}

#[tokio::test]
async fn us70_no_credential_reaches_the_log() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    let created = share(&app, &[id], Some("For Kari")).await;
    let token = common::test_token();

    let (_guard, log) = capture(LevelFilter::TRACE);
    for uri in [
        format!("/s/{}/api/share", created.token),
        format!("/s/{}/media/trips/{id}/x.jpg", created.token),
        format!("/app/s/{}", created.token),
        format!("/app/s/{}/trips/{id}", created.token),
    ] {
        visit(&app, &uri).await;
    }
    common::get(&app, "/api/trips").await;
    common::send_unauthenticated(
        &app,
        request_with(
            "/api/trips",
            &[("cookie", &format!("trip_archive_session={token}"))],
        ),
    )
    .await;
    common::send_unauthenticated(
        &app,
        common::json_request(
            Method::POST,
            "/api/session",
            &serde_json::json!({ "password": common::TEST_PASSWORD }).to_string(),
        ),
    )
    .await;

    let everything = String::from_utf8(log.lock().unwrap().clone()).unwrap();
    assert_eq!(access_lines(&log).len(), 7, "{everything}");
    for secret in [
        created.token.as_str(),
        token.as_str(),
        common::TEST_PASSWORD,
    ] {
        assert!(!everything.contains(secret), "{secret:?} in {everything}");
    }
    assert!(everything.contains("GET /app/s/… "), "{everything}");
    assert!(
        everything.contains(&format!("GET /app/s/…/trips/{id} ")),
        "{everything}"
    );
}

#[tokio::test]
async fn us70_bundle_files_are_logged_only_when_debugging() {
    let (app, _dir) = common::test_app().await;
    let asset = "/app/assets/ui-dioxus-dxh0123456789abcdef.js";

    let (guard, log) = capture(LevelFilter::INFO);
    visit(&app, asset).await;
    visit(&app, "/app/").await;
    let lines = access_lines(&log);
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(lines[0].contains("GET /app/ "), "{}", lines[0]);
    drop(guard);

    let (_guard, log) = capture(LevelFilter::DEBUG);
    visit(&app, asset).await;
    assert_eq!(access_lines(&log).len(), 1);
}

// ── The stored record ────────────────────────────────────────────────────────

#[tokio::test]
async fn us70_each_request_is_stored_with_who_made_it() {
    let (app, state, _dir) = app_with_state().await;
    let id = common::import_sample(&app).await;
    let created = share(&app, &[id], Some("For Kari")).await;
    let share_id = share_id(&app, &created.token).await;
    let before = stored(&state).await.len();

    let phone = [("user-agent", "Mozilla/5.0 (iPhone)")];
    common::send(
        &app,
        request_with("/api/trips", &[("user-agent", "Firefox")]),
    )
    .await;
    for _ in 0..2 {
        common::send_unauthenticated(
            &app,
            request_with(&format!("/s/{}/api/share", created.token), &phone),
        )
        .await;
    }
    visit(&app, "/api/trips").await;
    visit(&app, &format!("/s/{}/api/share", "0".repeat(64))).await;

    let rows = stored(&state).await.split_off(before);
    let share = |ua: &str| {
        (
            "GET".to_string(),
            "/s/…/api/share".to_string(),
            200,
            "share".to_string(),
            Some(share_id),
            Some("For Kari".to_string()),
            Some(ua.to_string()),
        )
    };
    assert_eq!(
        rows,
        vec![
            (
                "GET".to_string(),
                "/api/trips".to_string(),
                200,
                "owner".to_string(),
                None,
                None,
                Some("Firefox".to_string()),
            ),
            share("Mozilla/5.0 (iPhone)"),
            share("Mozilla/5.0 (iPhone)"),
            (
                "GET".to_string(),
                "/api/trips".to_string(),
                401,
                "anonymous".to_string(),
                None,
                None,
                None,
            ),
            (
                "GET".to_string(),
                "/s/…/api/share".to_string(),
                404,
                "unknown_link".to_string(),
                None,
                None,
                None,
            ),
        ]
    );
    // A user agent is stored once, however many requests it made.
    let agents: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_agent WHERE value = 'Mozilla/5.0 (iPhone)'")
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(agents, 1);
}

#[tokio::test]
async fn us70_no_ip_address_and_no_token_is_stored() {
    let (app, state, _dir) = app_with_state().await;
    let id = common::import_sample(&app).await;
    let created = share(&app, &[id], Some("For Kari")).await;

    common::send_unauthenticated(
        &app,
        request_with(
            &format!("/s/{}/api/share", created.token),
            &[("fly-client-ip", "203.0.113.9")],
        ),
    )
    .await;
    visit(&app, &format!("/app/s/{}", created.token)).await;
    state.access_log.flush().await;

    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('access_log')")
            .fetch_all(&state.pool)
            .await
            .unwrap();
    assert!(
        !columns.iter().any(|column| column.contains("ip")),
        "{columns:?}"
    );
    let dump: Vec<String> = sqlx::query_scalar(
        r#"SELECT concat_ws('|', at, method, path, status, duration_ms, caller,
                            share_id, share_label, user_agent_id)
           FROM access_log"#,
    )
    .fetch_all(&state.pool)
    .await
    .unwrap();
    for row in &dump {
        assert!(!row.contains(&created.token), "{row}");
        assert!(!row.contains("203.0.113.9"), "{row}");
    }
}

#[tokio::test]
async fn us70_bundle_files_are_not_stored() {
    let (app, state, _dir) = app_with_state().await;

    visit(&app, "/app/assets/ui-dioxus-dxh0123456789abcdef.js").await;
    visit(&app, "/app/").await;

    let paths: Vec<String> = stored(&state).await.into_iter().map(|row| row.1).collect();
    assert_eq!(paths, ["/app/"]);
}

#[tokio::test]
async fn us70_a_stopped_shares_records_keep_its_label() {
    let (app, state, _dir) = app_with_state().await;
    let id = common::import_sample(&app).await;
    let created = share(&app, &[id], Some("For Kari")).await;
    let share_id = share_id(&app, &created.token).await;
    visit(&app, &format!("/s/{}/api/share", created.token)).await;

    common::delete(&app, &format!("/api/shares/{share_id}")).await;

    let kept: Vec<Stored> = stored(&state)
        .await
        .into_iter()
        .filter(|row| row.4 == Some(share_id))
        .collect();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].5.as_deref(), Some("For Kari"));
}

#[tokio::test]
async fn us70_the_owner_sees_how_often_a_link_was_opened() {
    let (app, state, _dir) = app_with_state().await;
    let id = common::import_sample(&app).await;
    let created = share(&app, &[id], Some("For Kari")).await;
    for agent in ["Phone", "Phone", "Laptop"] {
        common::send_unauthenticated(
            &app,
            request_with(
                &format!("/app/s/{}", created.token),
                &[("user-agent", agent)],
            ),
        )
        .await;
    }
    state.access_log.flush().await;

    let listed: Vec<trip_archive::models::ActiveShare> =
        serde_json::from_str(&common::body_string(common::get(&app, "/api/shares").await).await)
            .unwrap();

    assert_eq!(listed[0].opens, 3);
    assert!(listed[0].last_opened_at.is_some());
    assert_eq!(listed[0].user_agents, ["Laptop", "Phone"]);
}
