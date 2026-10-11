//! Which icon a guide book's waypoint gets on a map (US-95), from the GPX
//! `<sym>` its file names; the script only draws it (ADR-0025).
//!
//! GPX leaves `<sym>` free text. The names are Garmin's, which its devices,
//! BaseCamp and QMapShack write — GPSBabel's `garmin_icon_tables.h` lists
//! them. A colored one is a shape and a color, `Triangle, Blue`, drawn as
//! that shape in that color; of the others, those a walking or riding book
//! uses get a glyph on a badge. Every other name keeps US-86's marker.
//!
//! The shapes and glyphs are Material Design Icons (Pictogrammers,
//! Apache-2.0, `@mdi/js` 7.4.47) on their 24 × 24 grid, as the activity
//! icons are.

use serde::Serialize;

/// How a waypoint's symbol is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum WaypointIcon {
    /// The shape itself, in the symbol's color.
    Shape {
        path: &'static str,
        color: &'static str,
    },
    /// A white glyph on a badge in the waypoint marker's slate.
    Glyph { path: &'static str },
}

/// The icon for `symbol`, matched ignoring case and spacing; `None` keeps
/// the plain marker.
pub fn icon(symbol: &str) -> Option<WaypointIcon> {
    match symbol.split_once(',') {
        Some((shape, color)) => Some(WaypointIcon::Shape {
            path: look_up(SHAPES, shape)?,
            color: look_up(COLORS, color)?,
        }),
        None => look_up(GLYPHS, symbol).map(|path| WaypointIcon::Glyph { path }),
    }
}

