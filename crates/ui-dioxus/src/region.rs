//! The region map (US-52, carrying US-14; made the screen's centre by
//! US-63): the trips the filters match, drawn as a heat map, and the
//! rectangle the owner drags to narrow the list to trips whose stored
//! bounding box overlaps it.
//!
//! The map itself is Leaflet, reached through `interop`; this module is the
//! Rust half — what to hand the map, and what to do with the rectangle it
//! reports back.

use dioxus::prelude::*;

use crate::filters::Filters;
use crate::heat::HeatMarks;
use crate::interop;

/// The map and its controls, always in view (US-63). `marks` is `None`
/// until the list has been read, so the map is not wiped blank while a
/// re-query is in flight.
///
/// Being in view means an ordinary visit fetches OSM tiles, which the
/// collapsed panel this replaced deliberately avoided: that is what having
/// the map on screen costs, and it is paid on purpose.
#[component]
pub fn RegionFilter(filters: Signal<Filters>, marks: Option<HeatMarks>) -> Element {
    rsx! {
        section { class: "region",
            RegionMap { filters, marks }
            p { class: "region-controls",
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
                // Handled by the map's script alone: fitting the view to the
                // marks it already holds needs nothing from Rust.
                button {
                    r#type: "button",
                    id: "region-fit",
                    "Fit to trips"
                }
            }
        }
    }
}

/// The map itself: draws the rectangle the filters already hold and the
/// marks it is handed, and writes back every rectangle the owner drags.
#[component]
fn RegionMap(filters: Signal<Filters>, marks: Option<HeatMarks>) -> Element {
    let mut handle = use_signal(|| None::<document::Eval>);
    // One channel for the life of this component. `use_future` runs once, so
    // the re-render each new rectangle causes — the filters change, the list
    // re-queries — does not restart the map or drop the channel
    // (`docs/eval-two-way-spike.md`).
    use_future(move || async move {
        let restore = interop::bbox_corners(&filters.peek().bbox);
        let mut map = interop::start_region_map(restore);
        handle.set(Some(map));
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

    // The rectangle follows the region the filters hold, so clearing it —
    // "Clear region", or "Clear filters" on the toolbar — takes it off the
    // map. A memo, so a keystroke in the search box sends nothing here.
    let region = use_memo(move || filters.read().bbox.clone());
    use_effect(move || {
        let corners = interop::bbox_corners(&region.read());
        if let Some(map) = handle.read().as_ref() {
            interop::show_region(map, corners);
        }
    });

    // Redrawn whenever the list's rows change — on the same terms the table
    // re-queries — and once more when the map comes up, which may be after
    // the first rows have arrived.
    use_effect(use_reactive!(|marks| {
        if let (Some(map), Some(marks)) = (handle.read().as_ref(), marks) {
            interop::draw_heat_marks(map, &marks);
        }
    }));

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
    fn the_map_is_in_view_without_opening_anything() {
        // US-63: the map is the middle of the screen, not a filter's fine
        // print behind a disclosure.
        let html = render(|| {
            let filters = Signal::new(Filters::default());
            rsx! { RegionFilter { filters, marks: None } }
        });

        assert!(html.contains("region-map"), "{html}");
        assert!(!html.contains("<details"), "{html}");
        for control in ["Select area", "Clear region", "Fit to trips"] {
            assert!(html.contains(control), "{control}: {html}");
        }
    }
}
