//! Shared data models (ADR-0024): one crate serving the Axum server and the
//! UI targets (`wasm32-unknown-unknown`, Android), so the JSON API's shapes
//! are defined once. One type per file, re-exported here; the server
//! re-exports the whole crate as `crate::models`, so its
//! `crate::models::{TripSummary, TripDetail}` paths are unchanged. The SQLite
//! mappings sit behind the optional `sqlx` feature only the server enables.

mod activity_type;
mod bounding_box;
mod komoot_link;
mod komoot_privacy;
mod location_source;
mod photo;
mod tag;
mod trip_detail;
mod trip_kind;
mod trip_summary;

pub use activity_type::ActivityType;
pub use bounding_box::BoundingBox;
pub use komoot_link::KomootLink;
pub use komoot_privacy::KomootPrivacy;
pub use location_source::LocationSource;
pub use photo::Photo;
pub use tag::{normalize_tag_name, Tag};
pub use trip_detail::TripDetail;
pub use trip_kind::TripKind;
pub use trip_summary::TripSummary;
