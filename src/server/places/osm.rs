//! OpenStreetMap's places, read from what `osmium export` writes: one
//! GeoJSON feature per line, its tags as properties (`build.rs` runs it).
//!
//! Only what the name suggestion uses is kept: settlements, farms and
//! localities, summits, passes, huts, campsites, lakes, bays and glaciers —
//! named, or a summit known by its height.

use geo::{Centroid, ChamberlainDuquetteArea, Coord, MultiPolygon, Polygon, Simplify};
use serde_json::Value;

use super::{Place, PlaceKind, Shape, Source};

/// How far an outline is simplified, in degrees: about 10 m north-south.
const SIMPLIFY_DEGREES: f64 = 0.0001;

/// The languages a name in several is read in, first match first: the
/// covered countries' own (ADR-0027).
const NAME_LANGUAGES: [&str; 9] = ["no", "nb", "nn", "sv", "fi", "de", "it", "fr", "sl"];

/// The place `feature` describes, and its id in OSM (`node/123`); `None`
/// for anything the suggestion does not use.
pub fn place_from_feature(feature: &Value) -> Option<(Place, String)> {
    let tags = Tags(feature.get("properties")?.as_object()?);
    let tag = |key: &str| tags.get(key);
    let kind = kind(&tags)?;
    let ele_m = tag("ele").and_then(metres);
    let name = match tag("name").map(|name| display_name(name, &tags)) {
        Some(name) => name,
        None if kind == PlaceKind::Summit => format!("{:.0}", ele_m?),
        None => return None,
    };
    let shape = shape(feature.get("geometry")?)?;
    let shape = match (kind, shape) {
        (PlaceKind::Lake | PlaceKind::Bay | PlaceKind::Glacier, shape) => shape,
        (_, Shape::Area(area)) => Shape::Point(area.centroid()?.0),
        (_, point) => point,
    };
    let area_m2 = match &shape {
        Shape::Area(area) => Some(area.chamberlain_duquette_unsigned_area()),
        Shape::Point(_) => None,
    };
    let shape = match shape {
        Shape::Area(area) => Shape::Area(area.simplify(&SIMPLIFY_DEGREES)),
        point => point,
    };
    let id = format!(
        "{}/{}",
        tag("@type")?,
        feature["properties"]["@id"].as_i64()?
    );
    Some((
        Place {
            name,
            kind,
            source: Source::Osm,
            shape,
            ele_m,
            prominence_m: tag("prominence").and_then(metres),
            area_m2,
            population: tag("population").and_then(|p| p.replace([' ', ','], "").parse().ok()),
        },
        id,
    ))
}

/// A feature's OSM tags.
struct Tags<'a>(&'a serde_json::Map<String, Value>);

impl<'a> Tags<'a> {
    fn get(&self, key: &str) -> Option<&'a str> {
        self.0.get(key).and_then(Value::as_str)
    }
}

fn kind(tags: &Tags) -> Option<PlaceKind> {
    let tag = |key: &str| tags.get(key);
    let kind = match (tag("place"), tag("natural"), tag("tourism")) {
        (Some("city"), _, _) => PlaceKind::City,
        (Some("town"), _, _) => PlaceKind::Town,
        (Some("village"), _, _) => PlaceKind::Village,
        (Some("hamlet"), _, _) => PlaceKind::Hamlet,
        (Some("farm" | "isolated_dwelling"), _, _) => PlaceKind::Farm,
        (Some("locality"), _, _) => PlaceKind::Locality,
        (_, Some("peak" | "volcano"), _) => PlaceKind::Summit,
        (_, Some("saddle"), _) => PlaceKind::Pass,
        (_, Some("bay"), _) => PlaceKind::Bay,
        (_, Some("glacier"), _) => PlaceKind::Glacier,
        (_, Some("water"), _) => match tag("water") {
            None | Some("lake" | "reservoir" | "pond" | "lagoon" | "oxbow") => PlaceKind::Lake,
            Some(_) => return None,
        },
        (_, _, Some("alpine_hut" | "wilderness_hut")) => PlaceKind::Hut,
        (_, _, Some("camp_site")) => PlaceKind::Campsite,
        _ if tag("mountain_pass") == Some("yes") => PlaceKind::Pass,
        _ => return None,
    };
    Some(kind)
}

