//! The one place this crate talks to a JS library ([ADR-0025](../../../../docs/adr/0025-js-widget-interop-via-eval.md)).
//!
//! `document::eval` is the only interop that works identically on the web and
//! in the Android WebView, so every widget is driven from here through a
//! string of JS and a message channel — never `wasm-bindgen` externs or
//! `web-sys`, which do not exist on the mobile target.
//!
//! The rules that ADR carries, applied here:
//!
//! - **Payloads cross over the channel**, never spliced into the script text.
//! - **Wait for the library's global**, rather than assuming script order —
//!   and, since `use_future` can run before the node it draws into exists,
//!   for the container too (a spike finding, `docs/eval-two-way-spike.md`).
//! - **The container is Dioxus-empty**; Leaflet owns that subtree, and the
//!   script refuses to initialise a second time into it.
//! - **JS renders; Rust decides.** The scripts hold no fetching and no
//!   business logic: Rust reads the API, prepares the values — a rectangle's
//!   corners into a `bbox`, a track into a polyline and a pair of chart
//!   series — and passes them in, where all of it stays unit-testable.
//!
//! Each widget lives in its own file here: the list's region map, and the
//! detail screen's track map and elevation chart.

mod region;
mod track;

pub use region::{bbox_corners, bbox_param, start_region_map};
pub use track::{start_elevation_chart, start_track_map};
