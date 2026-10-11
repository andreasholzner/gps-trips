//! US-36/US-39/US-89: parsing and validating the export config.

use super::*;
use crate::server::archive_client::config::ArchiveConfigError;

const MINIMAL: &str = "url = \"https://archive.test\"\ntarget_db = \"/tmp/t.db\"\n\
     folder_template = \"Trips/{year}/{activity_type}\"\n\
     collection_folder_template = \"Guidebooks/{collection}\"\n";

/// `[activity_type_names]`/`[trip_type_names]` tables covering every
/// `ActivityType` (incl. `Unknown`) and `TripKind` variant. US-39 makes
/// mapping every variant mandatory, so any test exercising a successful
/// parse needs this. Scalar keys (e.g. `undated`) must be interpolated
/// *before* this constant — TOML requires bare keys to precede table
/// headers in the same document.
const FULL_NAME_TABLES: &str = "\
[activity_type_names]\n\
unknown = \"Unspecified\"\n\
hiking = \"Hiking\"\n\
mountaineering = \"Mountaineering\"\n\
cycling = \"Cycling\"\n\
bikepacking = \"Bikepacking\"\n\
kayaking = \"Kayaking\"\n\
ski_touring = \"Ski touring\"\n\
cross_country_skiing = \"Cross-country skiing\"\n\
snow_shoe = \"Snowshoeing\"\n\
[trip_type_names]\n\
recorded = \"Recorded\"\n\
planned = \"Planned\"\n";

/// `MINIMAL` plus any extra scalar keys plus a complete name mapping —
/// the smallest config that actually parses under US-39.
fn complete(extra_scalars: &str) -> String {
    format!("{MINIMAL}{extra_scalars}{FULL_NAME_TABLES}")
}

fn config(toml: &str) -> ExportConfig {
    ExportConfig::from_toml_str(toml).expect("valid config")
}

#[test]
fn minimal_config_parses_with_full_mapping() {
    let cfg = config(&complete(""));
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
    let cfg = config(&complete(""));
    let path = cfg.resolve_folder_path(ActivityType::Hiking, TripKind::Recorded, None);
    assert_eq!(path, ["Trips", "undated", "Hiking"]);

    let custom = config(&complete("undated = \"ohne Datum\"\n"));
    let path = custom.resolve_folder_path(ActivityType::Hiking, TripKind::Recorded, None);
    assert_eq!(path[1], "ohne Datum");
}

#[test]
fn year_never_panics_on_a_malformed_start_time() {
    let cfg = config(&complete(""));
    // Byte 4 splits the second 'λ' — a char-boundary panic trap for
    // naive slicing. Malformed timestamps fall back to the undated
    // bucket instead of taking the run down.
    let path = cfg.resolve_folder_path(ActivityType::Hiking, TripKind::Recorded, Some("aλλ"));
    assert_eq!(path[1], "undated");
}

#[test]
fn mapped_unknown_activity_resolves_to_its_configured_name() {
    let cfg = config(&complete(""));
    let path = cfg.resolve_folder_path(ActivityType::Unknown, TripKind::Recorded, None);
    // FULL_NAME_TABLES maps unknown -> "Unspecified"; there is no more
    // hardcoded "unspecified" fallback (US-39 requires an explicit entry).
    assert_eq!(path[2], "Unspecified");
}

#[test]
fn omitting_unknown_from_activity_type_names_is_an_incomplete_mapping_error() {
    let toml = format!(
        "{MINIMAL}{}",
        FULL_NAME_TABLES.replace("unknown = \"Unspecified\"\n", "")
    );
    let err = ExportConfig::from_toml_str(&toml).expect_err("unknown must be mapped");
    match &err {
        ConfigError::IncompleteMapping(msg) => {
            assert!(msg.contains("activity_type_names"), "{msg}");
            assert!(msg.contains("unknown"), "{msg}");
        }
        other => panic!("unexpected error {other}"),
    }
}

#[test]
fn incomplete_mapping_lists_every_missing_entry_across_both_tables() {
    // Only `hiking` mapped; every other activity type and both trip
    // kinds are missing. Acceptance criteria (US-39): all missing
    // entries are listed together in one error, not just the first.
    let toml = format!("{MINIMAL}[activity_type_names]\nhiking = \"Hiking\"\n");
    let err = ExportConfig::from_toml_str(&toml).expect_err("incomplete mapping");
    match &err {
        ConfigError::IncompleteMapping(msg) => {
            assert!(msg.contains("activity_type_names"), "{msg}");
            for missing in [
                "unknown",
                "mountaineering",
                "cycling",
                "bikepacking",
                "kayaking",
                "ski_touring",
                "cross_country_skiing",
                "snow_shoe",
            ] {
                assert!(
                    msg.contains(missing),
                    "missing {missing:?} not listed: {msg}"
                );
            }
            assert!(msg.contains("trip_type_names"), "{msg}");
            assert!(msg.contains("recorded"), "{msg}");
            assert!(msg.contains("planned"), "{msg}");
        }
        other => panic!("unexpected error {other}"),
    }
}

