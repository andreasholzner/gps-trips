//! A suggestion offered next to a form field (US-74, US-76): the archive's
//! idea, shown beside what the field holds and put into it only by the
//! owner's click — nothing the owner chose is ever replaced without it.

use dioxus::prelude::*;

/// `text` offered under a field; `on_use` puts what it says there.
#[component]
pub fn Suggested(id: String, text: String, on_use: EventHandler<()>) -> Element {
    rsx! {
        p { id, class: "suggested",
            "Suggested: "
            span { class: "suggested-text", "{text}" }
            button {
                r#type: "button",
                class: "quiet",
                onclick: move |_| on_use.call(()),
                "Use"
            }
        }
    }
}

/// The credits the place names' sources require (ADR-0027), wherever a
/// name suggested from them is offered.
#[component]
pub fn PlaceCredits() -> Element {
    rsx! {
        small { class: "credits",
            "Place names: © "
            a { href: "https://www.openstreetmap.org/copyright", "OpenStreetMap contributors" }
            ", © "
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
                    text: "2019-09-07 Rysstad - Kilefjorden",
                    on_use: move |_| {},
                }
            }
        });

        assert!(html.contains(r#"id="edit-name-suggestion""#), "{html}");
        assert!(html.contains("2019-09-07 Rysstad - Kilefjorden"), "{html}");
        assert!(html.contains(">Use</button>"), "{html}");
    }

    #[test]
    fn us74_the_place_names_sources_are_credited() {
        let html = render(|| rsx! { PlaceCredits {} });

        assert!(html.contains("OpenStreetMap contributors"), "{html}");
        assert!(
            html.contains("https://www.openstreetmap.org/copyright"),
            "{html}"
        );
        assert!(html.contains("Kartverket"), "{html}");
    }
}
