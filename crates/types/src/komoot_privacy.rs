use serde::{Deserialize, Serialize};

/// A linked Komoot tour's privacy — Komoot's `status` field (US-35,
/// ADR-0018). Stored as `TEXT` on `trip_komoot_link` (never on `trip`:
/// privacy belongs to the Komoot link, and a trip with no link row has none)
/// and serialized the same way in JSON responses.
///
/// `Unknown` is the landing spot for any `status` string Komoot sends that
/// this app doesn't map — displayed, but never pushed back, so an edit of
/// some other field can't overwrite a privacy state the archive doesn't
/// understand (ADR-0021). It is deliberately not offered in the UI; see
/// [`Self::SELECTABLE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "sqlx", sqlx(rename_all = "snake_case"))]
pub enum KomootPrivacy {
    /// Komoot reported a `status` this app has no mapping for (or none at
    /// all).
    #[default]
    Unknown,
    Private,
    Public,
}

impl KomootPrivacy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Private => "private",
            Self::Public => "public",
        }
    }

    /// A human-readable label for the list page's privacy column and the
    /// detail page's picker, distinct from `as_str()`'s wire/storage value.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Private => "Private",
            Self::Public => "Public",
        }
    }

    /// Map a raw Komoot `status` string onto a variant, falling back to
    /// `Unknown` for anything unrecognized (including an absent field, which
    /// deserializes to the empty string) — the same stance
    /// `komoot_sport::map_sport` takes for an unmapped sport: a pull never
    /// fails over a value Komoot added on its side.
    pub fn from_komoot_status(status: &str) -> Self {
        status.parse().unwrap_or(Self::Unknown)
    }

    /// The string to send as Komoot's `status` on a push, or `None` when
    /// there is nothing safe to send — the push then omits the field
    /// entirely, leaving Komoot's own privacy untouched.
    pub fn komoot_status(&self) -> Option<&'static str> {
        match self {
            Self::Unknown => None,
            Self::Private | Self::Public => Some(self.as_str()),
        }
    }

    /// Every variant the owner can explicitly choose, i.e. everything except
    /// `Unknown` — the single list the detail page's `<select>` iterates and
    /// the edit endpoint validates against. Guarded by the
    /// `selectable_lists_every_variant_except_unknown_exactly_once` test
    /// below against drifting from the enum's actual variants.
    pub const SELECTABLE: [KomootPrivacy; 2] = [Self::Private, Self::Public];

    /// Every variant, for round-trip tests and exhaustive iteration.
    pub const ALL: [KomootPrivacy; 3] = [Self::Unknown, Self::Private, Self::Public];
}

impl std::fmt::Display for KomootPrivacy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for KomootPrivacy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "unknown" => Ok(Self::Unknown),
            "private" => Ok(Self::Private),
            "public" => Ok(Self::Public),
            other => Err(format!("unknown komoot privacy: {other:?}")),
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_privacy_is_unknown() {
        // Nothing is known about a link row's privacy until a sync has read
        // it off Komoot's tour listing.
        assert_eq!(KomootPrivacy::default(), KomootPrivacy::Unknown);
    }

    #[test]
    fn komoot_status_strings_map_to_their_variants() {
        assert_eq!(
            KomootPrivacy::from_komoot_status("private"),
            KomootPrivacy::Private
        );
        assert_eq!(
            KomootPrivacy::from_komoot_status("public"),
            KomootPrivacy::Public
        );
    }

    #[test]
    fn an_unrecognized_komoot_status_maps_to_unknown() {
        // ADR-0021: an unmapped Komoot value never fails the sync — it lands
        // as `Unknown` and is simply never pushed back, the same stance
        // `komoot_sport::map_sport` takes for an unmapped sport. An absent
        // `status` deserializes to the empty string and follows the same path.
        assert_eq!(
            KomootPrivacy::from_komoot_status("friends_only"),
            KomootPrivacy::Unknown
        );
        assert_eq!(
            KomootPrivacy::from_komoot_status(""),
            KomootPrivacy::Unknown
        );
    }

    #[test]
    fn komoot_status_returns_a_pushable_string_only_for_known_variants() {
        // The push phase omits `status` entirely for `Unknown`, so that an
        // edit of some *other* field can't overwrite a Komoot privacy state
        // this app doesn't understand.
        assert_eq!(KomootPrivacy::Private.komoot_status(), Some("private"));
        assert_eq!(KomootPrivacy::Public.komoot_status(), Some("public"));
        assert_eq!(KomootPrivacy::Unknown.komoot_status(), None);
    }

    #[test]
    fn display_round_trips_through_from_str_for_every_variant() {
        for variant in KomootPrivacy::ALL {
            let rendered = variant.to_string();
            assert_eq!(rendered.parse::<KomootPrivacy>().unwrap(), variant);
        }
    }

    #[test]
    fn from_str_rejects_an_unrecognized_value() {
        assert!("friends_only".parse::<KomootPrivacy>().is_err());
    }

    #[test]
    fn selectable_lists_every_variant_except_unknown_exactly_once() {
        // Exhaustive match, no wildcard arm: adding a variant to the enum
        // without deciding whether the owner may pick it fails to compile.
        for privacy in KomootPrivacy::ALL {
            match privacy {
                KomootPrivacy::Unknown => assert!(!KomootPrivacy::SELECTABLE.contains(&privacy)),
                KomootPrivacy::Private | KomootPrivacy::Public => {
                    assert_eq!(
                        KomootPrivacy::SELECTABLE
                            .iter()
                            .filter(|&&p| p == privacy)
                            .count(),
                        1
                    );
                }
            }
        }
        assert_eq!(
            KomootPrivacy::SELECTABLE.len(),
            KomootPrivacy::ALL.len() - 1
        );
    }
}
