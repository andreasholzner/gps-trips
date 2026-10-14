//! Linking what is selected on the trip list (US-99): every recorded trip
//! to every planned trip and collection track, every planned trip to every
//! collection track — so six days and the plan they followed become six
//! links in one go.

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::CreateLinks;

use crate::api::{self, ApiClient};
use crate::picked::{link_ends, link_pairs, Picked};

/// What the panel says it is about to do.
pub fn proposal(pairs: usize) -> String {
    match pairs {
        1 => "Makes 1 link.".to_string(),
        n => format!("Makes {n} links."),
    }
}

/// The panel: how many links the selection makes, made once confirmed.
/// `on_linked` lets the list clear its selection and read its marks anew.
#[component]
pub fn LinkPanel(picked: BTreeSet<Picked>, on_linked: EventHandler<()>) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut message = use_signal(|| None::<String>);
    let pairs = link_pairs(&picked);
    let (trip_ids, collection_track_ids) = link_ends(&picked);
    rsx! {
        fieldset { id: "link-panel",
            legend { "Link selected" }
            p { "{proposal(pairs)} A pair already linked stays as it is." }
            button {
                r#type: "button",
                onclick: move |_| {
                    let request = CreateLinks {
                        trip_ids: trip_ids.clone(),
                        collection_track_ids: collection_track_ids.clone(),
                    };
                    spawn(async move {
                        match api::create_links(&archive(), &request).await {
                            Ok(_) => {
                                message.set(None);
                                on_linked.call(());
                            }
                            Err(err) => message.set(Some(err.to_string())),
                        }
                    });
                },
                "Link"
            }
            if let Some(message) = message() {
                p { class: "error", "{message}" }
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::Kind;
    use crate::test_support::render;

    #[test]
    fn us99_the_panel_names_how_many_links_it_makes() {
        assert_eq!(proposal(1), "Makes 1 link.");
        assert_eq!(proposal(6), "Makes 6 links.");

        let picked: BTreeSet<Picked> = [
            Picked::Trip(Kind::Recorded, 1),
            Picked::Trip(Kind::Recorded, 2),
            Picked::Trip(Kind::Planned, 3),
        ]
        .into();
        let html = render(move || rsx! { LinkPanel { picked: picked.clone(), on_linked: |_| {} } });

        assert!(html.contains("Makes 2 links."), "{html}");
        assert!(html.contains(">Link</button>"), "{html}");
    }
}
