//! A suggestion offered next to a form field (US-74, US-76): the archive's
//! idea, shown beside what the field holds and put into it only by the
//! owner's click — nothing the owner chose is ever replaced without it.

use dioxus::prelude::*;

/// `text` offered under a field; `on_use` puts what it says there. `what`
/// names the field for the button's label — a form can offer two.
#[component]
pub fn Suggested(
    id: String,
    what: &'static str,
    text: String,
    on_use: EventHandler<()>,
) -> Element {
    rsx! {
        p { id, class: "suggested",
            "Suggested: "
            span { class: "suggested-text", "{text}" }
            button {
                r#type: "button",
                class: "quiet",
                aria_label: "Use the suggested {what}",
                onclick: move |_| on_use.call(()),
                "Use"
            }
        }
    }
}

/// The credits the suggestions' sources require (ADR-0027): OpenStreetMap
/// for the ways, the water and most place names, Kartverket for the rest.
/// Once, under whatever a form suggests.
#[component]
pub fn MapCredits() -> Element {
    rsx! {
        small { class: "credits",
            "Map data © "
            a { href: "https://www.openstreetmap.org/copyright", "OpenStreetMap contributors" }
            " · Place names © "
            a { href: "https://www.kartverket.no/", "Kartverket" }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    #[test]
    fn us74_a_suggestion_is_offered_with_a_button_to_use_it() {
        let html = render(|| {
            rsx! {
                Suggested {
                    id: "edit-name-suggestion",
                    what: "name",
                    text: "2019-09-07 Rysstad - Kilefjorden",
                    on_use: move |_| {},
                }
            }
        });

        assert!(html.contains(r#"id="edit-name-suggestion""#), "{html}");
        assert!(html.contains("2019-09-07 Rysstad - Kilefjorden"), "{html}");
        assert!(html.contains(">Use</button>"), "{html}");
        // Two of these in one form: each button says what it is for.
        assert!(
            html.contains(r#"aria-label="Use the suggested name""#),
            "{html}"
        );
    }

    #[test]
    fn us74_us76_the_sources_of_what_is_suggested_are_credited() {
        let html = render(|| rsx! { MapCredits {} });

        // The ways and water (US-76) and the place names (US-74).
        assert!(html.contains("Map data © "), "{html}");
        assert!(html.contains("OpenStreetMap contributors"), "{html}");
        assert!(
            html.contains("https://www.openstreetmap.org/copyright"),
            "{html}"
        );
        assert!(html.contains("Place names © "), "{html}");
        assert!(html.contains("Kartverket"), "{html}");
    }
}
