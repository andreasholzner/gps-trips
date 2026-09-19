//! Owner-facing export configuration (US-36/US-39, ADR-0022): one TOML file
//! holding the target database path and the folder-mapping template.
//!
//! ```toml
//! target_db = "/home/owner/qms/Touren.db"
//! folder_template = "Trips/{year}/{activity_type}"
//! undated = "undated"                  # optional {year} fallback
//!
//! [activity_type_names]                # required: every ActivityType, incl. "unknown"
//! unknown = "Unspecified"
//! hiking = "Hiking"
//! mountaineering = "Mountaineering"
//! cycling = "Cycling"
//! bikepacking = "Bikepacking"
//! kayaking = "Kayaking"
//! ski_touring = "Skitour"
//! cross_country_skiing = "Cross-country skiing"
//! snow_shoe = "Snowshoeing"
//! [trip_type_names]                    # required: every TripKind
//! recorded = "Recorded"
//! planned = "Geplant"
//! ```
//!
//! Everything is validated up front (validate at the boundary): after
//! `from_toml_str` succeeds, folder resolution is infallible. Per US-39, the
//! name tables above are mandatory and exhaustive — a config missing an
//! entry for any `ActivityType`/`TripKind` variant fails to load, listing
//! every missing entry.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;

use crate::models::{ActivityType, TripKind};

/// Supported `folder_template` placeholders.
const PLACEHOLDERS: &str = "{year}, {activity_type}, {trip_type}";

/// Folder-name fallback for `{year}` when a trip has no start time.
const DEFAULT_UNDATED: &str = "undated";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read config file {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("config is not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("target_db must not be empty")]
    EmptyTargetDb,
    #[error("folder_template must not be empty")]
    EmptyTemplate,
    #[error("folder_template must not begin/end with '/' or contain empty segments: {0:?}")]
    EmptySegment(String),
    #[error("unknown placeholder {{{found}}} in folder_template; supported: {PLACEHOLDERS}")]
    UnknownPlaceholder { found: String },
    #[error("unmatched '{{' or '}}' in folder_template segment {0:?}")]
    UnmatchedBrace(String),
    #[error("[{table}] key {key:?} is not a valid value; expected one of: {valid}")]
    UnknownNameKey {
        table: &'static str,
        key: String,
        valid: String,
    },
    #[error("{what} {value:?} must be non-empty and must not contain '/'")]
    BadName { what: String, value: String },
    #[error("config is missing required folder-name mappings: {0}")]
    IncompleteMapping(String),
}

/// The raw TOML shape. `deny_unknown_fields` makes a typo'd key a loud
/// parse error instead of a silently ignored setting.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    target_db: PathBuf,
    folder_template: String,
    undated: Option<String>,
    activity_type_names: Option<HashMap<String, String>>,
    trip_type_names: Option<HashMap<String, String>>,
}

/// One piece of a folder-path segment; a segment like `"{year}-{trip_type}"`
/// parses into `[Year, Literal("-"), TripType]`.
#[derive(Debug, PartialEq)]
enum Piece {
    Literal(String),
    Year,
    ActivityType,
    TripType,
}

/// Validated export configuration.
#[derive(Debug)]
pub struct ExportConfig {
    pub target_db: PathBuf,
    /// Parsed `folder_template`: one `Vec<Piece>` per path segment.
    template: Vec<Vec<Piece>>,
    undated: String,
    activity_names: HashMap<ActivityType, String>,
    trip_kind_names: HashMap<TripKind, String>,
}

impl ExportConfig {
    /// Parse and validate a TOML config document.
    pub fn from_toml_str(s: &str) -> Result<Self, ConfigError> {
        let raw: RawConfig = toml::from_str(s)?;

        if raw.target_db.as_os_str().is_empty() {
            return Err(ConfigError::EmptyTargetDb);
        }
        let template = parse_template(&raw.folder_template)?;

        let undated = raw.undated.unwrap_or_else(|| DEFAULT_UNDATED.to_string());
        check_folder_name("undated", &undated)?;

        let activity_names = parse_name_table(
            raw.activity_type_names.unwrap_or_default(),
            "activity_type_names",
            &activity_type_keys(),
        )?;
        let trip_kind_names = parse_name_table(
            raw.trip_type_names.unwrap_or_default(),
            "trip_type_names",
            &TripKind::ALL.map(|k| k.as_str().to_string()),
        )?;

        // US-39: every ActivityType/TripKind variant must be mapped, not
        // just the ones the owner chose to override. Collect every gap
        // across both tables into one error rather than failing on the
        // first, per the story's "all missing entries are listed" criteria.
        let missing_activity: Vec<&'static str> = all_activity_types()
            .filter(|a| !activity_names.contains_key(a))
            .map(|a| a.as_str())
            .collect();
        let missing_trip_kind: Vec<&'static str> = TripKind::ALL
            .into_iter()
            .filter(|k| !trip_kind_names.contains_key(k))
            .map(|k| k.as_str())
            .collect();
        if !missing_activity.is_empty() || !missing_trip_kind.is_empty() {
            let mut parts = Vec::new();
            if !missing_activity.is_empty() {
                parts.push(format!(
                    "[activity_type_names] missing: {}",
                    missing_activity.join(", ")
                ));
            }
            if !missing_trip_kind.is_empty() {
                parts.push(format!(
                    "[trip_type_names] missing: {}",
                    missing_trip_kind.join(", ")
                ));
            }
            return Err(ConfigError::IncompleteMapping(parts.join("; ")));
        }

        Ok(Self {
            target_db: raw.target_db,
            template,
            undated,
            activity_names,
            trip_kind_names,
        })
    }