/// `name`, or where it gives several languages (`Guovdageaidnu /
/// Kautokeino`, `Gryllefjorden - Grullefierda`) the one in the first of
/// [`NAME_LANGUAGES`] it has a tag for — or else the first given. Without
/// the quotes a name is sometimes tagged in.
fn display_name(name: &str, tags: &Tags) -> String {
    let separator = if name.contains(" / ") { " / " } else { " - " };
    let parts: Vec<&str> = name
        .split(separator)
        .map(|part| part.trim().trim_matches('"').trim())
        .collect();
    if parts.len() == 1 {
        return parts[0].to_string();
    }
    NAME_LANGUAGES
        .iter()
        .filter_map(|lang| tags.get(&format!("name:{lang}")))
        .find(|local| parts.contains(local))
        .unwrap_or(parts[0])
        .to_string()
}

/// A height as OSM tags give it: `884`, `884 m`, `884.5`.
fn metres(value: &str) -> Option<f64> {
    let number = value.trim().trim_end_matches('m').trim();
    number.parse().ok().filter(|m: &f64| m.is_finite())
}

fn shape(geometry: &Value) -> Option<Shape> {
    let coords = &geometry["coordinates"];
    match geometry["type"].as_str()? {
        "Point" => Some(Shape::Point(coord(coords)?)),
        "Polygon" => Some(Shape::Area(MultiPolygon(vec![polygon(coords)?]))),
        "MultiPolygon" => Some(Shape::Area(MultiPolygon(
            coords
                .as_array()?
                .iter()
                .map(polygon)
                .collect::<Option<_>>()?,
        ))),
        _ => None,
    }
}

fn polygon(rings: &Value) -> Option<Polygon> {
    let mut rings = rings.as_array()?.iter().map(|ring| {
        ring.as_array()?
            .iter()
            .map(coord)
            .collect::<Option<Vec<Coord>>>()
            .map(geo::LineString)
    });
    let exterior = rings.next()??;
    Some(Polygon::new(exterior, rings.collect::<Option<_>>()?))
}

