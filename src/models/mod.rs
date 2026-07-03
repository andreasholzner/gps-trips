//! Shared data models — one type per file, re-exported here so callers keep
//! using `crate::models::{TripSummary, TripDetail}`.

mod activity_type;
mod location_source;
mod photo;
mod trip_detail;
mod trip_summary;

pub use activity_type::ActivityType;
pub use location_source::LocationSource;
pub use photo::Photo;
pub use trip_detail::TripDetail;
pub use trip_summary::TripSummary;
