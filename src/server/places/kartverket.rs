//! Kartverket's place-name register (Stedsnavn, ADR-0027), read from its
//! GML download in geographic ETRS89 (EPSG:4258) — near enough WGS 84 for
//! naming, and in latitude, longitude order.
//!
//! The register supplements OpenStreetMap in Norway: it names summits,
//! lakes, huts and communities OSM does not have, and gives each name in
//! the first of the languages the place is named in. It has no heights or
//! outlines; where OSM has the same place, its ranking merges the two.

use std::io::BufRead;

use geo::Coord;
use quick_xml::events::Event;
use quick_xml::Reader;

use super::{Place, PlaceKind, Shape, Source};

/// The kind of place a register object type is, for the types the name
/// suggestion uses.
fn kind(object_type: &str) -> Option<PlaceKind> {
    let kind = match object_type {
        "by" => PlaceKind::Town,
        "tettsted" | "tettbebyggelse" | "tettsteddel" | "bygdelagBygd" => PlaceKind::Village,
        "grend" => PlaceKind::Hamlet,
        "gard" | "bruk" | "navnegard" | "seterStøl" | "setervoll" => PlaceKind::Farm,
        "fjell" | "topp" | "høyde" | "berg" | "haug" | "rygg" | "egg" => PlaceKind::Summit,
        "skar" | "fjellovergang" => PlaceKind::Pass,
        "turisthytte" => PlaceKind::Hut,
        "campingplass" => PlaceKind::Campsite,
        "vann" | "innsjø" | "tjern" | "delAvInnsjø" | "lon" => PlaceKind::Lake,
        "vikISjø" | "vik" | "vågISjø" | "fjord" | "botn" => PlaceKind::Bay,
        "isbre" | "fonn" => PlaceKind::Glacier,
        _ => return None,
    };
    Some(kind)
}

/// The places in a register download, one `app:Sted` at a time, with their
/// register numbers.
pub struct Register<R: BufRead> {
    reader: Reader<R>,
    buf: Vec<u8>,
}

impl<R: BufRead> Register<R> {
    pub fn new(input: R) -> Self {
        Self {
            reader: Reader::from_reader(input),
            buf: Vec::new(),
        }
    }
}

/// What one `app:Sted` says, as far as it has been read.
#[derive(Default)]
struct Sted {
    position: Option<Coord>,
    object_type: Option<String>,
    language_priority: Option<String>,
    number: Option<String>,
    names: Vec<Name>,
}

#[derive(Default)]
struct Name {
    language: Option<String>,
    main: bool,
    spelling: Option<String>,
}

impl Sted {
    fn finish(self) -> Option<(Place, String)> {
        let kind = kind(self.object_type.as_deref()?)?;
        let first_language = self
            .language_priority
            .as_deref()
            .and_then(|priority| priority.split('-').next());
        let in_language = |name: &&Name| name.language.as_deref() == first_language;
        let name = self
            .names
            .iter()
            .filter(|name| name.spelling.is_some())
            .min_by_key(|name| (!in_language(name), !name.main))?;
        Some((
            Place {
                name: name.spelling.clone()?,
                kind,
                source: Source::Kartverket,
                shape: Shape::Point(self.position?),
                ele_m: None,
                prominence_m: None,
                area_m2: None,
                population: None,
            },
            self.number?,
        ))
    }
}

impl<R: BufRead> Iterator for Register<R> {
    type Item = Result<(Place, String), quick_xml::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut sted: Option<Sted> = None;
        let mut text = String::new();
        loop {
            self.buf.clear();
            let event = match self.reader.read_event_into(&mut self.buf) {
                Ok(event) => event,
                Err(e) => return Some(Err(e)),
            };
            match event {
                Event::Eof => return None,
                Event::Start(start) => {
                    text.clear();
                    match start.local_name().as_ref() {
                        "Sted" => sted = Some(Sted::default()),
                        "Stedsnavn" => {
                            if let Some(sted) = &mut sted {
                                sted.names.push(Name::default());
                            }
                        }
                        _ => {}
                    }
                }
                Event::Text(t) => text.push_str(&t.xml10_content()),
                Event::GeneralRef(r) => text.push_str(match r.as_ref() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    _ => "",
                }),
                Event::End(end) => {
                    let Some(current) = &mut sted else {
                        continue;
                    };
                    let value = text.trim().to_string();
                    let name = current.names.last_mut();
                    match end.local_name().as_ref() {
                        "Sted" => match sted.take().and_then(Sted::finish) {
                            Some(place) => return Some(Ok(place)),
                            None => continue,
                        },
                        "pos" | "posList" if current.position.is_none() => {
                            current.position = lat_lon(&value);
                        }
                        "navneobjekttype" => current.object_type = Some(value),
                        "språkprioritering" => current.language_priority = Some(value),
                        "stedsnummer" => current.number = Some(value),
                        "språk" => {
                            if let Some(name) = name {
                                name.language = Some(value);
                            }
                        }
                        "navnestatus" => {
                            if let Some(name) = name {
                                name.main = value == "hovednavn";
                            }
                        }
                        "komplettskrivemåte" => {
                            if let Some(name) = name.filter(|name| name.spelling.is_none()) {
                                name.spelling = Some(value);
                            }
                        }
                        _ => {}
                    }
                    text.clear();
                }
                _ => {}
            }
        }
    }
}

