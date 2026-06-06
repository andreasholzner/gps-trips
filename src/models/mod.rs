//! Shared data models — one type per file, re-exported here so callers keep
//! using `crate::models::{TripSummary, TripDetail}`.

mod photo;
mod trip_detail;
mod trip_summary;

pub use photo::Photo;
pub use trip_detail::TripDetail;
pub use trip_summary::TripSummary;
