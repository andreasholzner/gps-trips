//! The waypoint page (US-98). What the page, the edit form, the symbol
//! chooser and the confirmation say is asserted on rendered HTML, the page
//! once against a real server. Opening the forms, choosing, picking on the
//! map and deleting are the browser layer's
//! (`tests/browser/waypoint_page.spec.mjs`).

use super::edit::{changes, own_symbol, SymbolChooser, WaypointForm};
use super::*;
use crate::test_support::{
    import_collection, render, render_against_archive, serve_test_archive, COLLECTION_GPX,
};
use trip_archive_types::WaypointEdit;

fn a_hut() -> trip_archive_types::Waypoint {
    trip_archive_types::Waypoint {
        id: 4,
        collection_track_id: None,
        name: Some("Chamanna".to_string()),
        description: Some("Open in summer".to_string()),
        lat: 46.58,
        lon: 9.84,
        ele: Some(2065.0),
        symbol: Some("Lodge".to_string()),
    }
}

fn form_of(edit: impl FnOnce(&mut WaypointForm)) -> WaypointForm {
    let mut form = WaypointForm::of(&a_hut());
    edit(&mut form);
    form
}

// ── Names ────────────────────────────────────────────────────────────────────

#[test]
fn us104_the_pages_own_waypoint_is_named_on_its_map_but_opens_nothing() {
    let marker = own_marker(&a_hut());

    assert_eq!(marker.name.as_deref(), Some("Chamanna"));
    assert!(!marker.opens, "it would open the page it is on");
}

#[test]
fn us98_a_waypoint_without_a_name_is_called_unnamed() {
    let unnamed = trip_archive_types::Waypoint {
        name: None,
        ..a_hut()
    };

    assert_eq!(name_of(&a_hut()), "Chamanna");
    assert_eq!(name_of(&unnamed), "Unnamed waypoint");
}

// ── The edit form ────────────────────────────────────────────────────────────

#[test]
fn us98_the_form_starts_on_the_waypoint() {
    let form = WaypointForm::of(&a_hut());

    assert_eq!(form.name, "Chamanna");
    assert_eq!(form.description, "Open in summer");
    assert_eq!(form.symbol, "Lodge");
    assert_eq!(form.ele, "2065");
    let bare = trip_archive_types::Waypoint {
        name: None,
        description: None,
        ele: Some(868.5),
        symbol: None,
        ..a_hut()
    };
    let form = WaypointForm::of(&bare);
    assert_eq!((form.name.as_str(), form.description.as_str()), ("", ""));
    assert_eq!((form.symbol.as_str(), form.ele.as_str()), ("", "868.5"));
}

#[test]
fn us98_only_what_changed_is_sent() {
    let hut = a_hut();

    assert_eq!(
        changes(&hut, &WaypointForm::of(&hut)),
        Ok(WaypointEdit::default())
    );
    let edit = changes(
        &hut,
        &form_of(|form| {
            form.name = " Chamanna d'Es-cha ".to_string();
            form.symbol = "Summit".to_string();
        }),
    );
    assert_eq!(
        edit,
        Ok(WaypointEdit {
            name: Some("Chamanna d'Es-cha".to_string()),
            symbol: Some("Summit".to_string()),
            ..Default::default()
        })
    );
}

#[test]
fn us98_the_description_symbol_and_elevation_are_emptied_by_sending_them_blank() {
    let edit = changes(
        &a_hut(),
        &form_of(|form| {
            form.description = "  ".to_string();
            form.symbol = String::new();
            form.ele = String::new();
        }),
    );

    assert_eq!(
        edit,
        Ok(WaypointEdit {
            description: Some(String::new()),
            symbol: Some(String::new()),
            ele: Some(String::new()),
            ..Default::default()
        })
    );
}

#[test]
fn us98_the_name_is_required() {
    assert!(changes(&a_hut(), &form_of(|form| form.name = " ".to_string())).is_err());
    // Also for a waypoint the file gave none.
    let unnamed = trip_archive_types::Waypoint {
        name: None,
        ..a_hut()
    };
    assert!(changes(&unnamed, &WaypointForm::of(&unnamed)).is_err());
}

// ── The symbol chooser ───────────────────────────────────────────────────────

