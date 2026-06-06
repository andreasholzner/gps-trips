//! Shared data models — one type per file, re-exported here so callers keep
//! using `crate::models::{TripSummary, TripDetail}`.

mod trip_detail;
mod trip_summary;

pub use trip_detail::TripDetail;
pub use trip_summary::TripSummary;