#[test]
fn complete_mapping_of_every_variant_is_accepted() {
    config(&complete(""));
}

#[test]
fn name_tables_override_activity_and_trip_type_spellings() {
    let toml = format!(
        "url = \"https://archive.test\"\ntarget_db = \"/tmp/t.db\"\n\
         folder_template = \"{{trip_type}}/{{activity_type}}\"\n\
         collection_folder_template = \"{{collection}}\"\n\
         {}",
        FULL_NAME_TABLES
            .replace("ski_touring = \"Ski touring\"", "ski_touring = \"Skitour\"")
            .replace("planned = \"Planned\"", "planned = \"Geplant\"")
    );
    let cfg = config(&toml);
    assert_eq!(
        cfg.resolve_folder_path(ActivityType::SkiTouring, TripKind::Planned, None),
        ["Geplant", "Skitour"]
    );
    // Unoverridden values keep their configured (non-Skitour/Geplant) names.
    assert_eq!(
        cfg.resolve_folder_path(ActivityType::Hiking, TripKind::Recorded, None),
        ["Recorded", "Hiking"]
    );
}

#[test]
fn segments_may_mix_literals_and_placeholders() {
    let toml = format!(
        "url = \"https://archive.test\"\ntarget_db = \"/tmp/t.db\"\nfolder_template = \"Archiv {{year}}-{{trip_type}}\"\n\
         collection_folder_template = \"{{collection}}\"\n{FULL_NAME_TABLES}"
    );
    let cfg = config(&toml);
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
    let err = ExportConfig::from_toml_str(
        "url = \"https://archive.test\"\ntarget_db = \"\"\nfolder_template = \"Trips\"\n",
    )
    .expect_err("empty target_db");
    assert!(matches!(err, ConfigError::EmptyTargetDb), "{err}");
}

#[test]
fn empty_or_slash_delimited_templates_are_rejected() {
    for template in ["", "/Trips", "Trips/", "Trips//X"] {
        let toml = format!("url = \"https://archive.test\"\ntarget_db = \"/tmp/t.db\"\nfolder_template = \"{template}\"\n");
        let err = ExportConfig::from_toml_str(&toml).expect_err(template);
        assert!(
            matches!(
                err,
                ConfigError::EmptyTemplate(_) | ConfigError::EmptySegment { .. }
            ),
            "{template:?} → {err}"
        );
    }
}

#[test]
fn unknown_placeholders_are_rejected_and_named() {
    let toml = "url = \"https://archive.test\"\ntarget_db = \"/tmp/t.db\"\nfolder_template = \"Trips/{month}\"\n";
    let err = ExportConfig::from_toml_str(toml).expect_err("unknown placeholder");
    assert!(
        matches!(&err, ConfigError::UnknownPlaceholder { found, .. } if found == "month"),
        "{err}"
    );
    assert!(err.to_string().contains("{year}"), "lists supported set");
}

