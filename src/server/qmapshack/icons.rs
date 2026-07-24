//! Per-activity track color and item icon (US-36). Colors are names from
//! QMapShack's own color table (`IGisItem::init()`); the PNGs under `icons/`
//! were extracted from real QMapShack-written items of the matching color
//! (ADR-0022: reuse real icons rather than generating new artwork).

use crate::models::ActivityType;

/// The QMapShack color name (`trk.color`) for a trip's activity type.
pub fn color(activity: ActivityType) -> &'static str {
    match activity {
        ActivityType::Unknown => "DarkGray",
        ActivityType::Hiking => "DarkRed",
        ActivityType::Mountaineering => "Red",
        ActivityType::Cycling => "DarkBlue",
        ActivityType::Bikepacking => "Blue",
        ActivityType::Kayaking => "DarkCyan",
        ActivityType::SkiTouring => "DarkMagenta",
        ActivityType::CrossCountrySkiing => "Magenta",
        ActivityType::SnowShoe => "DarkGreen",
    }
}

/// The `items.icon` PNG bytes for a trip's activity type — the track-line
/// icon QMapShack itself renders for that color.
pub fn icon_png(activity: ActivityType) -> &'static [u8] {
    match activity {
        ActivityType::Unknown => include_bytes!("icons/track_DarkGray.png"),
        ActivityType::Hiking => include_bytes!("icons/track_DarkRed.png"),
        ActivityType::Mountaineering => include_bytes!("icons/track_Red.png"),
        ActivityType::Cycling => include_bytes!("icons/track_DarkBlue.png"),
        ActivityType::Bikepacking => include_bytes!("icons/track_Blue.png"),
        ActivityType::Kayaking => include_bytes!("icons/track_DarkCyan.png"),
        ActivityType::SkiTouring => include_bytes!("icons/track_DarkMagenta.png"),
        ActivityType::CrossCountrySkiing => include_bytes!("icons/track_Magenta.png"),
        ActivityType::SnowShoe => include_bytes!("icons/track_DarkGreen.png"),
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// QMapShack's color table (`IGisItem::init()`), the only valid
    /// `trk.color` names.
    const QMAPSHACK_COLORS: [&str; 17] = [
        "Black",
        "DarkRed",
        "DarkGreen",
        "DarkYellow",
        "DarkBlue",
        "DarkMagenta",
        "DarkCyan",
        "LightGray",
        "DarkGray",
        "Red",
        "Green",
        "Yellow",
        "Blue",
        "Magenta",
        "Cyan",
        "White",
        "Transparent",
    ];

    /// Every ActivityType — exhaustively matched in `color()`/`icon_png()`,
    /// so a new variant fails to compile there; this list only needs to
    /// cover the current set for the value assertions below.
    const ALL: [ActivityType; 9] = [
        ActivityType::Unknown,
        ActivityType::Hiking,
        ActivityType::Mountaineering,
        ActivityType::Cycling,
        ActivityType::Bikepacking,
        ActivityType::Kayaking,
        ActivityType::SkiTouring,
        ActivityType::CrossCountrySkiing,
        ActivityType::SnowShoe,
    ];

    #[test]
    fn every_activity_maps_to_a_qmapshack_color_name() {
        for activity in ALL {
            assert!(
                QMAPSHACK_COLORS.contains(&color(activity)),
                "{activity}: {} is not a QMapShack color",
                color(activity)
            );
        }
    }

    #[test]
    fn every_activity_icon_is_png_bytes() {
        for activity in ALL {
            let png = icon_png(activity);
            assert_eq!(&png[..4], b"\x89PNG", "{activity} icon must be a PNG");
        }
    }

    #[test]
    fn icon_matches_the_activity_color() {
        // The icon assets are named per color — spot-check the pairing holds
        // for a couple of distinct activities.
        assert_eq!(
            icon_png(ActivityType::Hiking),
            include_bytes!("icons/track_DarkRed.png")
        );
        assert_ne!(
            icon_png(ActivityType::Hiking),
            icon_png(ActivityType::Cycling)
        );
    }
}
