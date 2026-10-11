//! Importing a guide book's file as a new collection (US-86): the third
//! choice on the import screen beside Recorded and Planned, *Collection*
//! (US-88). Once the file is chosen, the archive reads what it holds and
//! suggests the collection's name and description; the owner confirms or
//! changes them, picks an activity or none, and the import ends on the new
//! collection's page. Several files, or files for an existing collection,
//! are reviewed first (`review`).

use dioxus::prelude::*;
use trip_archive_types::CollectionPreview;

use crate::collection::{counts, CollectionFields, CollectionForm};

/// The import's choice for a collection, beside the trip kinds' wire values.
pub const COLLECTION: &str = "collection";

/// A guide book's file, read by the archive and waiting to be confirmed.
/// Nothing is created yet, so the bytes stay here to be sent again.
#[derive(Clone, Debug, PartialEq)]
pub struct ChosenBook {
    pub file_name: String,
    pub bytes: Vec<u8>,
    pub preview: CollectionPreview,
}

/// The form as the file's suggestions fill it in. No activity: a book with
/// no activity focus is the one case the owner cannot be asked to undo.
pub fn form_of(preview: &CollectionPreview) -> CollectionForm {
    CollectionForm {
        name: preview.suggested_name.clone(),
        description: preview.suggested_description.clone(),
        activity: String::new(),
    }
}

/// Step two for a guide book: the details, what the file holds, and the
/// way back to the picker.
#[component]
pub fn ConfirmGuideBookStep(
    preview: CollectionPreview,
    #[props(default)] error: Option<String>,
    #[props(default)] submitting: bool,
    on_confirm: EventHandler<CollectionForm>,
    on_start_over: EventHandler<()>,
) -> Element {
    let form = use_signal(|| form_of(&preview));
    let held = counts(preview.track_count, preview.waypoint_count, " and ");

    rsx! {
        form {
            id: "confirm-guide-book",
            onsubmit: move |event| {
                event.prevent_default();
                on_confirm.call(form.read().clone());
            },
            p { id: "guide-book-counts", "The file holds {held}." }
            CollectionFields { form }
            button {
                id: "import-confirm",
                r#type: "submit",
                disabled: submitting,
                if submitting { "Importing…" } else { "Import collection" }
            }
            button {
                id: "import-start-over",
                r#type: "button",
                disabled: submitting,
                onclick: move |_| on_start_over.call(()),
                "Choose a different file"
            }
        }
        if let Some(error) = error {
            p { class: "error", "{error}" }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    fn a_preview() -> CollectionPreview {
        CollectionPreview {
            suggested_name: "Lakes & Passes".to_string(),
            suggested_description: "Bruckmann, Baukasten".to_string(),
            track_count: 125,
            waypoint_count: 46,
        }
    }

    #[test]
    fn us86_the_details_start_as_the_files_suggestions_without_an_activity() {
        let form = form_of(&a_preview());

        assert_eq!(form.name, "Lakes & Passes");
        assert_eq!(form.description, "Bruckmann, Baukasten");
        assert_eq!(form.activity, "");
    }

    #[test]
    fn us86_the_step_offers_the_details_and_says_what_the_file_holds() {
        let html = render(|| {
            rsx! {
                ConfirmGuideBookStep {
                    preview: a_preview(),
                    on_confirm: move |_| {},
                    on_start_over: move |_| {},
                }
            }
        });

        assert!(html.contains("125 tracks and 46 waypoints"), "{html}");
        assert!(html.contains(r#"value="Lakes &#38; Passes""#), "{html}");
        assert!(html.contains("Bruckmann, Baukasten"), "{html}");
        assert!(html.contains(r#"id="collection-activity""#), "{html}");
        assert!(html.contains(r#"id="import-start-over""#), "{html}");
        // A guide book's tracks carry no photos (US-86).
        assert!(!html.contains("import-photos"), "{html}");
    }

    #[test]
    fn us88_a_collections_files_are_several_and_may_go_to_an_existing_one() {
        let html = render(|| {
            rsx! {
                super::super::steps::ChooseGpx {
                    reading: false,
                    kind: COLLECTION.to_string(),
                    collections: vec![(4, "Lakes".to_string()), (7, "Passes".to_string())],
                    target: Some(7),
                    on_choose: move |_| {},
                }
            }
        });

        assert!(html.contains(r#"id="import-target""#), "{html}");
        assert!(html.contains("A new collection"), "{html}");
        assert!(html.contains(r#"value="7" selected"#), "{html}");
        assert!(html.contains("Lakes"), "{html}");
        assert!(html.contains("multiple"), "{html}");
    }

    #[test]
    fn us88_a_trip_takes_one_file_and_no_collection() {
        let html = render(|| {
            rsx! { super::super::steps::ChooseGpx { reading: false, on_choose: move |_| {} } }
        });

        assert!(!html.contains(r#"id="import-target""#), "{html}");
        assert!(!html.contains("multiple"), "{html}");
    }
}
