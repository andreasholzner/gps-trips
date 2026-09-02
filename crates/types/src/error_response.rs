use serde::{Deserialize, Serialize};

/// The body a failed API request answers with: the archive's own account of
/// what went wrong, as JSON like everything else the API says (ADR-0008).
///
/// One sentence, meant for the owner and not for a programmer — the screens
/// show it verbatim, which is why a refusal is worded rather than reduced to
/// a status code. The status stays the machine-readable half: the SPA tells a
/// trip that is simply gone (404) from an edit refused while a sync runs
/// (409) by reading it, never by matching on this string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl ErrorResponse {
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_is_one_worded_field() {
        let json = serde_json::to_string(&ErrorResponse::new("Not found")).unwrap();
        assert_eq!(json, r#"{"error":"Not found"}"#);
    }
}
