//! Deleting a trip from its detail screen (US-9).

use dioxus::prelude::*;

use crate::api;
use crate::filters::Filters;
use crate::Route;

/// The delete control: a button that arms a confirmation, and the request
/// once it is answered.
///
/// Deleting takes the trip's photo blobs with it and, for a Komoot-sourced
/// trip, queues the tour for deletion on Komoot too (US-24) — so it is
/// confirmed first, and the confirmation says what goes.
#[component]
pub fn DeleteTrip(id: i64) -> Element {
    let base_url = use_context::<Signal<String>>();
    let mut arming = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    // The router shows the next trip through this same scope; an armed
    // confirmation must not survive the trip it was armed on.
    use_effect(use_reactive!(|id| {
        let _ = id;
        arming.set(false);
        error.set(None);
    }));

    rsx! {
        if arming() {
            ConfirmDelete {
                busy: deleting(),
                on_confirm: move |_| async move {
                    // One delete per confirmation: the second would answer
                    // 404 for a trip the first one successfully removed, and
                    // report a failure that did not happen.
                    if deleting() {
                        return;
                    }
                    deleting.set(true);
                    match api::delete_trip(&base_url(), id).await {
                        Ok(()) => {
                            // Replaced, not pushed: the screen behind this one
                            // is the trip that no longer exists, and Back
                            // should not lead to a 404.
                            navigator().replace(Route::TripList {
                                filters: Filters::default(),
                            });
                        }
                        // A refusal is often "not now" rather than "no": a
                        // "Sync now" run in flight answers 409 (US-26), and
                        // the archive says so in words worth showing.
                        Err(err) => {
                            arming.set(false);
                            error.set(Some(err.to_string()));
                        }
                    }
                    deleting.set(false);
                },
                on_cancel: move |_| arming.set(false),
            }
        } else {
            p {
                button {
                    id: "delete-trip",
                    r#type: "button",
                    class: "danger",
                    // The previous attempt's failure is not this one's:
                    // leaving it up would read as though this had failed too.
                    onclick: move |_| {
                        error.set(None);
                        arming.set(true);
                    },
                    "Delete trip"
                }
            }
        }
        if let Some(message) = error() {
            p { class: "error", "Could not delete this trip: {message}" }
        }
    }
}

/// The confirmation. In the page rather than a browser dialog, for the same
/// reasons the new-tag one is (ADR-0025's platform rule, and so it can be
/// tested without a browser).
#[component]
fn ConfirmDelete(
    #[props(default)] busy: bool,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    rsx! {
        p { class: "confirm",
            "Delete this trip and its photos? This cannot be undone. "
            button {
                r#type: "button",
                class: "danger",
                disabled: busy,
                onclick: move |_| on_confirm.call(()),
                "Delete it"
            }
            button { r#type: "button", onclick: move |_| on_cancel.call(()), "Cancel" }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    #[test]
    fn deleting_is_offered_but_not_armed() {
        let html = render(|| rsx! { DeleteTrip { id: 1 } });

        assert!(html.contains(r#"id="delete-trip""#), "{html}");
        assert!(html.contains("Delete trip"), "{html}");
        // Nothing that could delete on one stray click.
        assert!(!html.contains("cannot be undone"), "{html}");
    }

    #[test]
    fn deleting_is_confirmed_first_and_says_what_goes_with_the_trip() {
        // US-9 deletes the trip's photo blobs along with its rows; the owner
        // is told so before it happens, in the page rather than a browser
        // dialog so the Android webview behaves the same way.
        let html = render(|| {
            rsx! {
                ConfirmDelete {
                    on_confirm: move |_| {},
                    on_cancel: move |_| {},
                }
            }
        });

        assert!(html.contains("photos"), "{html}");
        assert!(html.contains("cannot be undone"), "{html}");
        assert!(html.contains("Cancel"), "{html}");
    }
}
