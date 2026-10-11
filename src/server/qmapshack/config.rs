//! Owner-facing export configuration (US-36/US-39/US-51/US-89, ADR-0022):
//! one TOML file holding the archive to read, the target database path and
//! the folder-mapping templates, one for trips and one for collections. Found like `backup`'s own file, as
//! `trip-archive/qmapshack_export.toml` in the user's config directory.
//!
//! ```toml
//! url = "https://<app>.fly.dev"
//! password_command = "kwallet-query -r trip-archive kdewallet"   # optional
//! target_db = "/home/owner/qms/Touren.db"
//! folder_template = "Trips/{year}/{activity_type}"
//! collection_folder_template = "Guidebooks/{collection}"   # required
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
//! entry for any `ActivityType`/`TripKind` variant, or the collection
//! template (US-89), fails to load, listing every missing entry.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;

use crate::models::{ActivityType, TripKind};
use crate::server::archive_client::config::{self as archive, ArchiveConfigError};

/// The config file's name in the user's config directory.
const CONFIG_FILE: &str = "qmapshack_export.toml";

const TRIP_TEMPLATE_KEY: &str = "folder_template";
const COLLECTION_TEMPLATE_KEY: &str = "collection_folder_template";

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
    #[error("{0} must not be empty")]
    EmptyTemplate(&'static str),
    #[error("{key} must not begin/end with '/' or contain empty segments: {template:?}")]
    EmptySegment { key: &'static str, template: String },
    #[error("unknown placeholder {{{found}}} in {key}; supported: {supported}")]
    UnknownPlaceholder {
        key: &'static str,
        found: String,
        supported: &'static str,
    },
    #[error("unmatched '{{' or '}}' in {key} segment {segment:?}")]
    UnmatchedBrace { key: &'static str, segment: String },
    #[error("{COLLECTION_TEMPLATE_KEY} must contain {{collection}}, or every collection shares one folder")]
    NoCollectionPlaceholder,
    #[error("[{table}] key {key:?} is not a valid value; expected one of: {valid}")]
    UnknownNameKey {
        table: &'static str,
        key: String,
        valid: String,
    },
    #[error("{what} {value:?} must be non-empty and must not contain '/'")]
    BadName { what: String, value: String },
    #[error("config is missing required entries: {0}")]
    IncompleteMapping(String),
    #[error(transparent)]
    Archive(#[from] ArchiveConfigError),
}

/// The raw TOML shape. `deny_unknown_fields` makes a typo'd key a loud
/// parse error instead of a silently ignored setting.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    url: String,
    password_command: Option<String>,
    target_db: PathBuf,
    folder_template: String,
    collection_folder_template: Option<String>,
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
    Collection,
}

/// Which of the two folder templates is parsed: each takes only its own
/// placeholders, so a collection is never filed by a trip's year.
#[derive(Clone, Copy)]
enum Template {
    Trips,
    Collections,
}

impl Template {
    fn key(self) -> &'static str {
        match self {
            Self::Trips => TRIP_TEMPLATE_KEY,
            Self::Collections => COLLECTION_TEMPLATE_KEY,
        }
    }

    fn supported(self) -> &'static str {
        match self {
            Self::Trips => "{year}, {activity_type}, {trip_type}",
            Self::Collections => "{collection}",
        }
    }

    fn placeholder(self, name: &str) -> Option<Piece> {
        match (self, name) {
            (Self::Trips, "year") => Some(Piece::Year),
            (Self::Trips, "activity_type") => Some(Piece::ActivityType),
            (Self::Trips, "trip_type") => Some(Piece::TripType),
            (Self::Collections, "collection") => Some(Piece::Collection),
            _ => None,
        }
    }
}

