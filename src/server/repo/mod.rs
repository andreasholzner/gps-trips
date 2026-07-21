//! Database access (ADR-0002), split by domain: `trip` for trip/track CRUD,
//! `photo` for photo association rows — mirroring how `import.rs`/`photos.rs`/
//! `delete.rs` already separate concerns rather than one large repo module.
//! Re-exported flat here so existing call sites (`repo::insert_trip`,
//! `repo::list_photos`, ...) are unaffected by the split.

pub mod komoot;
mod photo;
mod tag;
mod trip;

pub use photo::{count_photos, insert_photo, list_photos, NewPhoto};
pub use tag::{
    add_trip_tag, bulk_add_trip_tags, get_or_create_tag, list_all_tags, list_trip_tags,
    remove_trip_tag, trips_exist,
};
pub use trip::{
    delete_trip, get_original_gpx, get_track_geojson, get_trip, insert_trip, insert_trip_in_tx,
    list_trips, set_trip_timezone, update_trip, GpxDownload, NewTrip, TripFilter,
};

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// Format a timestamp as RFC-3339 for storage. Formatting a valid `OffsetDateTime`
/// with the well-known RFC-3339 description cannot fail, so a failure is a bug.
fn to_rfc3339(t: OffsetDateTime) -> String {
    t.format(&Rfc3339)
        .expect("RFC-3339 formatting of a valid OffsetDateTime never fails")
}
