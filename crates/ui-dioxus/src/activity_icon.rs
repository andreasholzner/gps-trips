//! Each activity as an icon in its map color (US-77), where a column of
//! activity names would take a phone's screen.
//!
//! The shapes are Material Design Icons (Pictogrammers, Apache-2.0,
//! `@mdi/js` 7.4.47) on their 24 × 24 grid, two of them with parts added,
//! and one Font Awesome Free icon (7.3.1, CC BY 4.0, by Fonticons, Inc. —
//! <https://fontawesome.com/license/free>), scaled onto the same grid. An
//! icon is one or more shapes, so the added parts are not cut out of the
//! shape they overlap.

use dioxus::prelude::*;
use trip_archive_types::ActivityType;

use crate::activity_color;

/// The activities together: a sum sign (`mdiSigma`).
const SUM: &[&str] = &["M18,6H8.83L14.83,12L8.83,18H18V20H6V18L12,12L6,6V4H18V6Z"];

/// The icon's shapes.
fn paths(activity: ActivityType) -> &'static [&'static str] {
    match activity {
        // mdiHelpCircleOutline
        ActivityType::Unknown => &["M11,18H13V16H11V18M12,2A10,10 0 0,0 2,12A10,10 0 0,0 12,22A10,10 0 0,0 22,12A10,10 0 0,0 12,2M12,20C7.59,20 4,16.41 4,12C4,7.59 7.59,4 12,4C16.41,4 20,7.59 20,12C20,16.41 16.41,20 12,20M12,6A4,4 0 0,0 8,10H10A2,2 0 0,1 12,8A2,2 0 0,1 14,10C14,12 11,11.75 11,15H13C13,12.75 16,12.5 16,10A4,4 0 0,0 12,6Z"],
        // mdiHiking
        ActivityType::Hiking => &["M17.47 8.67H19V23H17.47V12.6C16.67 12.44 15.92 12.14 15.21 11.71S13.9 10.78 13.39 10.2L12.77 13.27L15 15.47V23H13V17L10.76 14.8L8.89 23H6.73C6.73 23 9.86 7.22 9.89 7.09C10 6.61 10.22 6.24 10.59 6C10.96 5.73 11.33 5.6 11.71 5.6C12.1 5.6 12.46 5.69 12.79 5.87C13.13 6.04 13.39 6.29 13.58 6.61L14.64 8.24C14.93 8.78 15.32 9.25 15.81 9.63S16.86 10.3 17.47 10.5V8.67M8.55 5.89L7.4 5.65C6.83 5.5 6.31 5.62 5.84 5.94C5.38 6.26 5.1 6.7 5 7.28L4.19 11.26C4.16 11.55 4.22 11.81 4.38 12.05C4.54 12.29 4.75 12.42 5 12.46L7.21 12.89L8.55 5.89M13 1C11.9 1 11 1.9 11 3S11.9 5 13 5 15 4.11 15 3 14.11 1 13 1Z"],
        // mdiImageFilterHdr
        ActivityType::Mountaineering => &["M14,6L10.25,11L13.1,14.8L11.5,16C9.81,13.75 7,10 7,10L1,18H23L14,6Z"],
        // mdiBike
        ActivityType::Cycling => &["M5,20.5A3.5,3.5 0 0,1 1.5,17A3.5,3.5 0 0,1 5,13.5A3.5,3.5 0 0,1 8.5,17A3.5,3.5 0 0,1 5,20.5M5,12A5,5 0 0,0 0,17A5,5 0 0,0 5,22A5,5 0 0,0 10,17A5,5 0 0,0 5,12M14.8,10H19V8.2H15.8L13.86,4.93C13.57,4.43 13,4.1 12.4,4.1C11.93,4.1 11.5,4.29 11.2,4.6L7.5,8.29C7.19,8.6 7,9 7,9.5C7,10.13 7.33,10.66 7.85,10.97L11.2,13V18H13V11.5L10.75,9.85L13.07,7.5M19,20.5A3.5,3.5 0 0,1 15.5,17A3.5,3.5 0 0,1 19,13.5A3.5,3.5 0 0,1 22.5,17A3.5,3.5 0 0,1 19,20.5M19,12A5,5 0 0,0 14,17A5,5 0 0,0 19,22A5,5 0 0,0 24,17A5,5 0 0,0 19,12M16,4.8C17,4.8 17.8,4 17.8,3C17.8,2 17,1.2 16,1.2C15,1.2 14.2,2 14.2,3C14.2,4 15,4.8 16,4.8Z"],
        // mdiBicycle with a saddle bag, a frame bag and a handlebar roll
        ActivityType::Bikepacking => &[
            "M19 10C18.44 10 17.91 10.11 17.41 10.28L14.46 4.5H11V6H13.54L14.42 7.72L12 13.13L10.23 8.95C10.5 8.85 10.74 8.58 10.74 8.25C10.74 7.84 10.41 7.5 10 7.5H8C7.58 7.5 7.24 7.84 7.24 8.25S7.58 9 8 9H8.61L10.86 14.25H9.92C9.56 11.85 7.5 10 5 10C2.24 10 0 12.24 0 15S2.24 20 5 20C7.5 20 9.56 18.15 9.92 15.75H12.5L15.29 9.43L16.08 10.96C14.82 11.87 14 13.34 14 15C14 17.76 16.24 20 19 20S24 17.76 24 15 21.76 10 19 10M5 18.5C3.07 18.5 1.5 16.93 1.5 15S3.07 11.5 5 11.5C6.67 11.5 8.07 12.68 8.41 14.25H4V15.75H8.41C8.07 17.32 6.67 18.5 5 18.5M19 18.5C17.07 18.5 15.5 16.93 15.5 15C15.5 13.92 16 12.97 16.77 12.33L18.57 15.85L19.89 15.13L18.1 11.63C18.39 11.56 18.69 11.5 19 11.5C20.93 11.5 22.5 13.07 22.5 15S20.93 18.5 19 18.5Z",
            "M7.4 7.4L2.2 6.2C1.3 6 .7 6.8 1.1 7.6L1.9 9.2C2.2 9.8 2.8 10.1 3.5 10L7.6 9.3Z",
            "M10.9 9.4L14 8.6L12.15 12.6Z",
            "M15.3 4.3H17.3A1.5 1.5 0 0 1 17.3 7.3H15.3A1.5 1.5 0 0 1 15.3 4.3Z",
        ],
        // mdiKayaking
        ActivityType::Kayaking => &["M22 23V21C20.58 21.05 19.21 20.9 18 20C16.23 21.25 13.77 21.25 12 20C10.23 21.25 7.77 21.25 6 20C4.79 20.9 3.42 21.05 2 21V23C3.38 23.05 4.79 22.94 6 22.25C7.84 23.25 10.16 23.25 12 22.25C13.84 23.25 16.16 23.25 18 22.25C19.21 22.94 20.62 23.05 22 23M23.39 17.21C21.13 16.29 18.3 15.56 15.66 15.22L19.36 6.88L20.67 6.41L22 3.41L18.8 2L17.5 4.95L18 6.27L16.31 9.97L13.5 11.47L11 10.2C10.95 10.14 10.86 10.09 10.74 10.06C9.82 9.73 8.77 10.32 8.5 11.23L7.13 15.41C6.59 15.18 1.25 16.97 .915 17.12L0 17.47C1.33 18.04 2.2 18.39 3.94 18.88C4.75 18.63 5.44 18.09 6 17.47C7.5 19.42 10.5 19.42 12 17.47C13.5 19.42 16.5 19.42 18 17.47C18.56 18.09 19.25 18.63 20.06 18.88L22.95 17.91L24 17.47L23.39 17.21M14.06 15.08C13.07 15 12.06 15 11.06 15L11.77 12.83L13.5 13.77L15 12.92L14.06 15.08M14 7.5C14 8.61 13.11 9.5 12 9.5S10 8.61 10 7.5 10.9 5.5 12 5.5 14 6.4 14 7.5Z"],
        // mdiSkiCrossCountry, scaled down, on skis, hauling a pulk
        ActivityType::SkiTouring => &[
            "M21.02 11.52H19.928V17.76H21.02V11.52M11.223 17.76H10.1L11.66 11.52H12.783L11.223 17.76M13.033 8.041V10.74H11.66V7.09L15.256 5.522C15.591 5.389 15.95 5.381 16.309 5.491S16.956 5.818 17.167 6.138L17.9 7.292C18.189 7.815 18.602 8.361 19.164 8.689C19.717 9.016 20.341 9.18 21.02 9.18V10.607C20.24 10.607 19.46 10.444 18.781 10.116S17.51 9.227 17.026 8.689L16.582 10.865L18.118 12.3V17.76H16.652V13.47L15.17 11.949L13.821 17.76H12.284L14.335 7.534L13.033 8.041M18.68 3.72C18.68 4.586 17.986 5.28 17.12 5.28S15.56 4.586 15.56 3.72 16.262 2.16 17.12 2.16 18.68 2.862 18.68 3.72Z",
            "M9 17.9H21.4C22.1 17.9 22.6 17.5 22.8 16.9L23.7 17.2C23.4 18.3 22.5 19 21.4 19H9Z",
            "M0.4 17.9C0.4 16.4 1.1 15.5 2.5 15.5H6.8V19H1.5C0.9 19 0.4 18.6 0.4 17.9Z",
            "M6.6 16.1L15.2 11.6L15.6 12.3L7 16.8Z",
        ],
        // Font Awesome person-skiing-nordic
        ActivityType::CrossCountrySkiing => &[
            "M17.7 2a2.333 2.333 0 1 1 0 4.667 2.333 2.333 0 1 1 0 -4.667zm4.158 8.071c0.317 0.633 0.079 1.4 -0.525 1.75l0 7.513 -1.333 0 0 -6.842 -0.421 0.208c-1.142 0.571 -2.529 0.254 -3.308 -0.758l-0.817 -1.062 -1.646 2.867 1.033 0.517c1.229 0.612 1.787 2.062 1.296 3.342l-1.175 3.058 6.225 0c0.329 0 0.65 -0.096 0.925 -0.279l0.329 -0.221c0.458 -0.308 1.079 -0.183 1.387 0.279s0.183 1.079 -0.279 1.387l-0.329 0.221C22.625 22.454 21.917 22.667 21.192 22.667l-7.879 0c-0.021 0 -0.042 0 -0.062 0L1 22.667c-0.554 0 -1 -0.446 -1 -1s0.446 -1 1 -1l3.667 0c0 -0.342 0.129 -0.683 0.392 -0.942l3.087 -3.087 0.425 -1.496c0.471 0.762 1.154 1.408 2.017 1.842l0.2 0.1 -0.079 0.283c-0.125 0.438 -0.358 0.833 -0.679 1.154l-2.146 2.146 4.221 0 1.546 -4.017 -2.317 -1.158C9.6 14.625 8.971 12.458 9.971 10.796l1.571 -2.612 -1.154 -0.321c-0.375 -0.104 -0.754 0.133 -0.833 0.512l-0.246 1.221c-0.129 0.65 -0.713 1.096 -1.354 1.071l-5.417 8.667 -1.571 0 5.858 -9.375c-0.142 -0.262 -0.192 -0.575 -0.129 -0.887l0.246 -1.221c0.379 -1.9 2.296 -3.075 4.162 -2.558l1.354 0.375c1.946 0.542 3.667 1.7 4.9 3.304l1.037 1.346 1.683 -0.842c0.658 -0.329 1.458 -0.062 1.787 0.596z",
        ],
        // mdiSnowshoeing
        ActivityType::SnowShoe => &["M12.5 3.5C12.5 2.4 13.4 1.5 14.5 1.5S16.5 2.4 16.5 3.5 15.6 5.5 14.5 5.5 12.5 4.6 12.5 3.5M6.32 19.03L5.18 17.56L4 18.5L6.38 21.54C6.89 22.19 7.54 22.69 8.26 22.95C8.54 23.05 8.79 23 9 22.84C9.28 22.61 9.4 22.14 9.1 21.77C9 21.67 8.9 21.6 8.79 21.55C8.36 21.37 7.97 21.1 7.65 20.72L7.57 20.62L11 18.2L11.89 15L14 17V21.5H12V23H15.87C16.69 23 17.5 22.79 18.13 22.39C18.39 22.23 18.5 22 18.5 21.75C18.5 21.37 18.2 21 17.73 21C17.6 21 17.47 21.04 17.36 21.1C16.96 21.33 16.5 21.47 16 21.5V15.5L13.89 13.5L14.5 10.5C15.79 12 17.8 13 20 13V11C18.1 11 16.5 10 15.69 8.58L14.69 7C14.29 6.4 13.69 6 13 6C12.24 6 11.58 6.34 7 8.28V13H9V9.58L10.79 8.88L9.2 17L6.32 19.03Z"],
    }
}

