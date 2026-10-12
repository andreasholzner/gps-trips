//! A waypoint's icon outside a map (US-98): beside its name in its
//! collection's list, on its page and in the symbol chooser — drawn as the
//! map script draws it (`interop/waypoints.rs`), from the same paths.

use dioxus::prelude::*;

use super::{icon, WaypointIcon};

/// The plain marker: a diamond, drawn rather than turned (US-86). The map
/// script draws the same path.
pub const WAYPOINT_MARK: &str = "M12,1L23,12L12,23L1,12Z";

/// The icon of `symbol`, or the plain marker for none or one without an
/// icon. Decoration only: the name beside it says what it is.
#[component]
pub fn WaypointSymbol(symbol: Option<String>) -> Element {
    let icon = symbol.as_deref().and_then(icon);
    let class = if icon.is_some() {
        "waypoint-icon waypoint-symbol"
    } else {
        "waypoint-icon waypoint-mark"
    };
    rsx! {
        span { class, aria_hidden: "true",
            svg { view_box: "0 0 24 24",
                match icon {
                    None => rsx! { path { class: "mark", d: WAYPOINT_MARK } },
                    Some(WaypointIcon::Glyph { path }) => rsx! {
                        circle { class: "badge", cx: "12", cy: "12", r: "12" }
                        path {
                            class: "glyph",
                            d: path,
                            transform: "translate(12 12) scale(0.62) translate(-12 -12)",
                        }
                    },
                    Some(WaypointIcon::Shape { path, color, half }) => rsx! {
                        path { class: "shape", d: path, fill: color }
                        if let Some(half) = half {
                            path { class: "half", d: half.path, fill: half.color }
                        }
                    },
                }
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    fn drawn(symbol: Option<&str>) -> String {
        let symbol = symbol.map(str::to_string);
        render(move || rsx! { WaypointSymbol { symbol: symbol.clone() } })
    }

    #[test]
    fn us98_no_symbol_or_one_without_an_icon_is_the_plain_marker() {
        for symbol in [None, Some("Shipwreck")] {
            let html = drawn(symbol);
            assert!(html.contains("waypoint-mark"), "{html}");
            assert!(html.contains(&format!(r#"d="{WAYPOINT_MARK}""#)), "{html}");
        }
    }

    #[test]
    fn us98_a_glyph_sits_on_its_badge() {
        let html = drawn(Some("Summit"));

        assert!(html.contains("waypoint-symbol"), "{html}");
        assert!(html.contains(r#"class="badge""#), "{html}");
        assert!(html.contains(r#"class="glyph""#), "{html}");
    }

    #[test]
    fn us98_a_shape_is_in_its_color_and_a_two_tone_one_has_its_half() {
        let html = drawn(Some("Triangle, Blue"));
        assert!(html.contains(r##"fill="#1f7fe0""##), "{html}");
        assert!(!html.contains("half"), "{html}");

        let html = drawn(Some("Navaid, Red/White"));
        assert!(html.contains(r#"class="half""#), "{html}");
    }

    #[test]
    fn us98_the_icon_is_decoration_beside_its_name() {
        assert!(drawn(None).contains(r#"aria-hidden="true""#));
    }
}
