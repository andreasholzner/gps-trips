use super::*;

fn shape(path: &'static str, color: &'static str) -> Option<WaypointIcon> {
    Some(WaypointIcon::Shape {
        path,
        color,
        half: None,
    })
}

fn two_tone(first: &'static str, second: &'static str) -> Option<WaypointIcon> {
    Some(WaypointIcon::Shape {
        path: RHOMBUS,
        color: first,
        half: Some(Half {
            path: RHOMBUS_RIGHT_HALF,
            color: second,
        }),
    })
}

#[test]
fn us95_a_colored_symbol_is_its_shape_in_its_color() {
    assert_eq!(icon("Triangle, Blue"), shape(TRIANGLE, "#1f7fe0"));
    assert_eq!(icon("Diamond, Green"), shape(RHOMBUS, "#2e9a3e"));
    assert_eq!(icon("Letter B, Red"), shape(ALPHA_B_CIRCLE, "#e0322b"));
    assert_eq!(icon("Number 7, Blue"), shape(NUMERIC7_CIRCLE, "#1f7fe0"));
}

#[test]
fn us95_every_shape_is_closed_so_its_outline_goes_all_round() {
    let shapes = [
        "Circle",
        "Triangle",
        "Square",
        "Diamond",
        "Oval",
        "Rectangle",
        "Flag",
        "Pin",
    ]
    .into_iter()
    .chain(["Block", "Letter A", "Letter D", "Number 0", "Number 9"]);
    for shape in shapes {
        let Some(WaypointIcon::Shape { path, .. }) = icon(&format!("{shape}, Red")) else {
            panic!("{shape} is not a shape");
        };
        assert!(path.trim_end().ends_with(['Z', 'z']), "{shape}: {path}");
    }
}

#[test]
fn us95_a_named_symbol_is_a_glyph() {
    assert_eq!(
        icon("Summit"),
        Some(WaypointIcon::Glyph {
            path: IMAGE_FILTER_HDR
        })
    );
    // Garmin's names for one thing share its glyph.
    assert_eq!(icon("Hotel"), icon("Lodging"));
    // A flag without a color is still a flag.
    assert_eq!(icon("Flag"), Some(WaypointIcon::Glyph { path: FLAG }));
}

#[test]
fn us95_a_symbol_is_matched_ignoring_case_and_spacing() {
    assert_eq!(icon("flag,red"), icon("Flag, Red"));
    assert_eq!(icon("  Scenic   AREA "), icon("Scenic Area"));
    assert!(icon("Flag, Red").is_some());
}

#[test]
fn us95_a_symbol_the_archive_has_no_icon_for_keeps_the_marker() {
    for symbol in [
        // Garmin's own default.
        "Waypoint",
        // A navaid's two tones Garmin has not.
        "Navaid, Amber/Violet",
        "Navaid, Red/Red",
        "Contact, Pirate",
        // A known shape in a color Garmin has not, and the reverse.
        "Circle, Amber",
        "Star, Blue",
        "Shipwreck",
        "",
    ] {
        assert_eq!(icon(symbol), None, "{symbol:?}");
    }
}

#[test]
fn us95_every_symbol_of_the_alpencross_book_gets_an_icon() {
    // data/collections/Alpencross-light-Baukasten.gpx
    for symbol in [
        "Flag, Red",
        "Flag, Green",
        "Circle, Blue",
        "Circle, Red",
        "Triangle, Red",
        "Triangle, Blue",
    ] {
        assert!(icon(symbol).is_some(), "{symbol}");
    }
}

#[test]
fn us95_a_navaid_is_a_diamond_in_any_of_garmins_navaid_colors() {
    assert_eq!(icon("Navaid, Red"), shape(RHOMBUS, "#e0322b"));
    assert_eq!(icon("Navaid, Black"), shape(RHOMBUS, "#212121"));
    for color in ["Amber", "Blue", "Green", "Orange", "Violet", "White"] {
        assert!(icon(&format!("Navaid, {color}")).is_some(), "{color}");
    }
}

#[test]
fn us95_navaid_colors_other_shapes_lack_keep_the_marker() {
    // Garmin has amber, black, orange, violet and white only as navaids.
    for symbol in ["Circle, Black", "Triangle, Orange", "Flag, White"] {
        assert_eq!(icon(symbol), None, "{symbol}");
    }
}

