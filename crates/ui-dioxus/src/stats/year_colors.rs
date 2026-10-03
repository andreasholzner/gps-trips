//! The colors of the running chart's other years (US-77). The highlighted
//! year keeps the accent (or its activity's color) and the wider line; the
//! others are told apart by these.
//!
//! The reference categorical palette's hues without its blue, which the
//! highlight already wears, followed by seven more, as light/dark pairs.
//! The first seven are neighbours that pass the colour-vision checks
//! (`validate_palette.js`); the rest trade that contrast for every year
//! having a color, and an archive older than all of them starts over.

/// Light and dark step of each slot, newest year first.
pub const SLOTS: [(&str, &str); 14] = [
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

/// `year`'s color, by how many years before `newest` it is — so a year
/// keeps its color whichever year is highlighted.
pub fn year_color(year: i32, newest: i32) -> (&'static str, &'static str) {
    let back = usize::try_from(newest - year).unwrap_or(0);
    SLOTS[back % SLOTS.len()]
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us77_every_year_has_a_color_and_fourteen_in_a_row_differ() {
        assert_eq!(year_color(2026, 2026), SLOTS[0]);
        assert_eq!(year_color(2013, 2026), SLOTS[13]);
        let distinct: std::collections::HashSet<_> =
            (2013..=2026).map(|y| year_color(y, 2026)).collect();
        assert_eq!(distinct.len(), 14, "{distinct:?}");
        // Older than that, the colors start over rather than run out.
        assert_eq!(year_color(2012, 2026), SLOTS[0]);
    }

    #[test]
    fn us77_a_year_keeps_its_color_until_a_newer_year_arrives() {
        assert_eq!(year_color(2024, 2025), year_color(2025, 2026));
    }
}