fn coord(value: &Value) -> Option<Coord> {
    Some(Coord {
        x: value.get(0)?.as_f64()?,
        y: value.get(1)?.as_f64()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn feature(geometry: Value, tags: Value) -> Value {
        let mut properties = tags;
        properties["@type"] = json!("node");
        properties["@id"] = json!(42);
        json!({ "type": "Feature", "geometry": geometry, "properties": properties })
    }

    fn at(x: f64, y: f64) -> Value {
        json!({ "type": "Point", "coordinates": [x, y] })
    }

    fn square() -> Value {
        json!({ "type": "MultiPolygon", "coordinates": [[[
            [10.0, 60.0], [10.01, 60.0], [10.01, 60.01], [10.0, 60.01], [10.0, 60.0]
        ]]] })
    }

    #[test]
    fn us74_a_named_summit_keeps_its_height_and_prominence() {
        let (place, id) = place_from_feature(&feature(
            at(16.0, 69.0),
            json!({ "natural": "peak", "name": "Hamperokken", "ele": "1404", "prominence": "1100 m" }),
        ))
        .expect("a place");

        assert_eq!(id, "node/42");
        assert_eq!(place.name, "Hamperokken");
        assert_eq!(place.kind, PlaceKind::Summit);
        assert_eq!(place.ele_m, Some(1404.0));
        assert_eq!(place.prominence_m, Some(1100.0));
        assert_eq!(place.shape, Shape::Point(Coord { x: 16.0, y: 69.0 }));
    }

    #[test]
    fn us74_a_summit_known_only_by_its_height_is_named_by_it() {
        let (place, _) = place_from_feature(&feature(
            at(16.0, 69.0),
            json!({ "natural": "peak", "ele": "884.4" }),
        ))
        .expect("a place");
        assert_eq!(place.name, "884");

        assert!(
            place_from_feature(&feature(at(16.0, 69.0), json!({ "natural": "peak" }))).is_none()
        );
    }

    #[test]
    fn us74_a_lake_keeps_its_outline_and_area() {
        let (place, _) = place_from_feature(&feature(
            square(),
            json!({ "natural": "water", "water": "lake", "name": "Vatnet" }),
        ))
        .expect("a place");

        assert_eq!(place.kind, PlaceKind::Lake);
        assert!(matches!(place.shape, Shape::Area(_)));
        // 0.01° by 0.01° at 60° N: about 1.1 km by 0.56 km.
        let area = place.area_m2.expect("an area");
        assert!((area - 620_000.0).abs() < 20_000.0, "{area}");
    }

    #[test]
    fn us74_a_river_is_not_a_lake_and_an_unnamed_lake_is_nothing() {
        assert!(place_from_feature(&feature(
            square(),
            json!({ "natural": "water", "water": "river", "name": "Elva" }),
        ))
        .is_none());
        assert!(place_from_feature(&feature(square(), json!({ "natural": "water" }))).is_none());
    }

    #[test]
    fn us74_a_hut_drawn_as_a_building_stands_at_its_centre() {
        let (place, _) = place_from_feature(&feature(
            square(),
            json!({ "tourism": "wilderness_hut", "name": "Koia" }),
        ))
        .expect("a place");

        assert_eq!(place.kind, PlaceKind::Hut);
        let Shape::Point(c) = place.shape else {
            panic!("a point")
        };
        assert!((c.x - 10.005).abs() < 1e-9 && (c.y - 60.005).abs() < 1e-9);
        assert_eq!(place.area_m2, None);
    }

    #[test]
    fn us74_each_kind_is_read_from_its_tags() {
        let cases = [
            (json!({ "place": "city" }), Some(PlaceKind::City)),
            (json!({ "place": "town" }), Some(PlaceKind::Town)),
            (json!({ "place": "village" }), Some(PlaceKind::Village)),
            (json!({ "place": "hamlet" }), Some(PlaceKind::Hamlet)),
            (
                json!({ "place": "isolated_dwelling" }),
                Some(PlaceKind::Farm),
            ),
            (json!({ "place": "farm" }), Some(PlaceKind::Farm)),
            (json!({ "place": "locality" }), Some(PlaceKind::Locality)),
            (json!({ "place": "islet" }), None),
            (json!({ "natural": "saddle" }), Some(PlaceKind::Pass)),
            (
                json!({ "mountain_pass": "yes", "highway": "unclassified" }),
                Some(PlaceKind::Pass),
            ),
            (json!({ "natural": "bay" }), Some(PlaceKind::Bay)),
            (json!({ "natural": "glacier" }), Some(PlaceKind::Glacier)),
            (json!({ "natural": "wood" }), None),
            (json!({ "tourism": "alpine_hut" }), Some(PlaceKind::Hut)),
            (json!({ "tourism": "camp_site" }), Some(PlaceKind::Campsite)),
            (json!({ "amenity": "ferry_terminal" }), None),
        ];
        for (mut tags, kind) in cases {
            tags["name"] = json!("Navn");
            let read = place_from_feature(&feature(at(10.0, 60.0), tags.clone()));
            assert_eq!(read.map(|(p, _)| p.kind), kind, "{tags}");
        }
    }

    #[test]
    fn us74_a_name_in_several_languages_is_read_in_the_local_one() {
        let (place, _) = place_from_feature(&feature(
            at(23.0, 69.0),
            json!({ "place": "village", "name": "Guovdageaidnu / Kautokeino",
                    "name:no": "Kautokeino", "name:se": "Guovdageaidnu" }),
        ))
        .expect("a place");
        assert_eq!(place.name, "Kautokeino");

        let (place, _) = place_from_feature(&feature(
            at(23.0, 69.0),
            json!({ "place": "village", "name": "Áhkánjárga / Narvik" }),
        ))
        .expect("a place");
        assert_eq!(place.name, "Áhkánjárga");

        let (place, _) = place_from_feature(&feature(
            at(17.0, 69.0),
            json!({ "place": "village", "name": "Gryllefjorden - Grullefierda" }),
        ))
        .expect("a place");
        assert_eq!(place.name, "Gryllefjorden");
    }

    #[test]
    fn us74_a_name_tagged_in_quotes_is_read_without_them() {
        let (place, _) = place_from_feature(&feature(
            at(18.5, 69.6),
            json!({ "natural": "peak", "name": "\"884\"", "ele": "884" }),
        ))
        .expect("a place");
        assert_eq!(place.name, "884");
        assert!(place.is_height_only());
    }

    #[test]
    fn us74_a_settlement_keeps_its_population() {
        let (place, _) = place_from_feature(&feature(
            at(23.0, 69.0),
            json!({ "place": "town", "name": "Narvik", "population": "14199" }),
        ))
        .expect("a place");
        assert_eq!(place.population, Some(14_199));
    }
}
