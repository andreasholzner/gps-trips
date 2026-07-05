use serde::{Deserialize, Serialize};

/// The kind of trip (ADR-0018: a closed, application-defined set of values
/// modeled as an enum rather than a bare string). Stored as `TEXT` in SQLite
/// (`#[derive(sqlx::Type)]` maps each variant to/from its snake_case name) and
/// serialized the same way in JSON responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum ActivityType {
    /// No activity was specified at import time.
    #[default]
    Unknown,
    Hiking,
    /// Hiking through mountain terrain that requires scrambling (German
    /// "Bergsteigen", as opposed to "Wandern" for plain hiking).
    Mountaineering,
    Cycling,
    Bikepacking,
    Kayaking,
    /// Multi-day backcountry ski touring, often hauling a pulk.
    SkiTouring,
    CrossCountrySkiing,
}

impl ActivityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Hiking => "hiking",
            Self::Mountaineering => "mountaineering",
            Self::Cycling => "cycling",
            Self::Bikepacking => "bikepacking",
            Self::Kayaking => "kayaking",
            Self::SkiTouring => "ski_touring",
            Self::CrossCountrySkiing => "cross_country_skiing",
        }
    }

    /// A human-readable label for UI pickers (e.g. "Ski touring"), distinct
    /// from `as_str()`'s wire/storage value (e.g. `ski_touring`).
    pub fn label(&self) -> &'static str {
        match self {
            Self::Unknown => "— unspecified —",
            Self::Hiking => "Hiking",
            Self::Mountaineering => "Mountaineering",
            Self::Cycling => "Cycling",
            Self::Bikepacking => "Bikepacking",
            Self::Kayaking => "Kayaking",
            Self::SkiTouring => "Ski touring",
            Self::CrossCountrySkiing => "Cross-country skiing",
        }
    }

    /// Every variant a user can explicitly choose in the import/edit forms —
    /// everything except `Unknown` (a picker's implicit "unspecified"). The
    /// `selectable_lists_every_variant_except_unknown_exactly_once` test
    /// below guards this against silently drifting from the enum's actual
    /// variants (US-11/US-15's `<select>`s both iterate this single list).
    pub const SELECTABLE: [ActivityType; 7] = [
        Self::Hiking,
        Self::Mountaineering,
        Self::Cycling,
        Self::Bikepacking,
        Self::Kayaking,
        Self::SkiTouring,
        Self::CrossCountrySkiing,
    ];
}

impl std::fmt::Display for ActivityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ActivityType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "unknown" => Ok(Self::Unknown),
            "hiking" => Ok(Self::Hiking),
            "mountaineering" => Ok(Self::Mountaineering),
            "cycling" => Ok(Self::Cycling),
            "bikepacking" => Ok(Self::Bikepacking),
            "kayaking" => Ok(Self::Kayaking),
            "ski_touring" => Ok(Self::SkiTouring),
            "cross_country_skiing" => Ok(Self::CrossCountrySkiing),
            other => Err(format!("unknown activity type: {other:?}")),
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_activity_type_is_unknown() {
        assert_eq!(ActivityType::default(), ActivityType::Unknown);
    }

    #[test]
    fn display_round_trips_through_from_str_for_every_variant() {
        let variants = [
            ActivityType::Unknown,
            ActivityType::Hiking,
            ActivityType::Mountaineering,
            ActivityType::Cycling,
            ActivityType::Bikepacking,
            ActivityType::Kayaking,
            ActivityType::SkiTouring,
            ActivityType::CrossCountrySkiing,
        ];
        for variant in variants {
            let rendered = variant.to_string();
            assert_eq!(rendered.parse::<ActivityType>().unwrap(), variant);
        }
    }

    #[test]
    fn from_str_rejects_an_unrecognized_value() {
        assert!("unicycling".parse::<ActivityType>().is_err());
    }

    #[test]
    fn selectable_lists_every_variant_except_unknown_exactly_once() {
        // Exhaustive match, no wildcard arm: adding a variant to the enum
        // without updating this test fails to compile, so `SELECTABLE` can't
        // silently drift from the enum's actual variants.
        for activity in ActivityType::SELECTABLE {
            match activity {
                ActivityType::Unknown => panic!("Unknown must not be in SELECTABLE"),
                ActivityType::Hiking
                | ActivityType::Mountaineering
                | ActivityType::Cycling
                | ActivityType::Bikepacking
                | ActivityType::Kayaking
                | ActivityType::SkiTouring
                | ActivityType::CrossCountrySkiing => {}
            }
        }
        assert_eq!(ActivityType::SELECTABLE.len(), 7);
    }
}
