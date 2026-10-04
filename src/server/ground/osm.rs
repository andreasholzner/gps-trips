//! OpenStreetMap's ways and water, read from what `osmium export` writes:
//! one GeoJSON feature per line, its tags as properties (`build.rs` runs
//! it).

use geo::{ChamberlainDuquetteArea, Coord, LineString, MultiPolygon, Polygon, Simplify};
use serde_json::Value;

use super::Way;
use crate::config::activity_suggestion::MIN_WATER_M2;

/// How far water outlines are simplified, in degrees: about 10 m
/// north-south.
const SIMPLIFY_DEGREES: f64 = 0.0001;

/// What a feature adds to the ground data.
#[derive(Debug, Clone, PartialEq)]
pub enum Ground {
    Way(Way, Vec<Coord>),
    Water(MultiPolygon),
}

/// What `feature` adds; `None` for anything the suggestion does not use.
pub fn ground_from_feature(feature: &Value) -> Option<Ground> {
    let tags = feature.get("properties")?.as_object()?;
    let tag = |key: &str| tags.get(key).and_then(Value::as_str);
    let geometry = feature.get("geometry")?;
    if let Some(highway) = tag("highway") {
        let way = way(highway, tag("surface"), tag("bicycle"))?;
        let coords = match geometry["type"].as_str()? {
            "LineString" => coords(&geometry["coordinates"])?,
            _ => return None,
        };
        return Some(Ground::Way(way, coords));
    }
    if tag("natural") == Some("water") || tag("waterway") == Some("riverbank") {
        let area = match geometry["type"].as_str()? {
            "Polygon" => MultiPolygon(vec![polygon(&geometry["coordinates"])?]),
            "MultiPolygon" => MultiPolygon(
                geometry["coordinates"]
                    .as_array()?
                    .iter()
                    .map(polygon)
                    .collect::<Option<_>>()?,
            ),
            _ => return None,
        };
        if area.chamberlain_duquette_unsigned_area() < MIN_WATER_M2 {
            return None;
        }
        return Some(Ground::Water(area.simplify(&SIMPLIFY_DEGREES)));
    }
    None
}

/// The kind of way a `highway` is: a road; a track, cycleway or path
/// with a good surface; or a small path.
fn way(highway: &str, surface: Option<&str>, bicycle: Option<&str>) -> Option<Way> {
    let good_surface = matches!(
        surface,
        Some(
            "asphalt"
                | "paved"
                | "concrete"
                | "concrete:plates"
                | "paving_stones"
                | "chipseal"
                | "gravel"
                | "fine_gravel"
                | "compacted"
        )
    );
    let way = match highway {
        "motorway" | "motorway_link" | "trunk" | "trunk_link" | "primary" | "primary_link"
        | "secondary" | "secondary_link" | "tertiary" | "tertiary_link" | "unclassified"
        | "residential" | "living_street" | "service" | "road" | "pedestrian" | "busway" => {
            Way::Road
        }
        "track" | "cycleway" => Way::Good,
        "path" | "footway" | "bridleway" if good_surface || bicycle == Some("designated") => {
            Way::Good
        }
        "path" | "footway" | "bridleway" | "steps" | "via_ferrata" => Way::Small,
        _ => return None,
    };
    Some(way)
}

fn coords(value: &Value) -> Option<Vec<Coord>> {
    value
        .as_array()?
        .iter()
        .map(|c| {
            Some(Coord {
                x: c.get(0)?.as_f64()?,
                y: c.get(1)?.as_f64()?,
            })
        })
        .collect()
}

fn polygon(rings: &Value) -> Option<Polygon> {
    let mut rings = rings
        .as_array()?
        .iter()
        .map(|ring| coords(ring).map(LineString));
    let exterior = rings.next()??;
    Some(Polygon::new(exterior, rings.collect::<Option<_>>()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn line(tags: Value) -> Value {
        json!({ "type": "Feature", "properties": tags,
                "geometry": { "type": "LineString", "coordinates": [[10.0, 60.0], [10.01, 60.0]] } })
    }

    fn square(tags: Value, side_degrees: f64) -> Value {
        let s = side_degrees;
        json!({ "type": "Feature", "properties": tags,
                "geometry": { "type": "MultiPolygon", "coordinates": [[[
                    [10.0, 60.0], [10.0 + s, 60.0], [10.0 + s, 60.0 + s], [10.0, 60.0 + s], [10.0, 60.0]
                ]]] } })
    }

    fn kind(tags: Value) -> Option<Way> {
        match ground_from_feature(&line(tags))? {
            Ground::Way(way, _) => Some(way),
            Ground::Water(_) => None,
        }
    }

    #[test]
    fn us76_each_kind_of_way_is_read_from_its_tags() {
        assert_eq!(kind(json!({ "highway": "primary" })), Some(Way::Road));
        assert_eq!(kind(json!({ "highway": "residential" })), Some(Way::Road));
        assert_eq!(kind(json!({ "highway": "track" })), Some(Way::Good));
        assert_eq!(kind(json!({ "highway": "cycleway" })), Some(Way::Good));
        assert_eq!(
            kind(json!({ "highway": "path", "surface": "gravel" })),
            Some(Way::Good)
        );
        assert_eq!(
            kind(json!({ "highway": "footway", "bicycle": "designated" })),
            Some(Way::Good)
        );
        assert_eq!(kind(json!({ "highway": "path" })), Some(Way::Small));
        assert_eq!(
            kind(json!({ "highway": "path", "surface": "ground" })),
            Some(Way::Small)
        );
        assert_eq!(kind(json!({ "highway": "steps" })), Some(Way::Small));
        assert_eq!(kind(json!({ "highway": "construction" })), None);
        assert_eq!(kind(json!({ "railway": "rail" })), None);
    }

    #[test]
    fn us76_a_lake_is_water_and_a_pond_is_not() {
        // 0.01° by 0.01° at 60° N: about 0.6 km².
        let lake = ground_from_feature(&square(json!({ "natural": "water" }), 0.01));
        assert!(matches!(lake, Some(Ground::Water(_))), "{lake:?}");
        let riverbank = ground_from_feature(&square(json!({ "waterway": "riverbank" }), 0.01));
        assert!(matches!(riverbank, Some(Ground::Water(_))));
        // 0.0005° by 0.0005°: about 1500 m².
        assert_eq!(
            ground_from_feature(&square(json!({ "natural": "water" }), 0.0005)),
            None
        );
    }
}
