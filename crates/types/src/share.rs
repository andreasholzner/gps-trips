//! US-53 — a share: read-only access to a few trips through a link, for
//! someone who has no password and no account.
//!
//! The request the owner makes ([`CreateShare`]), what it answers
//! ([`CreatedShare`]), and what the recipient reads. The recipient's shapes
//! are their own rather than the owner's `TripSummary`/`TripDetail`
//! (ADR-0015): a field the owner's screens grow later — a tag, a Komoot link
//! — must not reach a stranger because two types happened to be one.

use serde::{Deserialize, Serialize};

use crate::ActivityType;

/// How long a share lasts. A closed set on the wire, so an enum (ADR-0018);
/// how long each one is lives in the server's configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareExpiry {
    #[default]
    Never,
    OneMonth,
    SixMonths,
}

impl ShareExpiry {
    pub const ALL: [ShareExpiry; 3] = [Self::Never, Self::OneMonth, Self::SixMonths];

    /// The wire value, which the create dialog's `<select>` also uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::OneMonth => "one_month",
            Self::SixMonths => "six_months",
        }
    }

    /// What the owner reads in the create dialog.
    pub fn label(self) -> &'static str {
        match self {
            Self::Never => "Never",
            Self::OneMonth => "In 1 month",
            Self::SixMonths => "In 6 months",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|expiry| expiry.as_str() == value)
    }
}

/// The `POST /api/shares` body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateShare {
    pub trip_ids: Vec<i64>,
    /// Who or what the share is for. The recipient sees it as the title of
    /// what was shared, so it is not a private note.
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub expiry: ShareExpiry,
}

/// What `POST /api/shares` answers with. The token rather than a whole link:
/// the server does not know the origin the owner reached it by, the screen
/// does.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatedShare {
    pub token: String,
    /// RFC-3339 UTC (ADR-0009); `None` for a share that never expires.
    pub expires_at: Option<String>,
}

/// Redacted: the token is the whole of the credential.
impl std::fmt::Debug for CreatedShare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreatedShare")
            .field("token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// What the recipient lands on: the share's title and its trips, oldest
/// first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShareOverview {
    pub label: Option<String>,
    pub trips: Vec<SharedTripSummary>,
}

/// One row of [`ShareOverview`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedTripSummary {
    pub id: i64,
    pub name: String,
    pub activity_type: ActivityType,
    pub start_time: Option<String>,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub duration_secs: Option<i64>,
}

/// One shared trip as the recipient's detail screen reads it: the numbers
/// and the bounds, nothing the owner uses to organise it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedTrip {
    pub id: i64,
    pub name: String,
    pub activity_type: ActivityType,
    /// Which zone the trip's times are shown in (US-62).
    pub tz_name: Option<String>,
    pub start_time: Option<String>,
    pub start_date: Option<String>,
    pub end_time: Option<String>,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub descent_m: Option<f64>,
    pub duration_secs: Option<i64>,
    pub min_lat: Option<f64>,
    pub min_lon: Option<f64>,
    pub max_lat: Option<f64>,
    pub max_lon: Option<f64>,
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us53_an_expiry_is_a_snake_case_string_on_the_wire() {
        for expiry in ShareExpiry::ALL {
            assert_eq!(
                serde_json::to_string(&expiry).unwrap(),
                format!("\"{}\"", expiry.as_str())
            );
            assert_eq!(ShareExpiry::parse(expiry.as_str()), Some(expiry));
        }
        assert_eq!(ShareExpiry::parse("forever"), None);
    }

    #[test]
    fn us53_a_request_without_label_or_expiry_never_expires() {
        let request: CreateShare = serde_json::from_str(r#"{"trip_ids":[1,2]}"#).unwrap();
        assert_eq!(
            request,
            CreateShare {
                trip_ids: vec![1, 2],
                label: None,
                expiry: ShareExpiry::Never,
            }
        );
    }

    #[test]
    fn us53_an_unknown_expiry_is_refused() {
        assert!(
            serde_json::from_str::<CreateShare>(r#"{"trip_ids":[1],"expiry":"forever"}"#).is_err()
        );
    }

    #[test]
    fn us53_a_created_share_never_prints_its_token() {
        let printed = format!(
            "{:?}",
            CreatedShare {
                token: "secret-token".to_string(),
                expires_at: None,
            }
        );
        assert!(!printed.contains("secret-token"), "got {printed}");
    }
}
