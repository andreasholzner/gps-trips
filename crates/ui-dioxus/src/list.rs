//! The trip-list screen (US-41): browse, filter and tag trips. A placeholder
//! until the screen's feature slices land, each moving its story's acceptance
//! assertions over from the server-rendered page (ADR-0012).

use dioxus::prelude::*;

#[component]
pub fn TripList() -> Element {
    rsx! {
        h1 { "Trips" }
    }
}
