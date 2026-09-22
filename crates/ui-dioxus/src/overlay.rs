//! One thing over the screen (US-62): the photo viewer, and the trip's edit
//! form, which opens from a control at the foot of a long screen and so is
//! shown over it rather than somewhere a scroll away from the button.
//!
//! Escape closes it, and the page behind it does not scroll while it is open —
//! a scroll there would move a screen the owner cannot see, and on a phone a
//! swipe meant for the overlay would be taken by it.

use dioxus::prelude::*;

use crate::interop;

/// The overlay itself. Focused when it opens, so the keys reach it without a
/// click first; `on_key` hears every key but Escape, which is its own.
#[component]
pub fn Overlay(
    label: String,
    on_close: EventHandler<()>,
    #[props(default)] on_key: EventHandler<KeyboardEvent>,
    #[props(default)] class: String,
    children: Element,
) -> Element {
    use_hook(|| interop::hold_page_scroll(true));
    use_drop(|| interop::hold_page_scroll(false));

    rsx! {
        div {
            class: "overlay {class}",
            role: "dialog",
            aria_modal: "true",
            aria_label: "{label}",
            tabindex: "-1",
            onmounted: move |event| async move {
                // Best effort: without focus the keys still work after a
                // click, and the buttons always do.
                let _ = event.data().set_focus(true).await;
            },
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    event.prevent_default();
                    on_close.call(());
                } else {
                    on_key.call(event);
                }
            },
            {children}
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    #[test]
    fn an_overlay_is_a_named_dialog_around_what_it_shows() {
        // The keys and the scroll lock are events and a DOM write; they are
        // the browser layer's to assert (ADR-0012's 2026-08-26b rule).
        let html = render(|| {
            rsx! {
                Overlay { label: "Edit trip", on_close: move |_| {},
                    p { "inside" }
                }
            }
        });

        assert!(html.contains(r#"role="dialog""#), "{html}");
        assert!(html.contains(r#"aria-label="Edit trip""#), "{html}");
        assert!(html.contains("<p>inside</p>"), "{html}");
    }
}
