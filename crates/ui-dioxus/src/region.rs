//! The region filter (US-52, carrying US-14): a collapsed map the owner
//! drags a rectangle on, narrowing the list to trips whose stored bounding
//! box overlaps it.
//!
//! The map itself is Leaflet, reached through `interop`; this module is the
//! Rust half — when to show a map at all, what to hand it, and what to do
//! with the rectangle it reports back.

use dioxus::prelude::*;

use crate::filters::Filters;
use crate::interop;

/// The collapsed "Region" panel and its map.
///
/// The map is built only once the panel is open: Leaflet cannot lay out
/// inside a closed `<details>` — the container has no size there and every
/// tile lands in the wrong place — and an ordinary list view then fetches no
/// OSM tiles at all.
#[component]
pub fn RegionFilter(filters: Signal<Filters>) -> Element {
    let mut opened = use_signal(|| false);

    rsx! {
        details {
            ontoggle: move |_| opened.set(true),
            summary { "Region" }
            p {
                button {
                    r#type: "button",
                    id: "region-select",
                    "Select area"
                }
                button {
                    r#type: "button",
                    id: "region-clear",
                    onclick: move |_| filters.write().bbox = String::new(),
                    "Clear region"
                }
            }
            // Mounted only once opened, so the map is built against a
            // container that actually has a size.
            if opened() {
                RegionMap { filters }
            }
        }
    }
}

/// The map itself: draws the rectangle the filters already hold, and writes
/// back every rectangle the owner drags.
#[component]
fn RegionMap(filters: Signal<Filters>) -> Element {
    // One channel for the life of this component. `use_future` runs once, so
    // the re-render each new rectangle causes — the filters change, the list
    // re-queries — does not restart the map or drop the channel
    // (`docs/eval-two-way-spike.md`).
    use_future(move || async move {
        let restore = interop::bbox_corners(&filters.peek().bbox);
        let mut map = interop::start_region_map(restore);
        loop {
            match map.recv::<[f64; 4]>().await {
                Ok(corners) => filters.write().bbox = interop::bbox_param(corners),
                Err(err) => {
                    dioxus::logger::tracing::error!("the region map stopped reporting: {err}");
                    break;
                }
            }
        }
    });

    rsx! {
        // Rendered empty and never given children: Leaflet owns this subtree
        // from the moment it initialises (ADR-0025).
        div { id: "region-map", class: "region-map" }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    #[test]
    fn the_region_panel_is_collapsed_and_draws_no_map_until_it_is_opened() {
        // US-14 wants the map collapsed; more practically, Leaflet cannot lay
        // out inside a closed `<details>`, and a list view that never opens
        // the panel should fetch no map tiles.
        let html = render(|| {
            let filters = Signal::new(Filters::default());
            rsx! { RegionFilter { filters } }
        });

        assert!(html.contains("Region"), "{html}");
        assert!(html.contains("Select area"), "{html}");
        assert!(
            !html.contains("region-map"),
            "the map container must not exist while collapsed: {html}"
        );
    }
}
