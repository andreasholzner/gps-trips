//! US-19 — the archive's own login screen.
//!
//! A screen rather than the browser's credential dialog, which is what
//! [ADR-0010]'s 2026-09-02 amendment gave up basic auth for: a dialog has no
//! wording of its own, no way out, and no existence at all in the Android
//! webview (US-16).
//!
//! It shows what the archive said rather than a status code, because "that
//! is not the password" and "too many attempts, wait a quarter of an hour"
//! are different things to be told and the archive already words both.
//!
//! [ADR-0010]: ../../../docs/adr/0010-single-user-optional-auth.md

use dioxus::prelude::*;

use crate::api::{self, ApiClient};

/// The sign-in screen.
///
/// Takes the archive as a prop rather than from context: this is the one
/// screen that runs *before* there is a session, and it hands the token it
/// obtains back to the app, which is what puts it in the shared client.
/// (Only a client with no cookie store — Android, and the host-target tests
/// — actually needs the token; in a browser the cookie the archive set is
/// already doing the work.)
///
/// The URL is untouched by any of this, so signing in lands on the screen
/// the address bar already named: US-19's "returns to the screen asked for"
/// is a property of not navigating, not a redirect to get right.
#[component]
pub fn Login(
    archive: ApiClient,
    on_signed_in: EventHandler<String>,
    /// Something to say above the form before anything has been typed — the
    /// session probe failing for a reason other than "not signed in", which
    /// would otherwise leave an unreachable archive looking like a password
    /// prompt.
    notice: Option<String>,
) -> Element {
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| notice);
    let mut signing_in = use_signal(|| false);

    rsx! {
        section { class: "login",
            h1 { "Trip Archive" }
            form {
                id: "login-form",
                onsubmit: move |event| {
                    let archive = archive.clone();
                    async move {
                        event.prevent_default();
                        signing_in.set(true);
                        match api::login(&archive, &password()).await {
                            Ok(session) => {
                                password.set(String::new());
                                on_signed_in.call(session.token);
                            }
                            Err(err) => {
                                error.set(Some(err.to_string()));
                                signing_in.set(false);
                            }
                        }
                    }
                },
                label {
                    "Password "
                    input {
                        id: "login-password",
                        r#type: "password",
                        // The browser's own password manager is the only
                        // place this secret is meant to be kept.
                        autocomplete: "current-password",
                        value: "{password}",
                        oninput: move |event| password.set(event.value()),
                    }
                }
                button {
                    r#type: "submit",
                    id: "login-submit",
                    // A second click while the first is in flight would only
                    // spend one of the five attempts before the lockout.
                    disabled: signing_in(),
                    if signing_in() { "Signing in…" } else { "Sign in" }
                }
            }
            if let Some(message) = error() {
                p { class: "error", id: "login-error", "{message}" }
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
//
// Rendering only: `dioxus-ssr` dispatches no events, so typing a password and
// submitting the form belong to the browser layer (ADR-0012's 2026-08-26b
// amendment), where `tests/browser/login.spec.mjs` covers them.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    fn a_login_screen(notice: Option<String>) -> String {
        render(move || {
            rsx! {
                Login {
                    archive: ApiClient::new("http://archive.test"),
                    on_signed_in: move |_| {},
                    notice: notice.clone(),
                }
            }
        })
    }

    #[test]
    fn us19_the_archive_asks_for_the_password_in_its_own_screen() {
        let html = a_login_screen(None);
        assert!(
            html.contains(r#"type="password""#),
            "the password field is the screen; got {html}"
        );
        assert!(html.contains("login-submit"), "got {html}");
    }

    #[test]
    fn us19_the_login_screen_shows_nothing_of_the_archive() {
        // The point of the gate: a signed-out visitor sees a password field
        // and the archive's name, and nothing about any trip.
        let html = a_login_screen(None);
        assert!(!html.contains("Trips"), "got {html}");
        assert!(!html.contains("Import"), "got {html}");
    }

    #[test]
    fn us19_an_archive_that_could_not_be_asked_says_so_before_anything_is_typed() {
        let html = a_login_screen(Some("http://archive.test unreachable".to_string()));
        assert!(html.contains("unreachable"), "got {html}");
    }

    #[test]
    fn us19_nothing_is_being_signed_in_before_the_owner_asks() {
        let html = a_login_screen(None);
        assert!(!html.contains("Signing in"), "got {html}");
        assert!(!html.contains("disabled"), "got {html}");
    }
}
