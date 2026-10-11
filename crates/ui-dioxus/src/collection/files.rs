//! The files a collection was imported and added from (US-88), each
//! downloadable as it was uploaded.

use dioxus::prelude::*;

use crate::api::{self, ApiClient};

/// The kept files in the order they came, as download links. Files that
/// could not be listed cost the list, not the page.
#[component]
pub(super) fn CollectionFiles(id: i64) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let files = use_resource(use_reactive!(|id| async move {
        api::list_collection_files(&archive(), id).await
    }));

    match &*files.read_unchecked() {
        Some(Ok(files)) if !files.is_empty() => rsx! {
            section { id: "collection-files",
                h2 { "Files" }
                ul {
                    for file in files.clone() {
                        li { key: "{file.id}",
                            a {
                                href: api::collection_file_url(&archive(), file.id),
                                download: "{file.file_name}",
                                "{file.file_name}"
                            }
                        }
                    }
                }
            }
        },
        Some(Err(err)) => {
            dioxus::logger::tracing::error!("could not list the files: {err}");
            rsx! {}
        }
        _ => rsx! {},
    }
}
