//! The app's own navigation (US-60): one menu carrying everything the owner
//! does occasionally, rather than links sitting on the screens themselves.
//!
//! It is a layout route, so the router renders it around every screen and
//! `Link` has the context it needs — the sign-out control used to live
//! outside the router entirely, which is why it could not be a link.
//!
//! Narrow screens get a burger that opens a panel; from a tablet's width up
//! the same items sit inline in the header, which is a CSS question and not
//! a second component (`app.css`).

use dioxus::prelude::*;

use crate::filters::Filters;
use crate::Route;

/// Signing out, as the app hands it to this menu. The action needs the
/// session state `App` owns and a router-rendered component cannot be given
/// props, so it arrives as context instead — and the menu learns nothing
/// about how a session ends.
#[derive(Clone, Copy)]
pub struct SignOut(pub Callback<()>);

/// The shell every screen renders inside: the menu, then the screen.
#[component]
pub fn AppShell() -> Element {
    rsx! {
        AppMenu {}
        Outlet::<Route> {}
    }
}

/// The menu itself.
///
/// Open/closed is state rather than a `<details>` element because the panel
/// has to close on things the element knows nothing about — an item being
/// chosen, a click outside it, Escape. The click-outside is a backdrop
/// rendered behind the open panel: `onfocusout` would fire when focus moves
/// *between* the menu's own items, and telling that apart needs
/// `relatedTarget`, which means `web-sys` — unavailable on Android
/// (ADR-0025).
#[component]
pub fn AppMenu() -> Element {
    let mut open = use_signal(|| false);
    // Absent on the host target, on Android and in the web app installed
    // there, where signing out is not offered at all; `App` provides it in a
    // browser tab.
    let sign_out = try_use_context::<SignOut>();

    rsx! {
        header {
            class: "app-header",
            // On the header, not the panel: the menu is opened from the
            // burger and focus stays there, so a handler on the panel alone
            // would never see the key that is meant to close it.
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    open.set(false);
                }
            },
            button {
                r#type: "button",
                id: "app-menu-button",
                class: "burger",
                aria_expanded: "{open()}",
                aria_controls: "app-menu",
                aria_label: "Menu",
                onclick: move |_| open.toggle(),
                "☰"
            }
            // Rendered whether or not it is open: on a wide screen the items
            // are always on show, and which of the two it is belongs to the
            // stylesheet rather than to this component.
            nav {
                id: "app-menu",
                class: if open() { "app-menu open" } else { "app-menu" },
                Link {
                    to: Route::TripList { filters: Filters::default() },
                    onclick: move |_| open.set(false),
                    "All trips"
                }
                Link {
                    to: Route::ImportTrip {},
                    onclick: move |_| open.set(false),
                    "Import a trip"
                }
                Link {
                    to: Route::KomootSync {},
                    onclick: move |_| open.set(false),
                    "Sync with Komoot"
                }
                Link {
                    to: Route::Shares {},
                    onclick: move |_| open.set(false),
                    "Shares"
                }
                // In a browser tab only, deliberately — `offers_sign_out`
                // in main.rs says why (US-16, US-67).
                if let Some(SignOut(sign_out)) = sign_out {
                    button {
                        r#type: "button",
                        id: "sign-out",
                        onclick: move |_| {
                            open.set(false);
                            sign_out.call(());
                        },
                        "Sign out"
                    }
                }
            }
        }
        // Only while the panel is open, and only on a narrow screen — the
        // stylesheet hides it above the breakpoint, so resizing with the menu
        // open cannot leave an invisible sheet over the page.
        if open() {
            div {
                class: "menu-backdrop",
                onclick: move |_| open.set(false),
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    #[test]
    fn the_menu_carries_every_screen_the_owner_goes_to_occasionally() {
        let html = render(|| rsx! { AppMenu {} });

        assert!(html.contains("All trips"), "{html}");
        assert!(html.contains("Import a trip"), "{html}");
        assert!(html.contains("Sync with Komoot"), "{html}");
        // Each is a link to its screen, so the browser's own affordances —
        // middle-click, copy link — work as they do anywhere else.
        assert!(html.contains(r#"href="/import""#), "{html}");
        assert!(html.contains(r#"href="/komoot/sync""#), "{html}");
        // US-69: the links handed out, and stopping one.
        assert!(html.contains("Shares"), "{html}");
        assert!(html.contains(r#"href="/shares""#), "{html}");
        // The unfiltered list, spelled the way the router spells it: the
        // default filters are part of that URL (US-52), so the way home is
        // asked of the route rather than written out here.
        let home = Route::TripList {
            filters: Filters::default(),
        }
        .to_string();
        assert!(html.contains(&format!(r#"href="{home}""#)), "{html}");
    }

    #[test]
    fn the_menu_starts_closed_and_says_so() {
        // The burger is the only way to the panel on a narrow screen, so what
        // it announces to a screen reader is part of the story, not decoration.
        let html = render(|| rsx! { AppMenu {} });

        assert!(html.contains(r#"aria-expanded="false""#), "{html}");
        assert!(html.contains(r#"aria-controls="app-menu""#), "{html}");
        assert!(!html.contains("app-menu open"), "{html}");
        // Nothing to click away from while it is closed.
        assert!(!html.contains("menu-backdrop"), "{html}");
    }

    #[test]
    fn signing_out_is_offered_only_where_the_app_provides_it() {
        // Not on Android (US-16), where there is nothing a sign-out could
        // usefully do — and the menu is one item shorter rather than
        // offering it and failing.
        let html = render(|| rsx! { AppMenu {} });

        assert!(!html.contains("Sign out"), "{html}");

        let html = render(|| {
            rsx! {
                SignOutProvider { AppMenu {} }
            }
        });

        assert!(html.contains("Sign out"), "{html}");
        // The id the browser suite's sign-out test reaches for (US-19).
        assert!(html.contains(r#"id="sign-out""#), "{html}");
    }

    /// Stands in for `App`, which provides the action on the web.
    #[component]
    fn SignOutProvider(children: Element) -> Element {
        use_context_provider(|| SignOut(Callback::new(|_| {})));
        rsx! { {children} }
    }
}
