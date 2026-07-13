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

/// The reverse of [`map_sport`] (US-20, ADR-0021): the Komoot `sport` string
/// to push when a Komoot-sourced trip's `activity_type` is written back.
/// Exhaustive over the closed `ActivityType` enum, so push always has a
/// defined outgoing string.
///
/// `Cycling` collapses four distinct Komoot sports (`touringbicycle`,
/// `racebike`, `mtb`, `mtb_easy`); `touringbicycle` is the generic fallback
/// used here. `Bikepacking`/`Kayaking`/`Unknown` have no Komoot sport of
/// their own (never observed on the account, `docs/komoot-api.md`) and map
/// to Komoot's own catch-all `"other"`.
///
/// Callers that want to avoid downgrading a trip pulled as e.g. `mtb` to the
/// generic `touringbicycle` on an edit that didn't actually touch the
/// activity type should compare against the tour's *current* live sport
/// first (via [`map_sport`]) and only fall back to this function when that
/// comparison shows a real change — this function itself always returns the
/// same string for a given `ActivityType`, with no such conditional logic.
pub fn activity_to_sport(activity_type: ActivityType) -> &'static str {
    match activity_type {
        ActivityType::Unknown => "other",
        ActivityType::Hiking => "hike",
        ActivityType::Mountaineering => "mountaineering",
        ActivityType::Cycling => "touringbicycle",
        ActivityType::Bikepacking => "other",
        ActivityType::Kayaking => "other",
        ActivityType::SkiTouring => "skitour",
        ActivityType::CrossCountrySkiing => "nordic",
        ActivityType::SnowShoe => "snowshoe",
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

    // US-20: the reverse (push) direction is exhaustive over every
    // `ActivityType` variant, so it always has a defined outgoing string.

    #[test]
    fn activity_to_sport_covers_every_activity_type() {
        assert_eq!(activity_to_sport(ActivityType::Unknown), "other");
        assert_eq!(activity_to_sport(ActivityType::Hiking), "hike");
        assert_eq!(
            activity_to_sport(ActivityType::Mountaineering),
            "mountaineering"
        );
        assert_eq!(activity_to_sport(ActivityType::Cycling), "touringbicycle");
        assert_eq!(activity_to_sport(ActivityType::Bikepacking), "other");
        assert_eq!(activity_to_sport(ActivityType::Kayaking), "other");
        assert_eq!(activity_to_sport(ActivityType::SkiTouring), "skitour");
        assert_eq!(
            activity_to_sport(ActivityType::CrossCountrySkiing),
            "nordic"
        );
        assert_eq!(activity_to_sport(ActivityType::SnowShoe), "snowshoe");
    }

    #[test]
    fn activity_to_sport_round_trips_through_map_sport_for_every_mapped_komoot_sport() {
        // Regression guard for the "don't downgrade a more specific Komoot
        // sport" logic in `komoot_sync::push_pending_edits`: every sport
        // `map_sport` recognizes must map back to an `ActivityType` whose
        // `activity_to_sport` is itself a sport `map_sport` recognizes as
        // the same `ActivityType` — otherwise the live-diff comparison could
        // loop between two different strings for the same activity type.
        for sport in [
            "hike",
            "mountaineering",
            "touringbicycle",
            "racebike",
            "mtb",
            "mtb_easy",
            "nordic",
            "skitour",
            "snowshoe",
        ] {
            let activity = map_sport(sport);
            let pushed = activity_to_sport(activity);
            assert_eq!(
                map_sport(pushed),
                activity,
                "sport {sport} -> {activity:?} -> {pushed} did not round-trip"
            );
        }
    }
}
