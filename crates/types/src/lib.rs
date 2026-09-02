//! Shared data models (ADR-0024): one crate serving the Axum server and the
//! UI targets (`wasm32-unknown-unknown`, Android), so the JSON API's shapes
//! are defined once. One type per file, re-exported here; the server
//! re-exports the whole crate as `crate::models`, so its
//! `crate::models::{TripSummary, TripDetail}` paths are unchanged. The SQLite
//! mappings sit behind the optional `sqlx` feature only the server enables.
//!
//! Two categories of type live here, and the boundary between them is what
//! keeps this from becoming an undifferentiated module
//! ([ADR-0015](../../../docs/adr/0015-db-model-response-type-separation.md)'s
//! 2026-08-28 amendment): **stored records**, which never grow a
//! response-only field, and **the API's own request and response shapes**,
//! such as [`PhotoResponse`] and [`SyncRequest`], which receive computed
//! values as plain data and carry no server dependency. A record serves as
//! its own response type where the shapes genuinely coincide.

mod activity_type;
mod bounding_box;
mod error_response;
mod komoot_link;
mod komoot_privacy;
mod komoot_sync;
mod location_source;
mod photo;
mod photo_response;
mod session;
mod staged_import;
mod tag;
mod trip_detail;
mod trip_kind;
mod trip_summary;

pub use activity_type::ActivityType;
pub use bounding_box::BoundingBox;
pub use error_response::ErrorResponse;
pub use komoot_link::KomootLink;
pub use komoot_privacy::KomootPrivacy;
pub use komoot_sync::{
    SelectedTour, SyncCandidate, SyncCandidates, SyncPhase, SyncRequest, SyncResponse,
};
pub use location_source::LocationSource;
pub use photo::Photo;
pub use photo_response::PhotoResponse;
pub use session::{Identity, Login, Principal, Session};
pub use staged_import::{ConfirmImport, ImportedTrip, StagedImport};
pub use tag::{normalize_tag_name, Tag};
pub use trip_detail::TripDetail;
pub use trip_kind::TripKind;
pub use trip_summary::TripSummary;