#[test]
fn us95_a_two_tone_navaid_is_split_left_first_right_second() {
    assert_eq!(icon("Navaid, Green/Red"), two_tone("#2e9a3e", "#e0322b"));
    assert_eq!(icon("navaid, white / red"), two_tone("#f5f5f5", "#e0322b"));
    for pair in ["Green/White", "Red/Green", "Red/White", "White/Green"] {
        assert!(icon(&format!("Navaid, {pair}")).is_some(), "{pair}");
    }
}

#[test]
fn us95_the_right_half_is_closed_and_right_of_the_middle() {
    assert!(RHOMBUS_RIGHT_HALF.ends_with('Z'));
    // Absolute commands only, so every other number is an x.
    let numbers: Vec<f64> = RHOMBUS_RIGHT_HALF
        .split(|c: char| c.is_ascii_alphabetic() || c == ' ')
        .filter(|n| !n.is_empty())
        .map(|n| n.parse().unwrap())
        .collect();
    assert!(numbers.iter().step_by(2).all(|&x| x >= 12.0), "{numbers:?}");
}

#[test]
fn us95_circle_with_x_is_a_glyph() {
    assert_eq!(
        icon("Circle with X"),
        Some(WaypointIcon::Glyph { path: CLOSE })
    );
}

#[test]
fn us95_every_symbol_of_the_ostalpen_book_gets_an_icon() {
    // data/bug_collection/Alpencross-Ostalpen-Pässe-Digital.gpx
    for symbol in [
        "Navaid, Blue",
        "Navaid, Black",
        "Navaid, Green",
        "Navaid, Red",
        "Circle with X",
        "Triangle, Red",
        "Triangle, Blue",
        "Triangle, Green",
        "Flag, Red",
        "Flag, Green",
    ] {
        assert!(icon(symbol).is_some(), "{symbol}");
    }
}

#[test]
fn us95_a_two_tone_icon_reaches_the_script_with_its_half() {
    assert_eq!(
        serde_json::to_value(icon("Navaid, Red/White")).unwrap(),
        serde_json::json!({
            "kind": "shape",
            "path": RHOMBUS,
            "color": "#e0322b",
            "half": { "path": RHOMBUS_RIGHT_HALF, "color": "#f5f5f5" },
        })
    );
}

#[test]
fn us95_an_icon_reaches_the_script_tagged_by_how_it_is_drawn() {
    assert_eq!(
        serde_json::to_value(icon("Circle, Red")).unwrap(),
        serde_json::json!({ "kind": "shape", "path": CIRCLE, "color": "#e0322b" })
    );
    assert_eq!(
        serde_json::to_value(icon("Summit")).unwrap(),
        serde_json::json!({ "kind": "glyph", "path": IMAGE_FILTER_HDR })
    );
}

// ── US-98: the symbols to choose from ────────────────────────────────────────

fn all_symbols() -> Vec<String> {
    symbol_groups()
        .into_iter()
        .flat_map(|(_, symbols)| symbols)
        .collect()
}

#[test]
fn us98_every_symbol_offered_has_an_icon() {
    let symbols = all_symbols();

    assert!(!symbols.is_empty());
    for symbol in &symbols {
        assert!(icon(symbol).is_some(), "{symbol}");
    }
}

#[test]
fn us98_the_symbols_are_spelled_as_garmin_spells_them() {
    let symbols = all_symbols();

    for name in [
        "Summit",
        "RV Park",
        "Drinking Water",
        "Circle with X",
        "Triangle, Blue",
        "Letter A, Red",
        "Navaid, Amber",
        "Navaid, Red/White",
    ] {
        assert!(symbols.iter().any(|symbol| symbol == name), "{name}");
    }
    // A navaid's own colors are not offered on the other shapes, nor a
    // two-tone navaid in one color twice.
    assert!(!symbols.iter().any(|symbol| symbol == "Circle, Amber"));
    assert!(!symbols.iter().any(|symbol| symbol == "Navaid, Red/Red"));
}

#[test]
fn us98_the_symbols_come_in_named_groups_each_offered_once() {
    let groups = symbol_groups();
    let names: Vec<&str> = groups.iter().map(|(name, _)| *name).collect();

    assert_eq!(names, ["Symbols", "Shapes", "Navaids"]);
    let symbols = all_symbols();
    let unique: std::collections::BTreeSet<&String> = symbols.iter().collect();
    assert_eq!(unique.len(), symbols.len());
}
