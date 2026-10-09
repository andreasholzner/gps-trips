//! Renaming a tag in its row (US-85): the name turns into a field holding
//! it, saved or cancelled in place. The archive normalizes and validates the
//! new name and says why it refuses one, as it does when creating a tag.

use dioxus::prelude::*;

use crate::api::{self, ApiClient};

/// The rename form for tag `id`, starting on its current `name`. Enter saves
/// and Escape cancels. A refused name keeps the form open under its reason.
#[component]
pub fn RenameTag(
    id: i64,
    name: String,
    on_renamed: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut typed = use_signal(|| name.clone());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    rsx! {
        form {
            class: "rename-tag",
            onsubmit: move |event| async move {
                event.prevent_default();
                // One rename per save, as deleting allows one delete.
                if saving() {
                    return;
                }
                saving.set(true);
                match api::rename_tag(&archive(), id, &typed()).await {
                    Ok(_) => on_renamed.call(()),
                    Err(err) => error.set(Some(err.to_string())),
                }
                saving.set(false);
            },
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    on_cancel.call(());
                }
            },
            input {
                aria_label: "New name for “{name}”",
                value: "{typed}",
                onmounted: move |event| async move {
                    // Best effort: without focus the field is a click away.
                    let _ = event.data().set_focus(true).await;
                },
                oninput: move |event| typed.set(event.value()),
            }
            button { r#type: "submit", disabled: saving(), "Save" }
            button { r#type: "button", class: "secondary", onclick: move |_| on_cancel.call(()), "Cancel" }
        }
        if let Some(message) = error() {
            p { class: "error", "Could not rename the tag: {message}" }
        }
    }
}