#[test]
fn us98_a_symbol_not_offered_is_kept_as_an_option_of_its_own() {
    assert_eq!(own_symbol("Shipwreck"), Some("Shipwreck".to_string()));
    // Spelled otherwise than Garmin does, it has an icon but no option.
    assert_eq!(own_symbol("summit"), Some("summit".to_string()));
    assert_eq!(own_symbol("Summit"), None);
    assert_eq!(own_symbol(""), None);
}

fn chooser(value: &str, own: Option<&str>) -> String {
    let value = value.to_string();
    let own = own.map(str::to_string);
    render(move || {
        rsx! { SymbolChooser { value: value.clone(), own: own.clone(), onchange: move |_| {} } }
    })
}

/// The option whose value is `value`, up to its label's end.
fn option<'a>(html: &'a str, value: &str) -> &'a str {
    let at = html
        .find(&format!(r#"value="{value}""#))
        .unwrap_or_else(|| panic!("no option {value}: {html}"));
    let start = html[..at].rfind("<label").unwrap();
    let end = at + html[at..].find("</label>").unwrap();
    &html[start..end]
}

#[test]
fn us98_every_symbol_is_offered_with_its_icon_and_none_too() {
    let html = chooser("Summit", None);

    let summit = option(&html, "Summit");
    assert!(summit.contains("checked"), "{summit}");
    assert!(summit.contains("waypoint-symbol"), "{summit}");
    assert!(option(&html, "Triangle, Blue").contains(r##"fill="#1f7fe0""##));
    let none = option(&html, "");
    assert!(none.contains("None"), "{none}");
    assert!(!none.contains("checked"), "{none}");
    for group in ["Symbols", "Shapes", "Navaids"] {
        assert!(html.contains(&format!(">{group}<")), "{group}: {html}");
    }
}

#[test]
fn us98_the_files_own_symbol_is_offered_and_chosen_until_another_is() {
    let html = chooser("Shipwreck", Some("Shipwreck"));

    let own = option(&html, "Shipwreck");
    assert!(own.contains("checked"), "{own}");
    assert!(own.contains("waypoint-mark"), "no icon: {own}");
}

// ── Deleting ─────────────────────────────────────────────────────────────────

#[test]
fn us98_deleting_is_confirmed_naming_the_waypoint() {
    let question = delete_question("Chamanna");

    assert!(question.contains("“Chamanna”"), "{question}");
    assert!(
        question.contains("collection and its tracks stay"),
        "{question}"
    );
    assert!(question.contains("cannot be undone"), "{question}");
}

// ── The page ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us98_the_page_shows_the_waypoint_its_collection_and_a_map() {
    let (archive, _dir) = serve_test_archive().await;
    let (collection, _) = import_collection(&archive, COLLECTION_GPX, &[("name", "Lakes")]).await;
    let pass = crate::api::list_collection_waypoints(&archive, collection)
        .await
        .unwrap()
        .remove(0);
    let id = pass.id;

    let html = render_against_archive(
        &archive,
        move || rsx! { Waypoint { id } },
        |html| html.contains("waypoint-name"),
    )
    .await;

    assert!(html.contains("San Bernardino Pass</h1>"), "{html}");
    assert!(
        html.contains(&format!(r#"href="/collections/{collection}?">Lakes</a>"#)),
        "{html}"
    );
    assert!(html.contains("Triangle, Blue"), "the symbol: {html}");
    assert!(html.contains("2065 m"), "{html}");
    assert!(
        html.contains(&format::position(pass.lat, pass.lon)),
        "{html}"
    );
    assert!(
        html.contains(r#"<div id="overview-map" class="overview-map"></div>"#),
        "{html}"
    );
    for control in ["edit-waypoint", "move-waypoint", "delete-waypoint"] {
        assert!(
            html.contains(&format!(r#"id="{control}""#)),
            "{control}: {html}"
        );
    }
}

#[tokio::test]
async fn us98_a_waypoint_that_no_longer_exists_says_so() {
    let (archive, _dir) = serve_test_archive().await;

    let html = render_against_archive(
        &archive,
        || rsx! { Waypoint { id: 999 } },
        |html| html.contains("error"),
    )
    .await;

    assert!(html.contains("no such waypoint"), "{html}");
}