/// An activity's icon in its color, named for a screen reader and, on a
/// pointer, by its tooltip.
#[component]
pub fn ActivityIcon(activity: ActivityType) -> Element {
    rsx! {
        Icon {
            paths: paths(activity),
            color: activity_color::color(activity),
            label: activity_color::label(activity),
        }
    }
}

/// The activities together, named `label`, in the text color.
#[component]
pub fn SumIcon(label: &'static str) -> Element {
    rsx! { Icon { paths: SUM, color: "currentColor", label } }
}

#[component]
fn Icon(paths: &'static [&'static str], color: &'static str, label: &'static str) -> Element {
    rsx! {
        // The name on a wrapper's `title`: Dioxus makes a `title` inside the
        // SVG an HTML element, which no browser shows as a tooltip.
        span {
            class: "activity-icon",
            role: "img",
            "aria-label": "{label}",
            title: "{label}",
            // An SVG attribute rather than a `style`: the page's CSP refuses
            // inline styles.
            svg { view_box: "0 0 24 24", "aria-hidden": "true",
                for shape in paths {
                    path { d: "{shape}", fill: "{color}" }
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
    use ActivityType::*;

    #[test]
    fn us77_every_activity_has_an_icon_of_its_own() {
        let every = [
            Unknown,
            Hiking,
            Mountaineering,
            Cycling,
            Bikepacking,
            Kayaking,
            SkiTouring,
            CrossCountrySkiing,
            SnowShoe,
        ];
        let icons: std::collections::HashSet<_> = every.into_iter().map(paths).collect();
        assert_eq!(icons.len(), every.len());
        assert!(!icons.contains(SUM));
    }

    #[test]
    fn us77_an_icon_wears_its_activitys_color_and_name() {
        let html = render(|| rsx! { ActivityIcon { activity: Hiking } });

        assert!(html.contains(r##"fill="#b2182b""##), "{html}");
        assert!(html.contains(r#"aria-label="Hiking""#), "{html}");
        // A `title` attribute, which a pointer shows as a tooltip; Dioxus
        // would make a `<title>` element an HTML one, which shows nothing.
        assert!(html.contains(r#"title="Hiking""#), "{html}");
        assert!(!html.contains("<title>"), "{html}");
    }

    #[test]
    fn us77_an_icon_draws_each_of_its_shapes_in_the_color() {
        let html = render(|| rsx! { ActivityIcon { activity: Bikepacking } });

        assert_eq!(html.matches("<path").count(), 4, "{html}");
        assert_eq!(html.matches(r##"fill="#4292e0""##).count(), 4, "{html}");
    }

    #[test]
    fn us77_an_unspecified_activity_is_not_named_as_the_picker_names_it() {
        let html = render(|| rsx! { ActivityIcon { activity: Unknown } });

        assert!(html.contains(r#"aria-label="Unspecified""#), "{html}");
    }

    #[test]
    fn us77_the_sum_icon_is_named_and_in_the_text_color() {
        let html = render(|| rsx! { SumIcon { label: "All activities" } });

        assert!(html.contains(r#"aria-label="All activities""#), "{html}");
        assert!(html.contains(r#"fill="currentColor""#), "{html}");
    }
}