    /// Read and parse the config file at `path`.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_toml_str(&text)
    }

    /// Resolve the folder path (one name per level, root-relative) for a
    /// trip. Infallible: everything variable was validated at load time.
    pub fn resolve_folder_path(
        &self,
        activity: ActivityType,
        kind: TripKind,
        start_time: Option<&str>,
    ) -> Vec<String> {
        self.template
            .iter()
            .map(|segment| {
                segment
                    .iter()
                    .map(|piece| match piece {
                        Piece::Literal(text) => text.clone(),
                        // RFC-3339 always starts "YYYY-" (ADR-0009); a
                        // malformed value falls back to the undated bucket
                        // (`get` also covers a non-char-boundary byte 4).
                        Piece::Year => match start_time.and_then(|t| t.get(..4)) {
                            Some(year) => year.to_string(),
                            None => self.undated.clone(),
                        },
                        Piece::ActivityType => self.activity_name(activity),
                        Piece::TripType => self.trip_kind_name(kind),
                    })
                    .collect::<String>()
            })
            .collect()
    }

    fn activity_name(&self, activity: ActivityType) -> String {
        // Invariant: from_toml_str rejects a config missing any ActivityType
        // entry (US-39), so every variant is present by the time this runs.
        self.activity_names
            .get(&activity)
            .cloned()
            .expect("activity mapping validated complete at config load")
    }

    fn trip_kind_name(&self, kind: TripKind) -> String {
        self.trip_kind_names
            .get(&kind)
            .cloned()
            .expect("trip-kind mapping validated complete at config load")
    }
}

/// Every `ActivityType` an `[activity_type_names]` entry is required for,
/// incl. `Unknown` (US-39).
fn all_activity_types() -> impl Iterator<Item = ActivityType> {
    std::iter::once(ActivityType::Unknown).chain(ActivityType::SELECTABLE)
}

/// Every valid `[activity_type_names]` key: the wire names, incl. `unknown`.
fn activity_type_keys() -> Vec<String> {
    all_activity_types()
        .map(|a| a.as_str().to_string())
        .collect()
}

fn parse_template(template: &str) -> Result<Vec<Vec<Piece>>, ConfigError> {
    if template.is_empty() {
        return Err(ConfigError::EmptyTemplate);
    }
    let segments: Vec<&str> = template.split('/').collect();
    let mut parsed = Vec::with_capacity(segments.len());
    for segment in segments {
        if segment.is_empty() {
            return Err(ConfigError::EmptySegment(template.to_string()));
        }
        parsed.push(parse_segment(segment)?);
    }
    Ok(parsed)
}

fn parse_segment(segment: &str) -> Result<Vec<Piece>, ConfigError> {
    let mut pieces = Vec::new();
    let mut rest = segment;
    while !rest.is_empty() {
        match rest.find('{') {
            None => {
                if rest.contains('}') {
                    return Err(ConfigError::UnmatchedBrace(segment.to_string()));
                }
                pieces.push(Piece::Literal(rest.to_string()));
                rest = "";
            }
            Some(open) => {
                let literal = &rest[..open];
                if literal.contains('}') {
                    return Err(ConfigError::UnmatchedBrace(segment.to_string()));
                }
                if !literal.is_empty() {
                    pieces.push(Piece::Literal(literal.to_string()));
                }
                let after = &rest[open + 1..];
                let Some(close) = after.find('}') else {
                    return Err(ConfigError::UnmatchedBrace(segment.to_string()));
                };
                let name = &after[..close];
                pieces.push(match name {
                    "year" => Piece::Year,
                    "activity_type" => Piece::ActivityType,
                    "trip_type" => Piece::TripType,
                    other => {
                        return Err(ConfigError::UnknownPlaceholder {
                            found: other.to_string(),
                        })
                    }
                });
                rest = &after[close + 1..];
            }
        }
    }
    Ok(pieces)
}

fn parse_name_table<K: FromStr + std::hash::Hash + Eq>(
    raw: HashMap<String, String>,
    table: &'static str,
    valid_keys: &[String],
) -> Result<HashMap<K, String>, ConfigError> {
    let mut parsed = HashMap::with_capacity(raw.len());
    for (key, name) in raw {
        let Ok(parsed_key) = key.parse::<K>() else {
            return Err(ConfigError::UnknownNameKey {
                table,
                key,
                valid: valid_keys.join(", "),
            });
        };
        check_folder_name(&format!("[{table}] value for {key:?}"), &name)?;
        parsed.insert(parsed_key, name);
    }
    Ok(parsed)
}

fn check_folder_name(what: &str, value: &str) -> Result<(), ConfigError> {
    if value.is_empty() || value.contains('/') {
        return Err(ConfigError::BadName {
            what: what.to_string(),
            value: value.to_string(),
        });
    }
    Ok(())
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