/// The value of the entry whose name is `text`'s words, ignoring case.
fn look_up(table: &[(&str, &'static str)], text: &str) -> Option<&'static str> {
    table
        .iter()
        .find(|(name, _)| same_words(text, name))
        .map(|&(_, value)| value)
}

/// Whether `text` holds `name`'s words, in any case and with any spacing.
fn same_words(text: &str, name: &str) -> bool {
    let mut words = text.split_whitespace();
    let mut names = name.split(' ');
    loop {
        match (words.next(), names.next()) {
            (Some(word), Some(name)) if word.eq_ignore_ascii_case(name) => {}
            (None, None) => return true,
            _ => return false,
        }
    }
}

/// Garmin's three colors, a little deeper than pure, to hold against the
/// map's tiles.
const COLORS: &[(&str, &str)] = &[
    ("red", "#e0322b"),
    ("green", "#2e9a3e"),
    ("blue", "#1f7fe0"),
];

const SHAPES: &[(&str, &str)] = &[
    ("circle", CIRCLE),
    ("triangle", TRIANGLE),
    ("square", SQUARE),
    ("diamond", RHOMBUS),
    ("oval", ELLIPSE),
    ("rectangle", RECTANGLE),
    ("flag", FLAG),
    ("pin", MAP_MARKER),
    ("block", SQUARE_ROUNDED),
    ("letter a", ALPHA_A_CIRCLE),
    ("letter b", ALPHA_B_CIRCLE),
    ("letter c", ALPHA_C_CIRCLE),
    ("letter d", ALPHA_D_CIRCLE),
    ("number 0", NUMERIC0_CIRCLE),
    ("number 1", NUMERIC1_CIRCLE),
    ("number 2", NUMERIC2_CIRCLE),
    ("number 3", NUMERIC3_CIRCLE),
    ("number 4", NUMERIC4_CIRCLE),
    ("number 5", NUMERIC5_CIRCLE),
    ("number 6", NUMERIC6_CIRCLE),
    ("number 7", NUMERIC7_CIRCLE),
    ("number 8", NUMERIC8_CIRCLE),
    ("number 9", NUMERIC9_CIRCLE),
];

const GLYPHS: &[(&str, &str)] = &[
    ("summit", IMAGE_FILTER_HDR),
    ("lodging", BED),
    ("hotel", BED),
    ("lodge", BED),
    ("residence", HOME),
    ("house", HOME),
    ("campground", TENT),
    ("rv park", TENT),
    ("drinking water", WATER),
    ("water source", WATER),
    ("scenic area", BINOCULARS),
    ("trail head", SIGN_DIRECTION),
    ("parking area", PARKING),
    ("restaurant", SILVERWARE_FORK_KNIFE),
    ("fast food", SILVERWARE_FORK_KNIFE),
    ("bar", BEER),
    ("information", INFORMATION),
    ("danger area", ALERT),
    ("church", CHURCH),
    ("bridge", BRIDGE),
    ("tunnel", TUNNEL),
    ("bike trail", BIKE),
    ("restroom", TOILET),
    ("shower", SHOWER),
    ("skiing area", SKI),
    ("ski resort", SKI),
    ("railway", TRAIN),
    ("funicular", GONDOLA),
    ("ferry", FERRY),
    ("medical facility", HOSPITAL),
    ("ground transportation", BUS),
    ("forest", PINE_TREE),
    ("swimming area", SWIM),
    ("picnic area", TABLE_FURNITURE),
    ("museum", BANK),
    ("post office", EMAIL),
    ("telephone", PHONE),
    ("gas station", GAS_STATION),
    ("convenience store", STORE),
    ("shopping center", CART),
    ("pharmacy", MEDICAL_BAG),
    ("flag", FLAG),
    ("geocache", TREASURE_CHEST),
    ("waterfall", WATERFALL),
];

// mdiCircle
const CIRCLE: &str = "M12,2A10,10 0 0,0 2,12A10,10 0 0,0 12,22A10,10 0 0,0 22,12A10,10 0 0,0 12,2Z";
// mdiTriangle
const TRIANGLE: &str = "M1,21H23L12,2Z";
// mdiSquare
const SQUARE: &str = "M3,3V21H21V3Z";
// mdiRhombus
const RHOMBUS: &str = "M12 2C11.5 2 11 2.19 10.59 2.59L2.59 10.59C1.8 11.37 1.8 12.63 2.59 13.41L10.59 21.41C11.37 22.2 12.63 22.2 13.41 21.41L21.41 13.41C22.2 12.63 22.2 11.37 21.41 10.59L13.41 2.59C13 2.19 12.5 2 12 2Z";
// mdiEllipse
const ELLIPSE: &str =
    "M12,4C6.5,4 2,7.58 2,12C2,16.42 6.5,20 12,20C17.5,20 22,16.42 22,12C22,7.58 17.5,4 12,4Z";
// mdiRectangle
const RECTANGLE: &str = "M4,6V19H20V6H4Z";
// mdiFlag
const FLAG: &str = "M14.4,6L14,4H5V21H7V14H12.6L13,16H20V6H14.4Z";
// mdiMapMarker
const MAP_MARKER: &str = "M12,11.5A2.5,2.5 0 0,1 9.5,9A2.5,2.5 0 0,1 12,6.5A2.5,2.5 0 0,1 14.5,9A2.5,2.5 0 0,1 12,11.5M12,2A7,7 0 0,0 5,9C5,14.25 12,22 12,22C12,22 19,14.25 19,9A7,7 0 0,0 12,2Z";
// mdiSquareRounded
const SQUARE_ROUNDED: &str = "M8 3H16C18.76 3 21 5.24 21 8V16C21 18.76 18.76 21 16 21H8C5.24 21 3 18.76 3 16V8C3 5.24 5.24 3 8 3Z";
// mdiAlphaACircle
const ALPHA_A_CIRCLE: &str = "M12,2A10,10 0 0,1 22,12A10,10 0 0,1 12,22A10,10 0 0,1 2,12A10,10 0 0,1 12,2M11,7A2,2 0 0,0 9,9V17H11V13H13V17H15V9A2,2 0 0,0 13,7H11M11,9H13V11H11V9Z";
// mdiAlphaBCircle
const ALPHA_B_CIRCLE: &str = "M12,2A10,10 0 0,1 22,12A10,10 0 0,1 12,22A10,10 0 0,1 2,12A10,10 0 0,1 12,2M15,10.5V9A2,2 0 0,0 13,7H9V17H13A2,2 0 0,0 15,15V13.5C15,12.7 14.3,12 13.5,12C14.3,12 15,11.3 15,10.5M13,15H11V13H13V15M13,11H11V9H13V11Z";
// mdiAlphaCCircle
const ALPHA_C_CIRCLE: &str = "M12,2A10,10 0 0,1 22,12A10,10 0 0,1 12,22A10,10 0 0,1 2,12A10,10 0 0,1 12,2M11,7A2,2 0 0,0 9,9V15A2,2 0 0,0 11,17H13A2,2 0 0,0 15,15V14H13V15H11V9H13V10H15V9A2,2 0 0,0 13,7H11Z";
// mdiAlphaDCircle
const ALPHA_D_CIRCLE: &str = "M12,2A10,10 0 0,1 22,12A10,10 0 0,1 12,22A10,10 0 0,1 2,12A10,10 0 0,1 12,2M9,7V17H13A2,2 0 0,0 15,15V9A2,2 0 0,0 13,7H9M11,9H13V15H11V9Z";
// mdiNumeric0Circle
const NUMERIC0_CIRCLE: &str = "M11,7A2,2 0 0,0 9,9V15A2,2 0 0,0 11,17H13A2,2 0 0,0 15,15V9A2,2 0 0,0 13,7H11M11,9H13V15H11V9M12,2A10,10 0 0,1 22,12A10,10 0 0,1 12,22A10,10 0 0,1 2,12A10,10 0 0,1 12,2Z";
// mdiNumeric1Circle
const NUMERIC1_CIRCLE: &str = "M10,7V9H12V17H14V7H10M12,2A10,10 0 0,1 22,12A10,10 0 0,1 12,22A10,10 0 0,1 2,12A10,10 0 0,1 12,2Z";
// mdiNumeric2Circle
const NUMERIC2_CIRCLE: &str = "M9,7V9H13V11H11A2,2 0 0,0 9,13V17H11L15,17V15H11V13H13A2,2 0 0,0 15,11V9A2,2 0 0,0 13,7H9M12,2A10,10 0 0,1 22,12A10,10 0 0,1 12,22A10,10 0 0,1 2,12A10,10 0 0,1 12,2Z";
// mdiNumeric3Circle
const NUMERIC3_CIRCLE: &str = "M15,15V13.5A1.5,1.5 0 0,0 13.5,12A1.5,1.5 0 0,0 15,10.5V9C15,7.89 14.1,7 13,7H9V9H13V11H11V13H13V15H9V17H13A2,2 0 0,0 15,15M12,2A10,10 0 0,1 22,12A10,10 0 0,1 12,22A10,10 0 0,1 2,12A10,10 0 0,1 12,2Z";
// mdiNumeric4Circle
const NUMERIC4_CIRCLE: &str = "M9,7V13H13V17H15V7H13V11H11V7H9M12,2A10,10 0 0,1 22,12A10,10 0 0,1 12,22A10,10 0 0,1 2,12A10,10 0 0,1 12,2Z";
// mdiNumeric5Circle
const NUMERIC5_CIRCLE: &str = "M9,7V13H13V15H9V17H13A2,2 0 0,0 15,15V13A2,2 0 0,0 13,11H11V9H15V7H9M12,2A10,10 0 0,1 22,12A10,10 0 0,1 12,22A10,10 0 0,1 2,12A10,10 0 0,1 12,2Z";
// mdiNumeric6Circle
const NUMERIC6_CIRCLE: &str = "M11,7A2,2 0 0,0 9,9V15A2,2 0 0,0 11,17H13A2,2 0 0,0 15,15V13A2,2 0 0,0 13,11H11V9H15V7H11M11,13H13V15H11V13M12,2A10,10 0 0,1 22,12A10,10 0 0,1 12,22A10,10 0 0,1 2,12A10,10 0 0,1 12,2Z";
// mdiNumeric7Circle
const NUMERIC7_CIRCLE: &str = "M11,17L15,9V7H9V9H13L9,17H11M12,2A10,10 0 0,1 22,12A10,10 0 0,1 12,22A10,10 0 0,1 2,12A10,10 0 0,1 12,2Z";
// mdiNumeric8Circle
const NUMERIC8_CIRCLE: &str = "M11,13H13V15H11V13M11,9H13V11H11V9M11,17H13A2,2 0 0,0 15,15V13.5A1.5,1.5 0 0,0 13.5,12A1.5,1.5 0 0,0 15,10.5V9C15,7.89 14.1,7 13,7H11A2,2 0 0,0 9,9V10.5A1.5,1.5 0 0,0 10.5,12A1.5,1.5 0 0,0 9,13.5V15C9,16.11 9.9,17 11,17M12,2A10,10 0 0,1 22,12A10,10 0 0,1 12,22A10,10 0 0,1 2,12A10,10 0 0,1 12,2Z";
// mdiNumeric9Circle
const NUMERIC9_CIRCLE: &str = "M12,2A10,10 0 0,1 22,12A10,10 0 0,1 12,22A10,10 0 0,1 2,12A10,10 0 0,1 12,2M13,17A2,2 0 0,0 15,15V9A2,2 0 0,0 13,7H11A2,2 0 0,0 9,9V11A2,2 0 0,0 11,13H13V15H9V17H13M13,11H11V9H13V11Z";
// mdiImageFilterHdr
const IMAGE_FILTER_HDR: &str =
    "M14,6L10.25,11L13.1,14.8L11.5,16C9.81,13.75 7,10 7,10L1,18H23L14,6Z";
// mdiBed
const BED: &str = "M19,7H11V14H3V5H1V20H3V17H21V20H23V11A4,4 0 0,0 19,7M7,13A3,3 0 0,0 10,10A3,3 0 0,0 7,7A3,3 0 0,0 4,10A3,3 0 0,0 7,13Z";
// mdiHome
const HOME: &str = "M10,20V14H14V20H19V12H22L12,3L2,12H5V20H10Z";
// mdiTent
const TENT: &str = "M4,6C4,7.19 4.39,8.27 5,9A3,3 0 0,1 2,6A3,3 0 0,1 5,3C4.39,3.73 4,4.81 4,6M2,21V19H4.76L12,4.78L19.24,19H22V21H2M12,9.19L7,19H17L12,9.19Z";
// mdiWater
const WATER: &str =
    "M12,20A6,6 0 0,1 6,14C6,10 12,3.25 12,3.25C12,3.25 18,10 18,14A6,6 0 0,1 12,20Z";
// mdiBinoculars
const BINOCULARS: &str = "M11,6H13V13H11V6M9,20A1,1 0 0,1 8,21H5A1,1 0 0,1 4,20V15L6,6H10V13A1,1 0 0,1 9,14V20M10,5H7V3H10V5M15,20V14A1,1 0 0,1 14,13V6H18L20,15V20A1,1 0 0,1 19,21H16A1,1 0 0,1 15,20M14,5V3H17V5H14Z";
// mdiSignDirection
const SIGN_DIRECTION: &str = "M11,12H3.5L6,9.5L3.5,7H11V3L12,2L13,3V7H18L20.5,9.5L18,12H13V20A2,2 0 0,1 15,22H9A2,2 0 0,1 11,20V12Z";
// mdiParking
const PARKING: &str = "M13.2,11H10V7H13.2A2,2 0 0,1 15.2,9A2,2 0 0,1 13.2,11M13,3H6V21H10V15H13A6,6 0 0,0 19,9C19,5.68 16.31,3 13,3Z";
// mdiSilverwareForkKnife
const SILVERWARE_FORK_KNIFE: &str = "M11,9H9V2H7V9H5V2H3V9C3,11.12 4.66,12.84 6.75,12.97V22H9.25V12.97C11.34,12.84 13,11.12 13,9V2H11V9M16,6V14H18.5V22H21V2C18.24,2 16,4.24 16,6Z";
// mdiBeer
const BEER: &str = "M4,2H19L17,22H6L4,2M6.2,4L7.8,20H8.8L7.43,6.34C8.5,6 9.89,5.89 11,7C12.56,8.56 15.33,7.69 16.5,7.23L16.8,4H6.2Z";
// mdiInformation
const INFORMATION: &str = "M13,9H11V7H13M13,17H11V11H13M12,2A10,10 0 0,0 2,12A10,10 0 0,0 12,22A10,10 0 0,0 22,12A10,10 0 0,0 12,2Z";
// mdiAlert
const ALERT: &str = "M13 14H11V9H13M13 18H11V16H13M1 21H23L12 2L1 21Z";
// mdiChurch
const CHURCH: &str = "M18 12.22V9L13 6.5V5H15V3H13V1H11V3H9V5H11V6.5L6 9V12.22L2 14V22H10V19C10 17.9 10.9 17 12 17S14 17.9 14 19V22H22V14L18 12.22M12 13.5C11.17 13.5 10.5 12.83 10.5 12S11.17 10.5 12 10.5 13.5 11.17 13.5 12 12.83 13.5 12 13.5Z";
// mdiBridge
const BRIDGE: &str = "M7,14V10.91C6.28,10.58 5.61,10.18 5,9.71V14H7M5,18H3V16H1V14H3V7H5V8.43C6.8,10 9.27,11 12,11C14.73,11 17.2,10 19,8.43V7H21V14H23V16H21V18H19V16H5V18M17,10.91V14H19V9.71C18.39,10.18 17.72,10.58 17,10.91M16,14V11.32C15.36,11.55 14.69,11.72 14,11.84V14H16M13,14V11.96L12,12L11,11.96V14H13M10,14V11.84C9.31,11.72 8.64,11.55 8,11.32V14H10Z";
// mdiTunnel
const TUNNEL: &str = "M12 2C6.5 2 2 6.5 2 12V22H22V12C22 6.5 17.5 2 12 2M7.1 5.69C8.21 4.83 9.54 4.25 11 4.07V6.09C10.09 6.24 9.25 6.6 8.53 7.11L7.1 5.69M15.47 7.11C14.75 6.6 13.91 6.24 13 6.09V4.07C14.46 4.25 15.79 4.83 16.9 5.69L15.47 7.11M5.69 7.1L7.11 8.53C6.6 9.25 6.24 10.09 6.09 11H4.07C4.25 9.54 4.83 8.21 5.69 7.1M6 13V15.5H4V13H6M4 20V17.5H6V20H4M16.89 8.53L18.31 7.1C19.17 8.21 19.75 9.54 19.93 11H17.91C17.76 10.09 17.4 9.25 16.89 8.53M18 13H20V15.5H18V13M18 20V17.5H20V20H18Z";
// mdiBike
const BIKE: &str = "M5,20.5A3.5,3.5 0 0,1 1.5,17A3.5,3.5 0 0,1 5,13.5A3.5,3.5 0 0,1 8.5,17A3.5,3.5 0 0,1 5,20.5M5,12A5,5 0 0,0 0,17A5,5 0 0,0 5,22A5,5 0 0,0 10,17A5,5 0 0,0 5,12M14.8,10H19V8.2H15.8L13.86,4.93C13.57,4.43 13,4.1 12.4,4.1C11.93,4.1 11.5,4.29 11.2,4.6L7.5,8.29C7.19,8.6 7,9 7,9.5C7,10.13 7.33,10.66 7.85,10.97L11.2,13V18H13V11.5L10.75,9.85L13.07,7.5M19,20.5A3.5,3.5 0 0,1 15.5,17A3.5,3.5 0 0,1 19,13.5A3.5,3.5 0 0,1 22.5,17A3.5,3.5 0 0,1 19,20.5M19,12A5,5 0 0,0 14,17A5,5 0 0,0 19,22A5,5 0 0,0 24,17A5,5 0 0,0 19,12M16,4.8C17,4.8 17.8,4 17.8,3C17.8,2 17,1.2 16,1.2C15,1.2 14.2,2 14.2,3C14.2,4 15,4.8 16,4.8Z";
// mdiToilet
const TOILET: &str = "M9,22H17V19.5C19.41,17.87 21,15.12 21,12V4A2,2 0 0,0 19,2H15C13.89,2 13,2.9 13,4V12H3C3,15.09 5,18 9,19.5V22M5.29,14H18.71C18.14,15.91 16.77,17.5 15,18.33V20H11V18.33C9,18 5.86,15.91 5.29,14M15,4H19V12H15V4M16,5V8H18V5H16Z";
// mdiShower
const SHOWER: &str = "M21,14V15C21,16.91 19.93,18.57 18.35,19.41L19,22H17L16.5,20C16.33,20 16.17,20 16,20H8C7.83,20 7.67,20 7.5,20L7,22H5L5.65,19.41C4.07,18.57 3,16.91 3,15V14H2V12H20V5A1,1 0 0,0 19,4C18.5,4 18.12,4.34 18,4.79C18.63,5.33 19,6.13 19,7H13A3,3 0 0,1 16,4C16.06,4 16.11,4 16.17,4C16.58,2.84 17.69,2 19,2A3,3 0 0,1 22,5V14H21V14M19,14H5V15A3,3 0 0,0 8,18H16A3,3 0 0,0 19,15V14Z";
// mdiSki
const SKI: &str = "M17.92 13.32C17.67 13.28 16.71 13 16.46 12.89L14.39 19.37L11.3 18.24L13.5 12.47L10.45 9L13 7.54C13.45 8.67 14.17 9.62 15.12 10.4S17.16 11.67 18.38 11.86L19.5 8.43L18.06 7.96L17.54 9.56C16.88 9.28 16.3 8.86 15.8 8.32C15.3 7.77 14.94 7.13 14.72 6.41L14.39 5.33C14.27 4.93 14.04 4.61 13.71 4.37C13.38 4.14 13 4 12.63 3.97C12.24 3.94 11.86 4 11.5 4.21L8 6.23C7.63 6.44 7.36 6.74 7.19 7.12C7 7.5 6.96 7.88 7 8.29S7.26 9.06 7.54 9.37L11.11 13.08L9.42 17.54L2.47 15.05L2 16.46L16.04 21.58C16.82 21.86 17.65 22 18.53 22C19.15 22 19.76 21.92 20.36 21.77C20.95 21.61 21.5 21.39 22 21.11L20.87 20C20.12 20.33 19.34 20.5 18.53 20.5C17.87 20.5 17.21 20.39 16.55 20.17L15.8 19.89L17.92 13.32M19 3C19 4.11 18.11 5 17 5S15 4.11 15 3 15.9 1 17 1 19 1.9 19 3Z";
// mdiTrain
const TRAIN: &str = "M12,2C8,2 4,2.5 4,6V15.5A3.5,3.5 0 0,0 7.5,19L6,20.5V21H8.23L10.23,19H14L16,21H18V20.5L16.5,19A3.5,3.5 0 0,0 20,15.5V6C20,2.5 16.42,2 12,2M7.5,17A1.5,1.5 0 0,1 6,15.5A1.5,1.5 0 0,1 7.5,14A1.5,1.5 0 0,1 9,15.5A1.5,1.5 0 0,1 7.5,17M11,10H6V6H11V10M13,10V6H18V10H13M16.5,17A1.5,1.5 0 0,1 15,15.5A1.5,1.5 0 0,1 16.5,14A1.5,1.5 0 0,1 18,15.5A1.5,1.5 0 0,1 16.5,17Z";
// mdiGondola
const GONDOLA: &str = "M18,10H13V7.59L22.12,6.07L21.88,4.59L16.41,5.5C16.46,5.35 16.5,5.18 16.5,5A1.5,1.5 0 0,0 15,3.5A1.5,1.5 0 0,0 13.5,5C13.5,5.35 13.63,5.68 13.84,5.93L13,6.07V5H11V6.41L10.41,6.5C10.46,6.35 10.5,6.18 10.5,6A1.5,1.5 0 0,0 9,4.5A1.5,1.5 0 0,0 7.5,6C7.5,6.36 7.63,6.68 7.83,6.93L1.88,7.93L2.12,9.41L11,7.93V10H6C4.89,10 4,10.9 4,12V18A2,2 0 0,0 6,20H18A2,2 0 0,0 20,18V12A2,2 0 0,0 18,10M6,12H8.25V16H6V12M9.75,16V12H14.25V16H9.75M18,16H15.75V12H18V16Z";
// mdiFerry
const FERRY: &str = "M6,6H18V9.96L12,8L6,9.96M3.94,19H4C5.6,19 7,18.12 8,17C9,18.12 10.4,19 12,19C13.6,19 15,18.12 16,17C17,18.12 18.4,19 20,19H20.05L21.95,12.31C22.03,12.06 22,11.78 21.89,11.54C21.76,11.3 21.55,11.12 21.29,11.04L20,10.62V6C20,4.89 19.1,4 18,4H15V1H9V4H6A2,2 0 0,0 4,6V10.62L2.71,11.04C2.45,11.12 2.24,11.3 2.11,11.54C2,11.78 1.97,12.06 2.05,12.31M20,21C18.61,21 17.22,20.53 16,19.67C13.56,21.38 10.44,21.38 8,19.67C6.78,20.53 5.39,21 4,21H2V23H4C5.37,23 6.74,22.65 8,22C10.5,23.3 13.5,23.3 16,22C17.26,22.65 18.62,23 20,23H22V21H20Z";
// mdiHospital
const HOSPITAL: &str = "M18 14H14V18H10V14H6V10H10V6H14V10H18";
// mdiBus
const BUS: &str = "M18,11H6V6H18M16.5,17A1.5,1.5 0 0,1 15,15.5A1.5,1.5 0 0,1 16.5,14A1.5,1.5 0 0,1 18,15.5A1.5,1.5 0 0,1 16.5,17M7.5,17A1.5,1.5 0 0,1 6,15.5A1.5,1.5 0 0,1 7.5,14A1.5,1.5 0 0,1 9,15.5A1.5,1.5 0 0,1 7.5,17M4,16C4,16.88 4.39,17.67 5,18.22V20A1,1 0 0,0 6,21H7A1,1 0 0,0 8,20V19H16V20A1,1 0 0,0 17,21H18A1,1 0 0,0 19,20V18.22C19.61,17.67 20,16.88 20,16V6C20,2.5 16.42,2 12,2C7.58,2 4,2.5 4,6V16Z";
// mdiPineTree
const PINE_TREE: &str = "M10,21V18H3L8,13H5L10,8H7L12,3L17,8H14L19,13H16L21,18H14V21H10Z";
// mdiSwim
const SWIM: &str = "M2,18C4.22,17 6.44,16 8.67,16C10.89,16 13.11,18 15.33,18C17.56,18 19.78,16 22,16V19C19.78,19 17.56,21 15.33,21C13.11,21 10.89,19 8.67,19C6.44,19 4.22,20 2,21V18M8.67,13C7.89,13 7.12,13.12 6.35,13.32L11.27,9.88L10.23,8.64C10.09,8.47 10,8.24 10,8C10,7.66 10.17,7.35 10.44,7.17L16.16,3.17L17.31,4.8L12.47,8.19L17.7,14.42C16.91,14.75 16.12,15 15.33,15C13.11,15 10.89,13 8.67,13M18,7A2,2 0 0,1 20,9A2,2 0 0,1 18,11A2,2 0 0,1 16,9A2,2 0 0,1 18,7Z";
// mdiTableFurniture
const TABLE_FURNITURE: &str =
    "M2 7H22V10H20L21 19H18.5L17.94 14H6.06L5.5 19H3L4 10H2V7M17.5 10H6.5L6.29 12H17.71L17.5 10Z";
// mdiBank
const BANK: &str = "M11.5,1L2,6V8H21V6M16,10V17H19V10M2,22H21V19H2M10,10V17H13V10M4,10V17H7V10H4Z";
// mdiEmail
const EMAIL: &str = "M20,8L12,13L4,8V6L12,11L20,6M20,4H4C2.89,4 2,4.89 2,6V18A2,2 0 0,0 4,20H20A2,2 0 0,0 22,18V6C22,4.89 21.1,4 20,4Z";
// mdiPhone
const PHONE: &str = "M6.62,10.79C8.06,13.62 10.38,15.94 13.21,17.38L15.41,15.18C15.69,14.9 16.08,14.82 16.43,14.93C17.55,15.3 18.75,15.5 20,15.5A1,1 0 0,1 21,16.5V20A1,1 0 0,1 20,21A17,17 0 0,1 3,4A1,1 0 0,1 4,3H7.5A1,1 0 0,1 8.5,4C8.5,5.25 8.7,6.45 9.07,7.57C9.18,7.92 9.1,8.31 8.82,8.59L6.62,10.79Z";
// mdiGasStation
const GAS_STATION: &str = "M18,10A1,1 0 0,1 17,9A1,1 0 0,1 18,8A1,1 0 0,1 19,9A1,1 0 0,1 18,10M12,10H6V5H12M19.77,7.23L19.78,7.22L16.06,3.5L15,4.56L17.11,6.67C16.17,7 15.5,7.93 15.5,9A2.5,2.5 0 0,0 18,11.5C18.36,11.5 18.69,11.42 19,11.29V18.5A1,1 0 0,1 18,19.5A1,1 0 0,1 17,18.5V14C17,12.89 16.1,12 15,12H14V5C14,3.89 13.1,3 12,3H6C4.89,3 4,3.89 4,5V21H14V13.5H15.5V18.5A2.5,2.5 0 0,0 18,21A2.5,2.5 0 0,0 20.5,18.5V9C20.5,8.31 20.22,7.68 19.77,7.23Z";
// mdiStore
const STORE: &str = "M12,18H6V14H12M21,14V12L20,7H4L3,12V14H4V20H14V14H18V20H20V14M20,4H4V6H20V4Z";
// mdiCart
const CART: &str = "M17,18C15.89,18 15,18.89 15,20A2,2 0 0,0 17,22A2,2 0 0,0 19,20C19,18.89 18.1,18 17,18M1,2V4H3L6.6,11.59L5.24,14.04C5.09,14.32 5,14.65 5,15A2,2 0 0,0 7,17H19V15H7.42A0.25,0.25 0 0,1 7.17,14.75C7.17,14.7 7.18,14.66 7.2,14.63L8.1,13H15.55C16.3,13 16.96,12.58 17.3,11.97L20.88,5.5C20.95,5.34 21,5.17 21,5A1,1 0 0,0 20,4H5.21L4.27,2M7,18C5.89,18 5,18.89 5,20A2,2 0 0,0 7,22A2,2 0 0,0 9,20C9,18.89 8.1,18 7,18Z";
// mdiMedicalBag
const MEDICAL_BAG: &str = "M10,3L8,5V7H5C3.85,7 3.12,8 3,9L2,19C1.88,20 2.54,21 4,21H20C21.46,21 22.12,20 22,19L21,9C20.88,8 20.06,7 19,7H16V5L14,3H10M10,5H14V7H10V5M11,10H13V13H16V15H13V18H11V15H8V13H11V10Z";
// mdiTreasureChest
const TREASURE_CHEST: &str = "M5,4H19A3,3 0 0,1 22,7V11H15V10H9V11H2V7A3,3 0 0,1 5,4M11,11H13V13H11V11M2,12H9V13L11,15H13L15,13V12H22V20H2V12Z";
// mdiWaterfall
const WATERFALL: &str = "M20 20C18.61 20 17.22 19.53 16 18.67C13.56 20.38 10.44 20.38 8 18.67C6.78 19.53 5.39 20 4 20H2V22H4C5.37 22 6.74 21.65 8 21C10.5 22.3 13.5 22.3 16 21C17.26 21.65 18.62 22 20 22H22V20M20 16C18.61 16 17.22 15.53 16 14.67C13.56 16.38 10.44 16.38 8 14.67C6.78 15.53 5.39 16 4 16H2V18H4C5.37 18 6.74 17.65 8 17C10.5 18.3 13.5 18.3 16 17C17.26 17.65 18.62 18 20 18H22V16M22 2H2V4H6V16H18V4H22M9 4H11V10H9M13 8H15V14H13Z";

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(path: &'static str, color: &'static str) -> Option<WaypointIcon> {
        Some(WaypointIcon::Shape { path, color })
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
            "Navaid, Amber",
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
}