/// Validated export configuration.
#[derive(Debug)]
pub struct ExportConfig {
    /// The archive's base URL, https only (US-51).
    pub url: String,
    /// A command printing the archive password, run by `sh`; asked for when absent.
    pub password_command: Option<String>,
    pub target_db: PathBuf,
    /// Parsed `folder_template`: one `Vec<Piece>` per path segment.
    template: Vec<Vec<Piece>>,
    /// Parsed `collection_folder_template`, likewise (US-89).
    collection_template: Vec<Vec<Piece>>,
    undated: String,
    activity_names: HashMap<ActivityType, String>,
    trip_kind_names: HashMap<TripKind, String>,
}

impl ExportConfig {
    /// Parse and validate a TOML config document.
    pub fn from_toml_str(s: &str) -> Result<Self, ConfigError> {
        let raw: RawConfig = toml::from_str(s)?;

        archive::validate(&raw.url, raw.password_command.as_deref())?;
        if raw.target_db.as_os_str().is_empty() {
            return Err(ConfigError::EmptyTargetDb);
        }
        let template = parse_template(&raw.folder_template, Template::Trips)?;
        let collection_template = raw
            .collection_folder_template
            .as_deref()
            .map(|t| parse_template(t, Template::Collections))
            .transpose()?;
        if collection_template.as_ref().is_some_and(|template| {
            !template
                .iter()
                .flatten()
                .any(|piece| *piece == Piece::Collection)
        }) {
            return Err(ConfigError::NoCollectionPlaceholder);
        }

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
        // just the ones the owner chose to override, and US-89's collection
        // template given. Collect every gap into one error rather than
        // failing on the first, per the story's "all missing entries are
        // listed" criteria.
        let missing_activity: Vec<&'static str> = all_activity_types()
            .filter(|a| !activity_names.contains_key(a))
            .map(|a| a.as_str())
            .collect();
        let missing_trip_kind: Vec<&'static str> = TripKind::ALL
            .into_iter()
            .filter(|k| !trip_kind_names.contains_key(k))
            .map(|k| k.as_str())
            .collect();
        let mut parts = Vec::new();
        if collection_template.is_none() {
            parts.push(COLLECTION_TEMPLATE_KEY.to_string());
        }
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
        let Some(collection_template) = collection_template.filter(|_| parts.is_empty()) else {
            return Err(ConfigError::IncompleteMapping(parts.join("; ")));
        };

        Ok(Self {
            url: raw.url,
            password_command: raw.password_command,
            target_db: raw.target_db,
            template,
            collection_template,
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

    /// `$XDG_CONFIG_HOME/trip-archive/qmapshack_export.toml`, or
    /// `~/.config/…` when that is unset — where `backup` looks for its own.
    pub fn default_path() -> Result<PathBuf, ConfigError> {
        Ok(archive::default_path(CONFIG_FILE)?)
    }

    /// Resolve the folder path (one name per level, root-relative) for a
    /// trip. Infallible: everything variable was validated at load time.
    pub fn resolve_folder_path(
        &self,
        activity: ActivityType,
        kind: TripKind,
        start_date: Option<&str>,
    ) -> Vec<String> {
        self.template
            .iter()
            .map(|segment| {
                segment
                    .iter()
                    .map(|piece| match piece {
                        Piece::Literal(text) => text.clone(),
                        // A date starts "YYYY-"; a malformed value falls
                        // back to the undated bucket (`get` also covers a
                        // non-char-boundary byte 4).
                        Piece::Year => match start_date.and_then(|t| t.get(..4)) {
                            Some(year) => year.to_string(),
                            None => self.undated.clone(),
                        },
                        Piece::ActivityType => self.activity_name(activity),
                        Piece::TripType => self.trip_kind_name(kind),
                        Piece::Collection => unreachable!("not a trip template placeholder"),
                    })
                    .collect::<String>()
            })
            .collect()
    }

    /// Resolve the folder path for a collection (US-89), as
    /// [`Self::resolve_folder_path`] does for a trip.
    pub fn resolve_collection_folder_path(&self, name: &str) -> Vec<String> {
        self.collection_template
            .iter()
            .map(|segment| {
                segment
                    .iter()
                    .map(|piece| match piece {
                        Piece::Literal(text) => text.as_str(),
                        Piece::Collection => name,
                        _ => unreachable!("not a collection template placeholder"),
                    })
                    .collect::<String>()
            })
            .collect()
    }

    /// A warning when the collections' top folder can be one the trips'
    /// template gives too, so collection tracks may land among the trips
    /// (US-89); `None` when the two can never meet. Only a warning: the
    /// owner decides whether the export goes on.
    pub fn collection_folder_overlap(&self) -> Option<String> {
        let top = &self.collection_template[0];
        if top.contains(&Piece::Collection) {
            return Some(format!(
                "{COLLECTION_TEMPLATE_KEY}'s top folder is named after each collection, \
                 so a collection may share it with trips"
            ));
        }
        let name: String = top
            .iter()
            .map(|piece| match piece {
                Piece::Literal(text) => text.as_str(),
                _ => unreachable!("only literals are left"),
            })
            .collect();
        self.can_resolve_to(&self.template[0], &name).then(|| {
            format!(
                "{COLLECTION_TEMPLATE_KEY}'s top folder {name:?} can also be one of \
                 {TRIP_TEMPLATE_KEY}'s, so collections may share folders with trips"
            )
        })
    }

    /// Whether a trip template segment can resolve to exactly `name`.
    fn can_resolve_to(&self, pieces: &[Piece], name: &str) -> bool {
        let Some((first, rest)) = pieces.split_first() else {
            return name.is_empty();
        };
        let rest_matches = |prefix: &str| {
            name.strip_prefix(prefix)
                .is_some_and(|tail| self.can_resolve_to(rest, tail))
        };
        match first {
            Piece::Literal(text) => rest_matches(text),
            // A year is the start date's first four characters: digits.
            Piece::Year => {
                rest_matches(&self.undated)
                    || name.get(..4).is_some_and(|year| {
                        year.bytes().all(|b| b.is_ascii_digit()) && rest_matches(year)
                    })
            }
            Piece::ActivityType => self.activity_names.values().any(|n| rest_matches(n)),
            Piece::TripType => self.trip_kind_names.values().any(|n| rest_matches(n)),
            Piece::Collection => unreachable!("not a trip template placeholder"),
        }
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

fn parse_template(template: &str, kind: Template) -> Result<Vec<Vec<Piece>>, ConfigError> {
    if template.is_empty() {
        return Err(ConfigError::EmptyTemplate(kind.key()));
    }
    let segments: Vec<&str> = template.split('/').collect();
    let mut parsed = Vec::with_capacity(segments.len());
    for segment in segments {
        if segment.is_empty() {
            return Err(ConfigError::EmptySegment {
                key: kind.key(),
                template: template.to_string(),
            });
        }
        parsed.push(parse_segment(segment, kind)?);
    }
    Ok(parsed)
}

fn parse_segment(segment: &str, kind: Template) -> Result<Vec<Piece>, ConfigError> {
    let unmatched = || ConfigError::UnmatchedBrace {
        key: kind.key(),
        segment: segment.to_string(),
    };
    let mut pieces = Vec::new();
    let mut rest = segment;
    while !rest.is_empty() {
        match rest.find('{') {
            None => {
                if rest.contains('}') {
                    return Err(unmatched());
                }
                pieces.push(Piece::Literal(rest.to_string()));
                rest = "";
            }
            Some(open) => {
                let literal = &rest[..open];
                if literal.contains('}') {
                    return Err(unmatched());
                }
                if !literal.is_empty() {
                    pieces.push(Piece::Literal(literal.to_string()));
                }
                let after = &rest[open + 1..];
                let Some(close) = after.find('}') else {
                    return Err(unmatched());
                };
                let name = &after[..close];
                let Some(piece) = kind.placeholder(name) else {
                    return Err(ConfigError::UnknownPlaceholder {
                        key: kind.key(),
                        found: name.to_string(),
                        supported: kind.supported(),
                    });
                };
                pieces.push(piece);
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
