//! Sharing trips (US-53), the owner's side: a label, an expiry, and the link
//! that comes back. The same form serves the trip list's selection, a trip's
//! own page, the Summary screen's tags (US-82), and collections and their
//! tracks (US-94), in an overlay on each: sharing is occasional, and its
//! options would otherwise sit open beside the controls used every day.

use dioxus::prelude::*;
use trip_archive_types::{CreateShare, ShareExpiry};

use crate::api::{self, ApiClient};
use crate::format;
use crate::interop;
use crate::overlay::Overlay;

/// What a share is made for: a few trips, the summary of a few tags in
/// the order chosen (US-82), or collections or collection tracks (US-94).
#[derive(Clone, Debug, PartialEq)]
pub enum ShareTarget {
    Trips(Vec<i64>),
    Tags(Vec<String>),
    Collections(Vec<i64>),
    CollectionTracks(Vec<i64>),
}

/// What a selection holds, for sharing it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Selected {
    #[default]
    Trips,
    Collections,
    CollectionTracks,
}

impl Selected {
    pub fn target(self, ids: Vec<i64>) -> ShareTarget {
        match self {
            Self::Trips => ShareTarget::Trips(ids),
            Self::Collections => ShareTarget::Collections(ids),
            Self::CollectionTracks => ShareTarget::CollectionTracks(ids),
        }
    }

    fn legend(self) -> &'static str {
        match self {
            Self::Trips => "Share selected trips",
            Self::Collections => "Share selected collections",
            Self::CollectionTracks => "Share selected tracks",
        }
    }
}

/// Sharing what is selected — trips on the list screen, beside tagging them
/// and setting their activity, or collections or a collection's tracks
/// (US-94). Appears only once something is selected, as one button that
/// opens the options.
#[component]
pub fn ShareSelectedPanel(ids: Vec<i64>, #[props(default)] of: Selected) -> Element {
    let mut open = use_signal(|| false);
    if ids.is_empty() {
        return rsx! {};
    }
    let count = ids.len();
    rsx! {
        fieldset {
            legend { {of.legend()} }
            div { class: "tag-entry",
                button {
                    id: "share-selected",
                    r#type: "button",
                    onclick: move |_| open.set(true),
                    "Share {count} selected…"
                }
            }
        }
        if open() {
            ShareDialog { target: of.target(ids), on_close: move |_| open.set(false) }
        }
    }
}

/// A page's own "Share" button — a collection's or a collection track's
/// (US-94) — opening the options for `target`.
#[component]
pub fn ShareButton(id: String, target: ShareTarget) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        button {
            id,
            r#type: "button",
            class: "quiet",
            onclick: move |_| open.set(true),
            "Share"
        }
        if open() {
            ShareDialog { target, on_close: move |_| open.set(false) }
        }
    }
}

/// The share options over the screen, for `target`.
#[component]
pub fn ShareDialog(target: ShareTarget, on_close: EventHandler<()>) -> Element {
    let label = match target {
        ShareTarget::Trips(_) => "Share trips",
        ShareTarget::Tags(_) => "Share the summary",
        ShareTarget::Collections(_) => "Share collections",
        ShareTarget::CollectionTracks(_) => "Share tracks",
    };
    rsx! {
        Overlay { label, on_close: move |_| on_close.call(()),
            div { class: "panel",
                h2 { "Share" }
                ShareForm { target }
                div { class: "form-actions",
                    button {
                        id: "close-share",
                        r#type: "button",
                        class: "quiet",
                        onclick: move |_| on_close.call(()),
                        "Close"
                    }
                }
            }
        }
    }
}

