//! The tag summary's map (US-78): everything listed under the chosen tags
//! as a line (US-107), as a share's overview map draws them (US-53). Which
//! line is which color is decided in [`entries`](super::entries) and here,
//! where `cargo test` reaches it; the script only draws what it is handed
//! (ADR-0025).

use trip_archive_types::TripTrack;

use super::entries::Entry;
use crate::interop::OverviewLine;
use crate::map_items::MapSource;
use crate::track;

/// The tags' colors, as light/dark pairs: the reference categorical
/// palette's hues without its blue, followed by seven more. The first seven
/// are neighbours that pass the colour-vision checks (`validate_palette.js`);
/// the rest trade that contrast for more tags having a color of their own.
const SLOTS: [(&str, &str); 14] = [
    ("#eb6834", "#d95926"), // orange
    ("#1baf7a", "#199e70"), // aqua
    ("#eda100", "#c98500"), // yellow
    ("#e87ba4", "#d55181"), // magenta
    ("#008300", "#008300"), // green
    ("#4a3aa7", "#9085e9"), // violet
    ("#e34948", "#e66767"), // red
    ("#17a2b8", "#3fc9d6"), // teal
    ("#8c564b", "#b07a6e"), // brown
    ("#9a9a1f", "#bcbd22"), // olive
    ("#9467bd", "#b294d6"), // plum
    ("#5f7d95", "#8aa4b8"), // slate
    ("#b8860b", "#d4a017"), // ochre
    ("#c2185b", "#e05a8a"), // rose
];

/// The color of the `index`th chosen tag, with several chosen: the light
/// steps of [`SLOTS`], in the order the tags were chosen, starting over past
/// their end.
pub fn tag_color(index: usize) -> &'static str {
    SLOTS[index % SLOTS.len()].0
}

/// One line per drawn entry whose track was read, in its color and its
/// kind's line style (US-107), opened by its source: a trip and a collection
/// track may share an id. `trips` and `collection_tracks` are the tracks
/// read for each.
pub fn lines(
    entries: &[Entry],
    trips: &[TripTrack],
    collection_tracks: &[TripTrack],
) -> Vec<OverviewLine> {
    entries
        .iter()
        .filter_map(|entry| {
            let tracks = match entry.source {
                MapSource::Trips => trips,
                MapSource::CollectionTracks => collection_tracks,
            };
            let track = tracks.iter().find(|track| track.id == entry.id)?;
            Some(OverviewLine {
                id: entry.id,
                name: entry.name.clone(),
                color: entry.color,
                icon: crate::activity_icon::map_icon(entry.activity),
                dash: entry.kind.dash(),
                source: Some(entry.source),
                points: track::lat_lon(&track.coordinates),
                href: None,
            })
        })
        .collect()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::Kind;
    use trip_archive_types::ActivityType;

    fn entry(kind: Kind, source: MapSource, id: i64, color: &'static str) -> Entry {
        Entry {
            kind,
            source,
            id,
            name: format!("Entry {id}"),
            activity: ActivityType::Cycling,
            date: None,
            distance_m: 1000.0,
            uncounted: None,
            color,
        }
    }

    fn track(id: i64, lon: f64) -> TripTrack {
        TripTrack {
            id,
            coordinates: vec![[lon, 60.0], [lon + 0.1, 60.1]],
        }
    }

    #[test]
    fn us107_each_entry_is_drawn_from_its_own_sources_track_in_its_kinds_style() {
        let entries = [
            entry(Kind::Recorded, MapSource::Trips, 1, "#111111"),
            entry(Kind::Planned, MapSource::Trips, 2, "#222222"),
            entry(Kind::Collections, MapSource::CollectionTracks, 1, "#333333"),
        ];

        let lines = lines(
            &entries,
            &[track(1, 10.0), track(2, 11.0)],
            &[track(1, 12.0)],
        );

        let drawn: Vec<_> = lines
            .iter()
            .map(|line| (line.source, line.id, line.color, line.dash, line.points[0]))
            .collect();
        assert_eq!(
            drawn,
            [
                (Some(MapSource::Trips), 1, "#111111", None, [60.0, 10.0]),
                (
                    Some(MapSource::Trips),
                    2,
                    "#222222",
                    Some("8 6"),
                    [60.0, 11.0]
                ),
                (
                    Some(MapSource::CollectionTracks),
                    1,
                    "#333333",
                    Some("1 7"),
                    [60.0, 12.0]
                ),
            ]
        );
        assert_eq!(lines[0].name, "Entry 1");
        // US-104: named in the map's hint after its activity's icon.
        assert_eq!(
            lines[0].icon,
            crate::activity_icon::map_icon(ActivityType::Cycling)
        );
    }

    #[test]
    fn us78_an_entry_without_a_track_is_left_off_the_map() {
        let entries = [
            entry(Kind::Recorded, MapSource::Trips, 1, "#111111"),
            entry(Kind::Recorded, MapSource::Trips, 2, "#222222"),
        ];

        let lines = lines(&entries, &[track(2, 10.0)], &[]);

        assert_eq!(lines.len(), 1);
        assert_eq!((lines[0].id, lines[0].color), (2, "#222222"));
    }

    #[test]
    fn us78_every_chosen_tag_has_a_color_of_its_own_until_the_palette_runs_out() {
        let colors: Vec<&str> = (0..SLOTS.len()).map(tag_color).collect();
        let distinct: std::collections::HashSet<&str> = colors.iter().copied().collect();

        assert_eq!(distinct.len(), SLOTS.len());
        assert_eq!(tag_color(SLOTS.len()), tag_color(0));
    }
}
