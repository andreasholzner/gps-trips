//! Komoot `sport` string -> [`ActivityType`] mapping (US-22, ADR-0021).
//!
//! A hard-coded match over the sport values actually observed on the owner's
//! Komoot account (`docs/komoot-api.md`'s "Get tour photos"/list-tours
//! examples come from the same account). Komoot has more sport strings than
//! these; an unmapped/unrecognized one falls back to `ActivityType::Unknown`
//! rather than rejecting the tour (ADR-0021) — so this list only needs to
//! stay accurate for sports actually in use, not be a complete enumeration of
//! Komoot's own sport taxonomy.

use crate::models::ActivityType;

/// Map a Komoot `sport` value to this app's `ActivityType`. Several distinct
/// Komoot cycling sports (`touringbicycle`, `racebike`, `mtb`, `mtb_easy`)
/// collapse onto the single `Cycling` variant — the app has no finer-grained
/// cycling distinction today. `nordic` (Komoot's cross-country skiing sport)
/// maps to `CrossCountrySkiing`; `other` and anything unrecognized map to
/// `Unknown`, consistent with `ActivityType::default()`.
pub fn map_sport(sport: &str) -> ActivityType {
    match sport {
        "hike" => ActivityType::Hiking,
        "mountaineering" => ActivityType::Mountaineering,
        "touringbicycle" | "racebike" | "mtb" | "mtb_easy" => ActivityType::Cycling,
        "nordic" => ActivityType::CrossCountrySkiing,
        "skitour" => ActivityType::SkiTouring,
        "snowshoe" => ActivityType::SnowShoe,
        _ => ActivityType::Unknown,
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_every_sport_value_observed_on_the_real_account() {
        assert_eq!(map_sport("hike"), ActivityType::Hiking);
        assert_eq!(map_sport("mountaineering"), ActivityType::Mountaineering);
        assert_eq!(map_sport("touringbicycle"), ActivityType::Cycling);
        assert_eq!(map_sport("racebike"), ActivityType::Cycling);
        assert_eq!(map_sport("mtb"), ActivityType::Cycling);
        assert_eq!(map_sport("mtb_easy"), ActivityType::Cycling);
        assert_eq!(map_sport("nordic"), ActivityType::CrossCountrySkiing);
        assert_eq!(map_sport("skitour"), ActivityType::SkiTouring);
        assert_eq!(map_sport("snowshoe"), ActivityType::SnowShoe);
        assert_eq!(map_sport("other"), ActivityType::Unknown);
    }

    #[test]
    fn maps_an_unrecognized_sport_to_unknown_rather_than_rejecting_the_tour() {
        assert_eq!(map_sport("some_future_komoot_sport"), ActivityType::Unknown);
    }
}