/// The first position of a GML `pos` or `posList`: latitude, longitude.
fn lat_lon(value: &str) -> Option<Coord> {
    let mut numbers = value.split_whitespace().map(str::parse::<f64>);
    let y = numbers.next()?.ok()?;
    let x = numbers.next()?.ok()?;
    Some(Coord { x, y })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sted(object_type: &str, geometry: &str, names: &str, priority: &str) -> String {
        format!(
            r#"<gml:featureMember><app:Sted gml:id="x">
                <app:identifikasjon><app:Identifikasjon><app:lokalId>9</app:lokalId></app:Identifikasjon></app:identifikasjon>
                {geometry}
                {names}
                <app:navneobjekttype>{object_type}</app:navneobjekttype>
                <app:språkprioritering>{priority}</app:språkprioritering>
                <app:kommune><app:Kommune><app:kommunenavn>Tromsø</app:kommunenavn></app:Kommune></app:kommune>
                <app:stedsnummer>1234</app:stedsnummer>
            </app:Sted></gml:featureMember>"#
        )
    }

    fn name(language: &str, status: &str, spellings: &[&str]) -> String {
        let spellings: String = spellings
            .iter()
            .map(|s| {
                format!(
                    "<app:skrivemåte><app:Skrivemåte><app:komplettskrivemåte>{s}</app:komplettskrivemåte></app:Skrivemåte></app:skrivemåte>"
                )
            })
            .collect();
        format!(
            "<app:stedsnavn><app:Stedsnavn><app:navnestatus>{status}</app:navnestatus><app:språk>{language}</app:språk>{spellings}</app:Stedsnavn></app:stedsnavn>"
        )
    }

    const POINT: &str = r#"<app:posisjon><gml:Point><gml:pos>69.562045 19.358217</gml:pos></gml:Point></app:posisjon>"#;

    fn read(stedene: &[String]) -> Vec<(Place, String)> {
        let gml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><gml:FeatureCollection xmlns:app="a" xmlns:gml="g">{}</gml:FeatureCollection>"#,
            stedene.concat()
        );
        Register::new(gml.as_bytes())
            .collect::<Result<_, _>>()
            .expect("valid GML")
    }

    #[test]
    fn us74_a_summit_is_read_with_its_position_and_number() {
        let places = read(&[sted(
            "fjell",
            POINT,
            &name("norsk", "hovednavn", &["Hamperokken"]),
            "norsk-nordsamisk",
        )]);

        assert_eq!(places.len(), 1);
        let (place, number) = &places[0];
        assert_eq!(number, "1234");
        assert_eq!(place.name, "Hamperokken");
        assert_eq!(place.kind, PlaceKind::Summit);
        assert_eq!(place.source, Source::Kartverket);
        assert_eq!(
            place.shape,
            Shape::Point(Coord {
                x: 19.358217,
                y: 69.562045
            })
        );
    }

    #[test]
    fn us74_a_name_in_several_languages_is_read_in_the_register_s_first() {
        let names = [
            name("nordsamisk", "hovednavn", &["Romsa"]),
            name("norsk", "sidenavn", &["Tromsøya"]),
            name("norsk", "hovednavn", &["Tromsø", "Tromsøe"]),
        ]
        .concat();
        let places = read(&[sted("by", POINT, &names, "norsk-kvensk-nordsamisk")]);
        assert_eq!(places[0].0.name, "Tromsø");
    }

    #[test]
    fn us74_a_multipoint_or_line_stands_at_its_first_position() {
        let multipoint = r#"<app:multipunkt><gml:MultiPoint><gml:pointMember><gml:Point><gml:pos>59.093453 7.535889</gml:pos></gml:Point></gml:pointMember><gml:pointMember><gml:Point><gml:pos>59.1 7.6</gml:pos></gml:Point></gml:pointMember></gml:MultiPoint></app:multipunkt>"#;
        let curve = r#"<app:multikurve><gml:MultiCurve><gml:curveMember><gml:LineString><gml:posList>59.2 9.6 59.3 9.7</gml:posList></gml:LineString></gml:curveMember></gml:MultiCurve></app:multikurve>"#;
        let places = read(&[
            sted(
                "bygdelagBygd",
                multipoint,
                &name("norsk", "hovednavn", &["Rysstad"]),
                "norsk",
            ),
            sted(
                "fjord",
                curve,
                &name("norsk", "hovednavn", &["Kilefjorden"]),
                "norsk",
            ),
        ]);

        assert_eq!(places[0].0.kind, PlaceKind::Village);
        assert_eq!(
            places[0].0.shape,
            Shape::Point(Coord {
                x: 7.535889,
                y: 59.093453
            })
        );
        assert_eq!(places[1].0.kind, PlaceKind::Bay);
        assert_eq!(places[1].0.shape, Shape::Point(Coord { x: 9.6, y: 59.2 }));
    }

    #[test]
    fn us74_object_types_the_suggestion_does_not_use_are_skipped() {
        let places = read(&[
            sted(
                "adressenavn",
                POINT,
                &name("norsk", "hovednavn", &["Storgata"]),
                "norsk",
            ),
            sted(
                "myr",
                POINT,
                &name("norsk", "hovednavn", &["Kjølen"]),
                "norsk",
            ),
            sted(
                "turisthytte",
                POINT,
                &name("norsk", "hovednavn", &["Koia"]),
                "norsk",
            ),
        ]);
        assert_eq!(places.len(), 1);
        assert_eq!(places[0].0.kind, PlaceKind::Hut);
    }

    #[test]
    fn us74_an_escaped_name_is_read_unescaped() {
        let places = read(&[sted(
            "campingplass",
            POINT,
            &name("norsk", "hovednavn", &["Bakken &amp; Sønn Camping"]),
            "norsk",
        )]);
        assert_eq!(places[0].0.name, "Bakken & Sønn Camping");
    }
}
