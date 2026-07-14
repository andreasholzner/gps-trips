//! `KomootHttpClient` itself talks to the real Komoot API and has no
//! automated tests of its own (see module docs, and US-27/`komoot_check`) —
//! only pure, I/O-free logic is unit-tested here. `KomootClient` consumers
//! (e.g. `komoot_sync`) get a hand-rolled test-double `impl KomootClient`.
//! Split out of the parent `komoot.rs` purely to keep that file under the
//! repo's 500-line cap.

use super::*;

#[test]
fn resolve_photo_url_fills_in_width_height_and_crop() {
    let template = "https://cdn.example/photo?width={width}&height={height}&crop={crop}";
    assert_eq!(
        resolve_photo_url(template, 800, 600, true),
        "https://cdn.example/photo?width=800&height=600&crop=true"
    );
}

#[test]
fn resolve_photo_url_encodes_crop_as_the_literal_words_not_1_or_0() {
    // Regression guard: CloudFront rejects crop=1/crop=0 with a 400
    // ("crop must be true or false"), confirmed against the real API.
    let template = "https://cdn.example/photo?crop={crop}";
    assert_eq!(
        resolve_photo_url(template, 1, 1, false),
        "https://cdn.example/photo?crop=false"
    );
}

// Regression guard: HAL APIs conventionally omit `_embedded` entirely
// for an empty collection rather than sending an explicit empty one —
// these must parse as empty, not error out and halt a sync.

#[test]
fn tours_response_parses_with_embedded_present() {
    let json = r#"{"_embedded": {"tours": []}}"#;
    let parsed: ToursResponse = serde_json::from_str(json).unwrap();
    assert!(parsed.embedded.tours.is_empty());
}

#[test]
fn tours_response_parses_as_empty_when_embedded_is_missing() {
    let json = r#"{}"#;
    let parsed: ToursResponse = serde_json::from_str(json).unwrap();
    assert!(parsed.embedded.tours.is_empty());
}

#[test]
fn cover_images_response_parses_with_embedded_present() {
    let json = r#"{"_embedded": {"items": []}}"#;
    let parsed: CoverImagesResponse = serde_json::from_str(json).unwrap();
    assert!(parsed.embedded.items.is_empty());
}

#[test]
fn cover_images_response_parses_as_empty_when_embedded_is_missing() {
    let json = r#"{}"#;
    let parsed: CoverImagesResponse = serde_json::from_str(json).unwrap();
    assert!(parsed.embedded.items.is_empty());
}
