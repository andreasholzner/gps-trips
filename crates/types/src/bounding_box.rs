/// A geographic rectangle in WGS-84 degrees — the region the owner draws on
/// the trip-list map (US-14, ADR-0011). Rectangle-only in v1: no polygons, and
/// no antimeridian wrap, so `min_lon <= max_lon` and `min_lat <= max_lat`
/// always hold (`filter::parse_filter` rejects anything else at the HTTP
/// boundary).
///
/// Field order mirrors the `trip` table's bbox columns rather than the
/// `bbox=minLon,minLat,maxLon,maxLat` query parameter (ADR-0008), since this
/// type is compared against those columns far more often than it is parsed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    pub min_lat: f64,
    pub min_lon: f64,
    pub max_lat: f64,
    pub max_lon: f64,
}
