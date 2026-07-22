use serde::{Deserialize, Serialize};

/// A tag the owner can attach to trips to organize them (US-33). No hidden or
/// computed fields, so — unlike `Photo` (ADR-0015) — this doubles as both the
/// DB record and the API response shape, the same way `TripDetail` does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
}

/// Normalize owner-supplied tag text into the canonical stored form: trimmed,
/// lowercased (so "Hiking"/"hiking" collapse to one tag), and rejected if
/// empty, containing whitespace anywhere (the acceptance criteria's "no
/// spaces", generalized to all whitespace, not just the space character), or
/// containing a comma. The comma restriction (US-38) isn't part of the
/// original US-33 acceptance criteria — it exists so the trip-list filter
/// can safely encode a multi-tag selection as one comma-separated query
/// parameter (`filter::parse_tags`) with no ambiguity: since no stored tag
/// name can ever contain a comma, splitting that parameter on `,` can never
/// misinterpret one real tag as several.
pub fn normalize_tag_name(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("tag name cannot be empty".to_string());
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err("tag name cannot contain spaces".to_string());
    }
    if trimmed.contains(',') {
        return Err("tag name cannot contain a comma".to_string());
    }
    Ok(trimmed.to_lowercase())
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_lowercases_and_trims() {
        assert_eq!(normalize_tag_name("  Hiking  ").unwrap(), "hiking");
    }

    #[test]
    fn normalize_is_case_insensitive_for_equal_tags() {
        assert_eq!(
            normalize_tag_name("Hiking").unwrap(),
            normalize_tag_name("HIKING").unwrap()
        );
    }

    #[test]
    fn normalize_rejects_an_empty_string() {
        assert!(normalize_tag_name("").is_err());
    }

    #[test]
    fn normalize_rejects_a_whitespace_only_string() {
        assert!(normalize_tag_name("   ").is_err());
    }

    #[test]
    fn normalize_rejects_an_internal_space() {
        assert!(normalize_tag_name("day trip").is_err());
    }

    #[test]
    fn normalize_rejects_an_internal_tab_or_newline() {
        assert!(normalize_tag_name("day\ttrip").is_err());
        assert!(normalize_tag_name("day\ntrip").is_err());
    }

    #[test]
    fn normalize_rejects_a_comma() {
        assert!(normalize_tag_name("day,trip").is_err());
        assert!(normalize_tag_name(",").is_err());
    }

    #[test]
    fn normalize_accepts_hyphens_and_underscores() {
        assert_eq!(
            normalize_tag_name("multi-day_trip").unwrap(),
            "multi-day_trip"
        );
    }
}
