//! Where the API lives, and how the app remembers it.
//!
//! On the web the SPA is served by the very server it talks to, so an empty
//! base URL — plain relative `/api/…` — is right and there is nothing to
//! configure. In an Android webview the page is served from the app's own
//! internal origin, so relative URLs resolve to nothing: the base URL becomes
//! something the owner must supply, and something the app must remember.
//!
//! Stored in `localStorage`, reached through `eval`. That works on both
//! platforms for the same reason the map does: the Android app *is* a
//! webview, so browser storage is simply there, with no extra dependency and
//! no platform-specific code path.

use dioxus::prelude::*;

use crate::api;

const STORAGE_KEY: &str = "trip-archive.base_url";

/// The base URL to use: whatever was stored, or the sensible default for the
/// platform when nothing was.
///
/// On the web the default is the page's own origin. `reqwest` — unlike a
/// browser `fetch` wrapper — parses every URL into an absolute one, so a
/// relative `/api/trips` fails outright with a builder error rather than
/// resolving against the current page. Sharing one HTTP client across
/// platforms costs this small explicitness.
pub async fn load() -> String {
    let stored = read(STORAGE_KEY).await;
    if !stored.trim().is_empty() {
        return stored;
    }
    if cfg!(target_arch = "wasm32") {
        let mut eval = document::eval("dioxus.send(window.location.origin);");
        return eval.recv::<String>().await.unwrap_or_default();
    }
    String::new()
}

async fn read(key: &str) -> String {
    let mut eval = document::eval(&format!(
        r#"dioxus.send(window.localStorage.getItem("{key}") ?? "");"#
    ));
    eval.recv::<String>().await.unwrap_or_default()
}

/// Persist a base URL for the next launch.
pub fn save(base_url: &str) {
    let eval = document::eval(&format!(
        r#"
        const value = await dioxus.recv();
        window.localStorage.setItem("{STORAGE_KEY}", value);
        "#
    ));
    if let Err(err) = eval.send(base_url.to_string()) {
        dioxus::logger::tracing::error!("failed to store the base URL: {err}");
    }
}

/// Whether the app can't work until the owner supplies a base URL. Only ever
/// true off the web, where relative URLs have nothing to be relative to.
pub fn needs_configuration(base_url: &str) -> bool {
    !cfg!(target_arch = "wasm32") && base_url.trim().is_empty()
}

/// Tidy up a hand-typed address: trim it, assume `http://` when no scheme is
/// given (a LAN address is typed far more often than it is pasted), and drop
/// a trailing slash so joining `/api/…` can't produce a doubled one.
pub fn normalize(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    with_scheme.trim_end_matches('/').to_string()
}

/// The settings screen: type where the archive lives, check it, save it.
///
/// Deliberately verifies before saving — a mistyped address that is only
/// discovered as "could not load trips" three screens later is exactly the
/// kind of thing that makes a phone app feel broken.
#[component]
pub fn Settings() -> Element {
    let mut base_url = use_context::<Signal<String>>();
    let mut draft = use_signal(&*base_url);
    let mut status = use_signal(String::new);

    let check_and_save = move |_| async move {
        let candidate = normalize(&draft());
        if candidate.is_empty() {
            status.set("Enter the address your Trip Archive is reachable at.".to_string());
            return;
        }
        status.set(format!("Checking {candidate}…"));
        match api::list_trips(&candidate, "?kind=recorded".to_string()).await {
            Ok(trips) => {
                save(&candidate);
                base_url.set(candidate);
                status.set(format!("Connected — {} recorded trip(s).", trips.len()));
            }
            Err(err) => status.set(format!("No archive answered there: {err}")),
        }
    };

    rsx! {
        h1 { "Where is your archive?" }
        p { "The address of the Trip Archive server, e.g. http://192.168.1.20:3000" }
        input {
            r#type: "url",
            style: "width: 100%; max-width: 30rem; padding: 0.5rem;",
            placeholder: "192.168.1.20:3000",
            value: "{draft}",
            oninput: move |event| draft.set(event.value()),
        }
        p {
            button { onclick: check_and_save, "Connect" }
        }
        if !status().is_empty() {
            p { "{status}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_host_and_port_is_assumed_to_be_http() {
        assert_eq!(normalize("192.168.1.20:3000"), "http://192.168.1.20:3000");
    }

    #[test]
    fn an_explicit_scheme_is_kept() {
        assert_eq!(
            normalize("https://trips.example.com"),
            "https://trips.example.com"
        );
    }

    #[test]
    fn surrounding_space_and_a_trailing_slash_are_dropped() {
        // Both are what a phone keyboard's autocomplete tends to produce.
        assert_eq!(
            normalize("  http://10.0.0.5:3000/  "),
            "http://10.0.0.5:3000"
        );
    }

    #[test]
    fn nothing_typed_stays_nothing() {
        assert_eq!(normalize("   "), "");
    }

    #[test]
    fn the_web_build_never_demands_configuration() {
        // Served by the same origin it queries, so "" is a working base URL
        // there and a broken one everywhere else.
        assert_eq!(needs_configuration(""), !cfg!(target_arch = "wasm32"));
        assert!(!needs_configuration("http://10.0.0.5:3000"));
    }
}
