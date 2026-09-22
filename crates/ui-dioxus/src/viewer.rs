//! Looking at a photo properly (US-62): one photo over the screen at the size
//! the archive holds it, and the set it was opened from to browse — the
//! trip's photos from the gallery, or one marker's group from its popup.
//!
//! Ordinary markup and CSS rather than a lightbox library: it is an overlay,
//! one photo and two buttons, and a vendored asset would be out of proportion
//! to that. Previous/next buttons, the arrow keys and a horizontal swipe all
//! step through the set; none of them wraps, so it is clear where a set ends.

use dioxus::prelude::*;

use crate::overlay::Overlay;
use crate::photos::PhotoView;

/// How far a finger has to travel sideways, in CSS pixels, before a drag
/// counts as a swipe rather than a tap that wandered.
const SWIPE_PX: f64 = 50.0;

/// The photo `delta` steps from `at` in a set of `len`, stopping at either
/// end rather than wrapping round.
pub fn step(at: usize, len: usize, delta: isize) -> usize {
    at.saturating_add_signed(delta).min(len.saturating_sub(1))
}

/// Which way a horizontal drag of `dx` pixels steps: a swipe to the left
/// brings the next photo in, as a page turns.
pub fn swipe(dx: f64) -> isize {
    if dx <= -SWIPE_PX {
        1
    } else if dx >= SWIPE_PX {
        -1
    } else {
        0
    }
}

/// The viewer, open on `photos[start]`. Rendered only while it is open; the
/// screen that opens it closes it through `on_close`.
#[component]
pub fn PhotoViewer(photos: Vec<PhotoView>, start: usize, on_close: EventHandler<()>) -> Element {
    let len = photos.len();
    let mut at = use_signal(|| step(start, len, 0));
    // Where the finger went down, while it is down.
    let mut swipe_from = use_signal(|| None::<f64>);
    let mut go = move |delta: isize| at.set(step(at(), len, delta));
    let Some(photo) = photos.get(at()).cloned() else {
        return rsx! {};
    };

    rsx! {
        Overlay {
            label: "Photo viewer",
            class: "viewer",
            on_close,
            on_key: move |event: KeyboardEvent| match event.key() {
                Key::ArrowLeft => go(-1),
                Key::ArrowRight => go(1),
                _ => {}
            },
            figure {
                class: "viewer-photo",
                onpointerdown: move |event| swipe_from.set(Some(event.client_coordinates().x)),
                onpointerup: move |event| {
                    if let Some(from) = swipe_from.take() {
                        go(swipe(event.client_coordinates().x - from));
                    }
                },
                onpointercancel: move |_| swipe_from.set(None),
                img { src: "{photo.url}", alt: "{photo.name}", draggable: "false" }
                figcaption {
                    span { "{photo.name}" }
                    if let Some(caption) = photo.caption.clone() {
                        span { id: "viewer-caption", "{caption}" }
                    }
                    span { id: "viewer-place", "{at() + 1} / {len}" }
                }
            }
            // The ends are `aria-disabled` rather than `disabled`: a disabled
            // button drops the focus, which lands on `body`, outside the
            // overlay — and the arrow keys go dead with it. A step past an
            // end is a no-op anyway (`step`).
            div { class: "viewer-controls",
                button {
                    id: "viewer-prev",
                    r#type: "button",
                    aria_disabled: "{at() == 0}",
                    onclick: move |_| go(-1),
                    "‹ Previous"
                }
                button {
                    id: "viewer-next",
                    r#type: "button",
                    aria_disabled: "{at() + 1 >= len}",
                    onclick: move |_| go(1),
                    "Next ›"
                }
                button {
                    id: "viewer-close",
                    r#type: "button",
                    onclick: move |_| on_close.call(()),
                    "Close"
                }
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    fn a_view(name: &str) -> PhotoView {
        PhotoView {
            thumbnail_url: format!("http://archive.test/media/thumb-{name}"),
            url: format!("http://archive.test/media/{name}"),
            name: name.to_string(),
            caption: Some("10:15 (+02:00)".to_string()),
        }
    }

    fn set(names: &[&str]) -> Vec<PhotoView> {
        names.iter().map(|name| a_view(name)).collect()
    }

    fn viewer(photos: Vec<PhotoView>, start: usize) -> String {
        render(move || {
            rsx! { PhotoViewer { photos: photos.clone(), start, on_close: move |_| {} } }
        })
    }

    /// The opening tag of the element carrying `id`.
    fn tag<'a>(html: &'a str, id: &str) -> &'a str {
        let at = html.find(&format!(r#"id="{id}""#)).expect(id);
        let open = html[..at].rfind('<').unwrap();
        let close = at + html[at..].find('>').unwrap();
        &html[open..=close]
    }

    #[test]
    fn the_viewer_opens_on_the_photo_it_was_opened_from_at_full_size() {
        let html = viewer(set(&["a.jpg", "b.jpg", "c.jpg"]), 1);

        assert!(
            html.contains(r#"src="http://archive.test/media/b.jpg""#),
            "{html}"
        );
        assert!(
            !html.contains("thumb-"),
            "the full-size image, not a thumbnail: {html}"
        );
        assert!(html.contains("10:15 (+02:00)"), "{html}");
        assert!(html.contains("2 / 3"), "{html}");
    }

    #[test]
    fn the_ends_of_a_set_disable_the_way_past_them() {
        let disabled = r#"aria-disabled="true""#;
        let first = viewer(set(&["a.jpg", "b.jpg"]), 0);
        assert!(tag(&first, "viewer-prev").contains(disabled), "{first}");
        assert!(!tag(&first, "viewer-next").contains(disabled), "{first}");

        let last = viewer(set(&["a.jpg", "b.jpg"]), 1);
        assert!(!tag(&last, "viewer-prev").contains(disabled), "{last}");
        assert!(tag(&last, "viewer-next").contains(disabled), "{last}");
    }

    #[test]
    fn a_photo_without_a_capture_time_has_no_caption() {
        let photos = vec![PhotoView {
            caption: None,
            ..a_view("a.jpg")
        }];

        let html = viewer(photos, 0);

        assert!(!html.contains("viewer-caption"), "{html}");
    }

    #[test]
    fn stepping_stops_at_either_end_rather_than_wrapping() {
        assert_eq!(step(0, 3, 1), 1);
        assert_eq!(step(2, 3, 1), 2);
        assert_eq!(step(0, 3, -1), 0);
        assert_eq!(step(1, 3, -1), 0);
        // A start past the end of the set opens on its last photo.
        assert_eq!(step(7, 3, 0), 2);
    }

    #[test]
    fn a_swipe_left_brings_the_next_photo_and_a_short_drag_nothing() {
        assert_eq!(swipe(-120.0), 1);
        assert_eq!(swipe(120.0), -1);
        assert_eq!(swipe(-20.0), 0);
        assert_eq!(swipe(30.0), 0);
    }
}