#[test]
fn unmatched_braces_are_rejected() {
    for template in ["Trips/{year", "Trips/year}", "{yea{r}"] {
        let toml = format!("url = \"https://archive.test\"\ntarget_db = \"/tmp/t.db\"\nfolder_template = \"{template}\"\n");
        let err = ExportConfig::from_toml_str(&toml).expect_err(template);
        assert!(
            matches!(
                err,
                ConfigError::UnmatchedBrace { .. } | ConfigError::UnknownPlaceholder { .. }
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

#[test]
fn us51_the_archive_url_and_password_command_load() {
    let cfg = config(&complete("password_command = \"pass show archive\"\n"));
    assert_eq!(cfg.url, "https://archive.test");
    assert_eq!(cfg.password_command.as_deref(), Some("pass show archive"));
    assert_eq!(config(&complete("")).password_command, None);
}

#[test]
fn us51_the_archive_url_is_required() {
    let toml = complete("").replace("url = \"https://archive.test\"\n", "");
    let err = ExportConfig::from_toml_str(&toml).expect_err("no url");
    assert!(matches!(err, ConfigError::Toml(_)), "{err}");
}

#[test]
fn us51_the_archive_url_must_be_https() {
    // The password travels to it: never in the clear.
    let toml = complete("").replace("https://archive.test", "http://archive.test");
    let err = ExportConfig::from_toml_str(&toml).expect_err("plain http");
    assert!(
        matches!(err, ConfigError::Archive(ArchiveConfigError::BadUrl(_))),
        "{err}"
    );
}

#[test]
fn us51_an_empty_password_command_is_rejected() {
    let err = ExportConfig::from_toml_str(&complete("password_command = \" \"\n"))
        .expect_err("blank command");
    assert!(
        matches!(
            err,
            ConfigError::Archive(ArchiveConfigError::EmptyPasswordCommand)
        ),
        "{err}"
    );
}

// ── US-89: the collections' folder template ──────────────────────────────────

/// A complete config whose two templates are the given ones.
fn with_templates(trips: &str, collections: &str) -> ExportConfig {
    config(&format!(
        "url = \"https://archive.test\"\ntarget_db = \"/tmp/t.db\"\n\
         folder_template = {trips:?}\ncollection_folder_template = {collections:?}\n\
         {FULL_NAME_TABLES}"
    ))
}

#[test]
fn us89_a_collection_resolves_to_a_folder_of_its_own() {
    let cfg = config(&complete(""));
    assert_eq!(
        cfg.resolve_collection_folder_path("Rother: Jotunheimen"),
        ["Guidebooks", "Rother: Jotunheimen"]
    );
    let cfg = with_templates("Trips/{year}", "Books/Guide {collection}");
    assert_eq!(
        cfg.resolve_collection_folder_path("Lofoten"),
        ["Books", "Guide Lofoten"]
    );
}

#[test]
fn us89_a_missing_collection_template_is_listed_with_the_other_missing_entries() {
    let toml = format!(
        "{}[activity_type_names]\nhiking = \"Hiking\"\n",
        MINIMAL.replace(
            "collection_folder_template = \"Guidebooks/{collection}\"\n",
            ""
        )
    );
    let err = ExportConfig::from_toml_str(&toml).expect_err("no collection template");
    match &err {
        ConfigError::IncompleteMapping(msg) => {
            assert!(msg.contains("collection_folder_template"), "{msg}");
            assert!(msg.contains("activity_type_names"), "{msg}");
            assert!(msg.contains("trip_type_names"), "{msg}");
        }
        other => panic!("unexpected error {other}"),
    }
}

#[test]
fn us89_the_collection_template_must_name_the_collection() {
    let toml = complete("").replace("Guidebooks/{collection}", "Guidebooks");
    let err = ExportConfig::from_toml_str(&toml).expect_err("one folder for every collection");
    assert!(matches!(err, ConfigError::NoCollectionPlaceholder), "{err}");
}

#[test]
fn us89_each_template_takes_only_its_own_placeholders() {
    let toml = complete("").replace("Guidebooks/{collection}", "Guidebooks/{year}/{collection}");
    let err = ExportConfig::from_toml_str(&toml).expect_err("a trip placeholder");
    assert!(
        matches!(&err, ConfigError::UnknownPlaceholder { key, found, .. }
            if *key == "collection_folder_template" && found == "year"),
        "{err}"
    );
    assert!(
        err.to_string().contains("{collection}"),
        "lists its own: {err}"
    );

    let toml = complete("").replace("Trips/{year}/{activity_type}", "Trips/{collection}");
    let err = ExportConfig::from_toml_str(&toml).expect_err("a collection placeholder");
    assert!(
        matches!(&err, ConfigError::UnknownPlaceholder { key, found, .. }
            if *key == "folder_template" && found == "collection"),
        "{err}"
    );
}

#[test]
fn us89_collections_under_a_top_folder_of_their_own_are_not_warned_about() {
    assert_eq!(config(&complete("")).collection_folder_overlap(), None);
    // A trips' top folder by year can only be four digits or the undated
    // name; by activity, only a configured name.
    assert_eq!(
        with_templates("{year}/{activity_type}", "Guidebooks/{collection}")
            .collection_folder_overlap(),
        None
    );
    assert_eq!(
        with_templates("{activity_type}", "Guidebooks/{collection}").collection_folder_overlap(),
        None
    );
}

#[test]
fn us89_a_top_folder_the_trips_can_have_too_is_warned_about() {
    for (trips, collections) in [
        ("Trips/{year}", "Trips/Guides/{collection}"),
        ("{activity_type}/{year}", "Hiking/{collection}"),
        ("Archiv {year}", "Archiv 2024/{collection}"),
        ("{year}", "undated/{collection}"),
        (
            "{trip_type}-{activity_type}",
            "Planned-Cycling/{collection}",
        ),
    ] {
        let warning = with_templates(trips, collections).collection_folder_overlap();
        assert!(warning.is_some(), "{trips} vs {collections}");
    }
}

#[test]
fn us89_a_top_folder_named_after_each_collection_is_warned_about() {
    // A collection may be called anything, "Trips" too.
    let warning = with_templates("Trips/{year}", "{collection}").collection_folder_overlap();
    let warning = warning.expect("named after the collection");
    assert!(warning.contains("collection_folder_template"), "{warning}");
}
