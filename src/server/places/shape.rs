//! An area's outline as stored in the place database: compact, since the
//! covered regions hold hundreds of thousands of lakes.
//!
//! Little-endian: the polygon count, then per polygon its ring count, then
//! per ring its point count and the points as `f32` longitude/latitude
//! pairs — a `f32` keeps a coordinate to about a metre, finer than the
//! outlines are simplified to.

use geo::{Coord, LineString, MultiPolygon, Polygon};

/// The bytes `area` is stored as.
pub fn encode_area(area: &MultiPolygon) -> Vec<u8> {
    let mut bytes = Vec::new();
    push_len(&mut bytes, area.0.len());
    for polygon in &area.0 {
        push_len(&mut bytes, 1 + polygon.interiors().len());
        for ring in std::iter::once(polygon.exterior()).chain(polygon.interiors()) {
            push_len(&mut bytes, ring.0.len());
            for coord in &ring.0 {
                bytes.extend_from_slice(&(coord.x as f32).to_le_bytes());
                bytes.extend_from_slice(&(coord.y as f32).to_le_bytes());
            }
        }
    }
    bytes
}

/// The area `bytes` hold; `None` for bytes that are not one.
pub fn decode_area(bytes: &[u8]) -> Option<MultiPolygon> {
    let mut reader = Reader(bytes);
    let polygons = (0..reader.len()?)
        .map(|_| {
            let mut rings = (0..reader.len()?)
                .map(|_| {
                    let coords = (0..reader.len()?)
                        .map(|_| {
                            Some(Coord {
                                x: f64::from(reader.f32()?),
                                y: f64::from(reader.f32()?),
                            })
                        })
                        .collect::<Option<Vec<_>>>()?;
                    Some(LineString(coords))
                })
                .collect::<Option<Vec<_>>>()?
                .into_iter();
            let exterior = rings.next()?;
            Some(Polygon::new(exterior, rings.collect()))
        })
        .collect::<Option<Vec<_>>>()?;
    reader.0.is_empty().then_some(MultiPolygon(polygons))
}

fn push_len(bytes: &mut Vec<u8>, len: usize) {
    let len = u32::try_from(len).expect("an outline far below 4 billion points");
    bytes.extend_from_slice(&len.to_le_bytes());
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take4(&mut self) -> Option<[u8; 4]> {
        let (head, rest) = self.0.split_first_chunk::<4>()?;
        self.0 = rest;
        Some(*head)
    }

    fn len(&mut self) -> Option<u32> {
        self.take4().map(u32::from_le_bytes)
    }

    fn f32(&mut self) -> Option<f32> {
        self.take4().map(f32::from_le_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::{polygon, MultiPolygon};

    #[test]
    fn us74_an_outline_reads_back_as_it_was_stored() {
        let lake = MultiPolygon(vec![polygon!(
            exterior: [(x: 7.5, y: 59.0), (x: 7.6, y: 59.0), (x: 7.6, y: 59.1), (x: 7.5, y: 59.0)],
            interiors: [[(x: 7.55, y: 59.02), (x: 7.56, y: 59.02), (x: 7.55, y: 59.03), (x: 7.55, y: 59.02)]],
        )]);

        let read = decode_area(&encode_area(&lake)).expect("an outline");

        assert_eq!(read.0.len(), 1);
        assert_eq!(read.0[0].interiors().len(), 1);
        for (a, b) in read.0[0].exterior().0.iter().zip(&lake.0[0].exterior().0) {
            assert!((a.x - b.x).abs() < 1e-5 && (a.y - b.y).abs() < 1e-5);
        }
    }

    #[test]
    fn us74_bytes_that_are_not_an_outline_read_as_none() {
        let mut bytes = encode_area(&MultiPolygon(vec![polygon![
            (x: 0.0, y: 0.0),
            (x: 1.0, y: 0.0),
            (x: 0.0, y: 1.0),
        ]]));
        bytes.pop();
        assert_eq!(decode_area(&bytes), None);
        assert_eq!(decode_area(&[1, 0, 0]), None);
    }
}
