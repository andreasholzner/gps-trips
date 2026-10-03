//! The colors of the running chart's other years (US-77). The highlighted
//! year keeps the accent (or its activity's color) and the wider line; the
//! others are told apart by these.
//!
//! The reference categorical palette's hues without its blue, which the
//! highlight already wears, as light/dark pairs, in an order whose
//! neighbours pass the colour-vision checks in both schemes
//! (`validate_palette.js`: worst adjacent CVD ΔE 9.1 light, 8.4 dark).
//! No cycle of these seven passes, so they are not reused: a year more than
//! seven back is drawn muted instead, the "Other" of a categorical palette.

/// Light and dark step of each slot, newest year first.
pub const SLOTS: [(&str, &str); 7] = [
    ("#eb6834", "#d95926"), // orange
    ("#1baf7a", "#199e70"), // aqua
    ("#eda100", "#c98500"), // yellow
    ("#e87ba4", "#d55181"), // magenta
    ("#008300", "#008300"), // green
    ("#4a3aa7", "#9085e9"), // violet
    ("#e34948", "#e66767"), // red
];

/// `year`'s color, by how many years before `newest` it is — so a year
/// keeps its color whichever year is highlighted. `None` for a year too far
/// back to have one, which is drawn muted.
pub fn year_color(year: i32, newest: i32) -> Option<(&'static str, &'static str)> {
    usize::try_from(newest - year)
        .ok()
        .and_then(|back| SLOTS.get(back))
        .copied()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us77_each_recent_year_has_a_color_of_its_own_and_older_ones_none() {
        assert_eq!(year_color(2026, 2026), Some(SLOTS[0]));
        assert_eq!(year_color(2020, 2026), Some(SLOTS[6]));
        assert_eq!(year_color(2019, 2026), None);
        let distinct: std::collections::HashSet<_> =
            (2020..=2026).filter_map(|y| year_color(y, 2026)).collect();
        assert_eq!(distinct.len(), 7, "{distinct:?}");
    }

    #[test]
    fn us77_a_year_keeps_its_color_until_a_newer_year_arrives() {
        assert_eq!(year_color(2024, 2025), year_color(2025, 2026));
    }
}