/// The form: who the share is for, when it ends, and — once made — the link
/// to send. A change of trips or tags starts over, so a link made for one
/// selection is never shown as if it were the next one's.
#[component]
pub fn ShareForm(target: ShareTarget) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut label = use_signal(String::new);
    let mut expiry = use_signal(ShareExpiry::default);
    let mut link = use_signal(|| None::<(String, Option<String>)>);
    let mut message = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    use_effect(use_reactive!(|target| {
        let _ = target;
        link.set(None);
        message.set(None);
    }));

    let button = create_label(&target);
    let shows = shown(&target);
    let create = move |_| {
        let request = request(&target, &label(), expiry());
        busy.set(true);
        spawn(async move {
            match api::create_share(&archive(), &request).await {
                Ok(created) => {
                    link.set(Some((
                        api::share_link(&archive(), &created.token),
                        created.expires_at,
                    )));
                    message.set(None);
                }
                Err(err) => message.set(Some(err.to_string())),
            }
            busy.set(false);
        });
    };

    rsx! {
        div { class: "share-form",
            label {
                "Title the recipient sees (optional)"
                input {
                    r#type: "text",
                    name: "share-label",
                    maxlength: "100",
                    value: "{label}",
                    oninput: move |event| label.set(event.value()),
                }
            }
            label {
                "Link stops working"
                select {
                    name: "share-expiry",
                    value: expiry().as_str(),
                    onchange: move |event| {
                        if let Some(chosen) = ShareExpiry::parse(&event.value()) {
                            expiry.set(chosen);
                        }
                    },
                    for choice in ShareExpiry::ALL {
                        option { key: "{choice.as_str()}", value: choice.as_str(), "{choice.label()}" }
                    }
                }
            }
            button {
                id: "create-share",
                r#type: "button",
                disabled: busy(),
                onclick: create,
                "{button}"
            }
            if let Some((url, expires_at)) = link() {
                ShareLink { url, expires_at, id: "share-link".to_string(), shows }
            }
            if let Some(message) = message() {
                p { class: "error", "{message}" }
            }
        }
    }
}

/// The request that shares `target` under `label`, which blank leaves out.
pub fn request(target: &ShareTarget, label: &str, expiry: ShareExpiry) -> CreateShare {
    let named = CreateShare {
        label: Some(label.to_string()).filter(|label| !label.trim().is_empty()),
        expiry,
        ..CreateShare::default()
    };
    match target {
        ShareTarget::Trips(ids) => CreateShare {
            trip_ids: ids.clone(),
            ..named
        },
        ShareTarget::Tags(tags) => CreateShare {
            tags: tags.clone(),
            ..named
        },
        ShareTarget::Collections(ids) => CreateShare {
            collection_ids: ids.clone(),
            ..named
        },
        ShareTarget::CollectionTracks(ids) => CreateShare {
            collection_track_ids: ids.clone(),
            ..named
        },
    }
}

/// What the create button says.
pub fn create_label(target: &ShareTarget) -> String {
    let some = |ids: &[i64], one: &str, many: &str| match ids.len() {
        1 => format!("Make a link to this {one}"),
        n => format!("Make a link to {n} {many}"),
    };
    match target {
        ShareTarget::Trips(ids) => some(ids, "trip", "trips"),
        ShareTarget::Tags(_) => "Make a link to this summary".to_string(),
        ShareTarget::Collections(ids) => some(ids, "collection", "collections"),
        ShareTarget::CollectionTracks(ids) => some(ids, "track", "tracks"),
    }
}

/// What a link to trips or a summary shows.
pub const TRIPS_SHOWN: &str = "these trips, their photos and GPX";
/// What a link to collections shows (US-94).
pub const COLLECTIONS_SHOWN: &str = "these collections, their tracks, waypoints and GPX";
/// What a link to collection tracks shows (US-94).
pub const TRACKS_SHOWN: &str = "these tracks, their waypoints and GPX";

/// What a link to `target` lets its holder see.
pub fn shown(target: &ShareTarget) -> &'static str {
    match target {
        ShareTarget::Trips(_) | ShareTarget::Tags(_) => TRIPS_SHOWN,
        ShareTarget::Collections(_) => COLLECTIONS_SHOWN,
        ShareTarget::CollectionTracks(_) => TRACKS_SHOWN,
    }
}

/// A share's link, ready to copy, what it shows and until when it works —
/// the one just made, or one on the owner's list of shares (US-69), where
/// several sit on one page and so none takes the element id.
#[component]
pub fn ShareLink(
    url: String,
    expires_at: Option<String>,
    id: Option<String>,
    #[props(default = TRIPS_SHOWN)] shows: &'static str,
) -> Element {
    let until = match expires_at.as_deref() {
        Some(at) => format!("It works until {}.", format::date(Some(at))),
        None => "It works until you stop it.".to_string(),
    };
    rsx! {
        div { class: "share-link",
            input {
                id,
                r#type: "text",
                readonly: true,
                "aria-label": "Share link",
                value: "{url}",
            }
            button {
                r#type: "button",
                onclick: {
                    let url = url.clone();
                    move |_| {
                        let url = url.clone();
                        spawn(async move { interop::copy_to_clipboard(url).await });
                    }
                },
                "Copy"
            }
            p { "Anyone with this link can see {shows}. {until}" }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
