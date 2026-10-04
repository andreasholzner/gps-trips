//! US-74 — a suggested name that says where the trip went, behind its date.
//!
//! Acceptance criteria: the staged parse returns the date and the trip's
//! places in the name field, read from its track in the offline place-name
//! database (ADR-0027); the edit form is offered the same for the trip as it
//! is stored. Without places — none found, or no database — the suggestion
//! falls back to US-12's, and a name left empty still falls back as before.
//!
//! Tested against synthetic tracks — straight lines between public places,
//! no real track of the owner's — each with the name it should get, over a
//! place-name fixture cut from the real database around them (`places_build
//! cut`). The fixture holds OpenStreetMap data (© OpenStreetMap
//! contributors, ODbL) and Kartverket's place names (CC BY 4.0).

use axum::http::StatusCode;
use trip_archive::models::TripSuggestion;

use crate::common::{
    body_string, confirm_import_request, get, send, stage_import, test_app, test_app_with_places,
    SAMPLE_GPX,
};

const RYSSTAD_KILEFJORDEN: &[u8] =
    include_bytes!("../fixtures/place_names/rysstad_kilefjorden.gpx");
const LANGRYGGEN_HAMPEROKKEN: &[u8] =
    include_bytes!("../fixtures/place_names/langryggen_hamperokken.gpx");
const TROMSO_KVALOYVAGEN: &[u8] = include_bytes!("../fixtures/place_names/tromso_kvaloyvagen.gpx");
const RIDGE_TRAVERSE: &[u8] = include_bytes!("../fixtures/place_names/ridge_traverse.gpx");
const GRYLLEFJORD_FJORDBOTN: &[u8] =
    include_bytes!("../fixtures/place_names/gryllefjord_fjordbotn.gpx");

/// `gpx` without its timestamps: a planned trip.
fn untimed(gpx: &[u8]) -> Vec<u8> {
    let text = std::str::from_utf8(gpx).expect("UTF-8");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<time>") {
        out.push_str(&rest[..start]);
        let end = rest[start..].find("</time>").expect("a closed <time>") + start;
        rest = &rest[end + "</time>".len()..];
    }
    out.push_str(rest);
    out.into_bytes()
}

async fn suggestion(app: &axum::Router, id: i64) -> (StatusCode, String) {
    let response = get(app, &format!("/api/trips/{id}/suggestion")).await;
    (response.status(), body_string(response).await)
}

/// Stage and confirm `gpx` under `name`; the new trip's id.
async fn import_named(app: &axum::Router, gpx: &[u8], name: &str) -> i64 {
    let staged = stage_import(app, gpx).await;
    let body = serde_json::json!({ "name": name }).to_string();
    let response = send(app, confirm_import_request(staged.staging_id, &body)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_str(&body_string(response).await).expect("JSON");
    body["id"].as_i64().expect("an id")
}

// ── Import ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us74_the_staged_parse_suggests_the_trip_s_places_behind_its_date() {
    let (app, _dir) = test_app_with_places().await;

    for (gpx, expected) in [
        // One way: start - end, the end at a campsite named after its lake.
        (RYSSTAD_KILEFJORDEN, "2019-09-07 Rysstad - Kilefjorden"),
        // One way, the end at a campsite named after the place beside it.
        (GRYLLEFJORD_FJORDBOTN, "2019-06-23 Gryllefjord - Fjordbotn"),
        // A round trip: its start, then where it went.
        (LANGRYGGEN_HAMPEROKKEN, "2024-07-31 Langryggen: Hamperokken"),
        (TROMSO_KVALOYVAGEN, "2026-06-27 Tromsø: Kvaløyvågen"),
        // No name at either end: the main places in track order, a summit
        // known only by its height among them.
        (
            RIDGE_TRAVERSE,
            "2024-07-27 Leirholtinden - Storsteinnestinden - 884",
        ),
    ] {
        assert_eq!(stage_import(&app, gpx).await.suggested_name, expected);
    }
}

#[tokio::test]
async fn us74_a_planned_trip_s_places_stand_alone() {
    let (app, _dir) = test_app_with_places().await;

    let staged = stage_import(&app, &untimed(LANGRYGGEN_HAMPEROKKEN)).await;

    assert_eq!(staged.suggested_name, "Langryggen: Hamperokken");
}

#[tokio::test]
async fn us74_with_no_place_found_the_suggestion_is_us12_s() {
    // Oslo: nothing in the fixture is anywhere near.
    let (app, _dir) = test_app_with_places().await;

    let staged = stage_import(&app, SAMPLE_GPX).await;

    assert_eq!(staged.suggested_name, "2024-06-01 Oslo Hills Walk");
}

#[tokio::test]
async fn us74_without_the_place_database_the_suggestion_is_us12_s() {
    let (app, _dir) = test_app().await;

    let staged = stage_import(&app, RYSSTAD_KILEFJORDEN).await;

    assert_eq!(staged.suggested_name, "2019-09-07 ");
}

#[tokio::test]
async fn us74_a_name_left_empty_falls_back_as_before() {
    let (app, _dir) = test_app_with_places().await;

    let id = import_named(&app, RYSSTAD_KILEFJORDEN, "").await;

    let response = get(&app, &format!("/api/trips/{id}")).await;
    let trip: serde_json::Value = serde_json::from_str(&body_string(response).await).expect("JSON");
    assert_eq!(trip["name"], "2019-09-07 Imported Trip");
}

// ── Editing ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us74_the_edit_form_is_offered_the_suggestion_for_the_trip_as_stored() {
    let (app, _dir) = test_app_with_places().await;
    let id = import_named(&app, LANGRYGGEN_HAMPEROKKEN, "Unnamed walk").await;

    let (status, body) = suggestion(&app, id).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let offered: TripSuggestion = serde_json::from_str(&body).expect("suggestion JSON");
    assert_eq!(offered.name, "2024-07-31 Langryggen: Hamperokken");
    // Only offered: the owner's own name stands.
    let response = get(&app, &format!("/api/trips/{id}")).await;
    let trip: serde_json::Value = serde_json::from_str(&body_string(response).await).expect("JSON");
    assert_eq!(trip["name"], "Unnamed walk");
}

#[tokio::test]
async fn us74_without_the_place_database_the_edit_form_is_offered_us12_s_suggestion() {
    let (app, _dir) = test_app().await;
    let id = import_named(&app, SAMPLE_GPX, "My walk").await;

    let (status, body) = suggestion(&app, id).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let offered: TripSuggestion = serde_json::from_str(&body).expect("suggestion JSON");
    assert_eq!(offered.name, "2024-06-01 Oslo Hills Walk");
}

#[tokio::test]
async fn us74_there_is_no_suggestion_for_a_trip_that_does_not_exist() {
    let (app, _dir) = test_app_with_places().await;

    let (status, _) = suggestion(&app, 999).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}
