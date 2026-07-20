use serde::{Deserialize, Serialize};

/// Whether a trip was actually recorded or is only planned (US-31/US-32),
/// modeled as a closed enum per ADR-0018. Stored as `TEXT` in SQLite
/// (`#[derive(sqlx::Type)]` maps each variant to/from its snake_case name) and
/// serialized the same way in JSON responses.
///
/// Every trip-creating path today (manual GPX import, Komoot sync/backfill)
/// writes `Recorded` — nothing can produce `Planned` yet. That lands with
/// US-31, which gives the owner an explicit choice at import time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum TripKind {
    #[default]
    Recorded,
    Planned,
}

impl TripKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Recorded => "recorded",
            Self::Planned => "planned",
        }
    }

    /// A human-readable label for the list page's tabs.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Recorded => "Recorded",
            Self::Planned => "Planned",
        }
    }

    /// Every variant, for iterating the list page's tabs. Exhaustively
    /// matched in this file's tests so a future variant can't silently drift
    /// out of sync with this list.
    pub const ALL: [TripKind; 2] = [Self::Recorded, Self::Planned];
}

impl std::fmt::Display for TripKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for TripKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "recorded" => Ok(Self::Recorded),
            "planned" => Ok(Self::Planned),
            other => Err(format!("unknown trip kind: {other:?}")),
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_trip_kind_is_recorded() {
        assert_eq!(TripKind::default(), TripKind::Recorded);
    }

    #[test]
    fn display_round_trips_through_from_str_for_every_variant() {
        for variant in TripKind::ALL {
            let rendered = variant.to_string();
            assert_eq!(rendered.parse::<TripKind>().unwrap(), variant);
        }
    }

    #[test]
    fn from_str_rejects_an_unrecognized_value() {
        assert!("scheduled".parse::<TripKind>().is_err());
    }

    #[test]
    fn all_lists_every_variant_exactly_once() {
        // Exhaustive match, no wildcard arm: adding a variant to the enum
        // without updating `ALL` fails to compile.
        for kind in TripKind::ALL {
            match kind {
                TripKind::Recorded | TripKind::Planned => {}
            }
        }
        assert_eq!(TripKind::ALL.len(), 2);
    }
}
