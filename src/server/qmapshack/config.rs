//! Owner-facing export configuration (US-36, ADR-0022): one TOML file
//! holding the target database path and the folder-mapping template.
//!
//! ```toml
//! target_db = "/home/owner/qms/Touren.db"
//! folder_template = "Trips/{year}/{activity_type}"
//! undated = "undated"                  # optional {year} fallback
//!
//! [activity_type_names]                # optional, keys = wire names
//! ski_touring = "Skitour"
//! [trip_type_names]
//! planned = "Geplant"
//! ```
//!
//! Everything is validated up front (validate at the boundary): after
//! `from_toml_str` succeeds, folder resolution is infallible.

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
        self.activity_names
            .get(&activity)
            .cloned()
            .unwrap_or_else(|| {
                match activity {
                    // label() is a UI picker string ("— unspecified —"), not a
                    // usable folder name.
                    ActivityType::Unknown => "unspecified".to_string(),
                    other => other.label().to_string(),
                }
            })
    }

    fn trip_kind_name(&self, kind: TripKind) -> String {
        self.trip_kind_names
            .get(&kind)
            .cloned()
            .unwrap_or_else(|| kind.label().to_string())
    }
}

/// Every valid `[activity_type_names]` key: the wire names, incl. `unknown`.
fn activity_type_keys() -> Vec<String> {
    std::iter::once(ActivityType::Unknown)
        .chain(ActivityType::SELECTABLE)
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
mod tests {
    use super::*;

    const MINIMAL: &str =
        "target_db = \"/tmp/t.db\"\nfolder_template = \"Trips/{year}/{activity_type}\"\n";

    fn config(toml: &str) -> ExportConfig {
        ExportConfig::from_toml_str(toml).expect("valid config")
    }

    #[test]
    fn minimal_config_parses_with_defaults() {
        let cfg = config(MINIMAL);
        assert_eq!(cfg.target_db, PathBuf::from("/tmp/t.db"));
        assert_eq!(
            cfg.resolve_folder_path(
                ActivityType::Hiking,
                TripKind::Recorded,
                Some("2024-06-01T08:00:00Z"),
            ),
            ["Trips", "2024", "Hiking"]
        );
    }

    #[test]
    fn year_falls_back_to_undated_for_trips_without_start_time() {
        let cfg = config(MINIMAL);
        let path = cfg.resolve_folder_path(ActivityType::Hiking, TripKind::Recorded, None);
        assert_eq!(path, ["Trips", "undated", "Hiking"]);

        let custom = config(&format!("{MINIMAL}undated = \"ohne Datum\"\n"));
        let path = custom.resolve_folder_path(ActivityType::Hiking, TripKind::Recorded, None);
        assert_eq!(path[1], "ohne Datum");
    }

    #[test]
    fn year_never_panics_on_a_malformed_start_time() {
        let cfg = config(MINIMAL);
        // Byte 4 splits the second 'λ' — a char-boundary panic trap for
        // naive slicing. Malformed timestamps fall back to the undated
        // bucket instead of taking the run down.
        let path = cfg.resolve_folder_path(ActivityType::Hiking, TripKind::Recorded, Some("aλλ"));
        assert_eq!(path[1], "undated");
    }

    #[test]
    fn unknown_activity_resolves_to_unspecified_not_the_ui_label() {
        let cfg = config(MINIMAL);
        let path = cfg.resolve_folder_path(ActivityType::Unknown, TripKind::Recorded, None);
        assert_eq!(path[2], "unspecified");
    }

    #[test]
    fn name_tables_override_activity_and_trip_type_spellings() {
        let cfg = config(
            "target_db = \"/tmp/t.db\"\n\
             folder_template = \"{trip_type}/{activity_type}\"\n\
             [activity_type_names]\n\
             ski_touring = \"Skitour\"\n\
             [trip_type_names]\n\
             planned = \"Geplant\"\n",
        );
        assert_eq!(
            cfg.resolve_folder_path(ActivityType::SkiTouring, TripKind::Planned, None),
            ["Geplant", "Skitour"]
        );
        // Unoverridden values keep their label() defaults.
        assert_eq!(
            cfg.resolve_folder_path(ActivityType::Hiking, TripKind::Recorded, None),
            ["Recorded", "Hiking"]
        );
    }

    #[test]
    fn segments_may_mix_literals_and_placeholders() {
        let cfg =
            config("target_db = \"/tmp/t.db\"\nfolder_template = \"Archiv {year}-{trip_type}\"\n");
        assert_eq!(
            cfg.resolve_folder_path(
                ActivityType::Hiking,
                TripKind::Recorded,
                Some("2023-01-01T00:00:00Z"),
            ),
            ["Archiv 2023-Recorded"]
        );
    }

    #[test]
    fn unknown_toml_keys_are_rejected() {
        let err = ExportConfig::from_toml_str(&format!("{MINIMAL}folder_tempalte = \"x\"\n"))
            .expect_err("typo'd key");
        assert!(matches!(err, ConfigError::Toml(_)), "{err}");
    }

    #[test]
    fn empty_target_db_is_rejected() {
        let err = ExportConfig::from_toml_str("target_db = \"\"\nfolder_template = \"Trips\"\n")
            .expect_err("empty target_db");
        assert!(matches!(err, ConfigError::EmptyTargetDb), "{err}");
    }

    #[test]
    fn empty_or_slash_delimited_templates_are_rejected() {
        for template in ["", "/Trips", "Trips/", "Trips//X"] {
            let toml = format!("target_db = \"/tmp/t.db\"\nfolder_template = \"{template}\"\n");
            let err = ExportConfig::from_toml_str(&toml).expect_err(template);
            assert!(
                matches!(
                    err,
                    ConfigError::EmptyTemplate | ConfigError::EmptySegment(_)
                ),
                "{template:?} → {err}"
            );
        }
    }

    #[test]
    fn unknown_placeholders_are_rejected_and_named() {
        let toml = "target_db = \"/tmp/t.db\"\nfolder_template = \"Trips/{month}\"\n";
        let err = ExportConfig::from_toml_str(toml).expect_err("unknown placeholder");
        assert!(
            matches!(&err, ConfigError::UnknownPlaceholder { found } if found == "month"),
            "{err}"
        );
        assert!(err.to_string().contains("{year}"), "lists supported set");
    }

    #[test]
    fn unmatched_braces_are_rejected() {
        for template in ["Trips/{year", "Trips/year}", "{yea{r}"] {
            let toml = format!("target_db = \"/tmp/t.db\"\nfolder_template = \"{template}\"\n");
            let err = ExportConfig::from_toml_str(&toml).expect_err(template);
            assert!(
                matches!(
                    err,
                    ConfigError::UnmatchedBrace(_) | ConfigError::UnknownPlaceholder { .. }
                ),
                "{template:?} → {err}"
            );
        }
    }

    #[test]
    fn name_table_keys_must_be_wire_names() {
        let toml = format!("{MINIMAL}[activity_type_names]\nskitouring = \"Skitour\"\n");
        let err = ExportConfig::from_toml_str(&toml).expect_err("bad key");
        match &err {
            ConfigError::UnknownNameKey { table, key, valid } => {
                assert_eq!(*table, "activity_type_names");
                assert_eq!(key, "skitouring");
                assert!(valid.contains("ski_touring"), "lists valid keys: {valid}");
            }
            other => panic!("unexpected error {other}"),
        }
    }

    #[test]
    fn folder_name_values_must_not_be_empty_or_contain_slashes() {
        for bad in [
            format!("{MINIMAL}undated = \"a/b\"\n"),
            format!("{MINIMAL}[trip_type_names]\nplanned = \"\"\n"),
        ] {
            let err = ExportConfig::from_toml_str(&bad).expect_err("bad name value");
            assert!(matches!(err, ConfigError::BadName { .. }), "{err}");
        }
    }

    #[test]
    fn load_reports_a_missing_file_with_its_path() {
        let err = ExportConfig::load(Path::new("/nonexistent/qms.toml")).expect_err("missing");
        assert!(matches!(err, ConfigError::Io { .. }), "{err}");
        assert!(err.to_string().contains("/nonexistent/qms.toml"));
    }
}
