//! US-53 — a share: read-only access to a few trips through a link, for
//! someone who has no password and no account. US-82 — or to the recorded
//! trips under a few tags, as their summary.
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

/// The `POST /api/shares` body: either `trip_ids` or `tags` (US-82), never
/// both.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateShare {
    #[serde(default)]
    pub trip_ids: Vec<i64>,
    /// The tags whose summary is shared, in the order the owner chose them.
    #[serde(default)]
    pub tags: Vec<String>,
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

/// One row of `GET /api/shares` (US-69): a share that still opens
/// something, as the owner administers it. The token is here so the owner
/// can copy the link again; the id is what stopping it names, so the token
/// never has to sit in a URL.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveShare {
    pub id: i64,
    pub token: String,
    pub label: Option<String>,
    /// The trips it names, oldest first — as the recipient sees them. Empty
    /// for a share of tags.
    pub trip_names: Vec<String>,
    /// The tags it names, in the order chosen (US-82). Empty for a share of
    /// trips.
    pub tags: Vec<String>,
    /// RFC-3339 UTC (ADR-0009).
    pub created_at: String,
    /// RFC-3339 UTC; `None` for a share that never expires.
    pub expires_at: Option<String>,
    /// How often its link was opened (US-70): each load of a share's screen
    /// by someone other than the owner. What the screen then reads is part
    /// of the same opening.
    pub opens: i64,
    /// RFC-3339 UTC; `None` until the link is first opened.
    pub last_opened_at: Option<String>,
    /// The user agents it was opened with, each once, in text order.
    pub user_agents: Vec<String>,
}

/// Redacted, like [`CreatedShare`].
impl std::fmt::Debug for ActiveShare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveShare")
            .field("id", &self.id)
            .field("token", &"<redacted>")
            .field("label", &self.label)
            .field("trip_names", &self.trip_names)
            .field("tags", &self.tags)
            .field("created_at", &self.created_at)
            .field("expires_at", &self.expires_at)
            .field("opens", &self.opens)
            .field("last_opened_at", &self.last_opened_at)
            .field("user_agents", &self.user_agents)
            .finish()
    }
}

/// What the recipient lands on: the share's title and its trips, oldest
/// first — and, for a share of tags, their summary (US-82).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShareOverview {
    pub label: Option<String>,
    pub trips: Vec<SharedTripSummary>,
    /// `None` for a share of trips.
    pub summary: Option<SharedSummary>,
}

/// A shared tag summary (US-82): what the owner's `TagSummaries` holds, in
/// the recipient's own types, so nothing the owner's grow later reaches a
/// stranger by itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedSummary {
    /// One per shared tag, in the order the owner chose them.
    pub tags: Vec<SharedTag>,
    /// The dated recorded trips under any of them, each once, oldest first.
    pub trips: Vec<SharedSummaryTrip>,
}

/// One shared tag's part of [`SharedSummary`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedTag {
    pub name: String,
    /// Its dated recorded trips, oldest first.
    pub trip_ids: Vec<i64>,
    /// Its recorded trips without timestamps, which are not counted.
    pub undated: u32,
}

/// One trip of [`SharedSummary`], as the summary adds it up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedSummaryTrip {
    pub id: i64,
    pub name: String,
    pub activity_type: ActivityType,
    /// Local dates, `YYYY-MM-DD`, in the trip's own timezone.
    pub start_date: String,
    pub end_date: String,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub descent_m: Option<f64>,
    pub moving_secs: Option<i64>,
    pub moving_distance_m: Option<f64>,
}

/// One row of [`ShareOverview`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedTripSummary {
    pub id: i64,
    pub name: String,
    pub activity_type: ActivityType,
    pub start_time: Option<String>,
    /// The local date the trip started on, `YYYY-MM-DD` — in its own
    /// timezone, as `TripDetail::start_date` is, so the list and the trip's
    /// page give it the same day. `None` for a track with no timestamps.
    pub start_date: Option<String>,
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
    /// What the average speed is worked out from (US-80).
    pub moving_secs: Option<i64>,
    pub moving_distance_m: Option<f64>,
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
                tags: Vec::new(),
                label: None,
                expiry: ShareExpiry::Never,
            }
        );
    }

    #[test]
    fn us82_a_request_may_name_tags_instead_of_trips() {
        let request: CreateShare =
            serde_json::from_str(r#"{"tags":["alps","norway"],"label":"Summer"}"#).unwrap();
        assert_eq!(
            request,
            CreateShare {
                trip_ids: Vec::new(),
                tags: vec!["alps".to_string(), "norway".to_string()],
                label: Some("Summer".to_string()),
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
    fn us69_an_active_share_never_prints_its_token() {
        let printed = format!(
            "{:?}",
            ActiveShare {
                id: 1,
                token: "secret-token".to_string(),
                label: None,
                trip_names: vec!["Walk".to_string()],
                tags: Vec::new(),
                created_at: "2026-09-25T12:00:00Z".to_string(),
                expires_at: None,
                opens: 0,
                last_opened_at: None,
                user_agents: Vec::new(),
            }
        );
        assert!(!printed.contains("secret-token"), "got {printed}");
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
