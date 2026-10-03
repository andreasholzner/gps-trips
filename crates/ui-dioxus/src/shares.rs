//! The owner's shares (US-69): every link that still opens something, and
//! stopping one.

use dioxus::prelude::*;
use trip_archive_types::ActiveShare;

use crate::api::{self, ApiClient};
use crate::format;
use crate::share::ShareLink;

/// What a share without a label is called here. The recipient sees
/// "Shared trips" for it; the owner is better served by being told there is
/// no title.
const UNTITLED: &str = "Untitled share";

/// What the screen says when no link opens anything.
const NONE_ACTIVE: &str = "No active shares. Share trips from the trip list's selection or \
    from a trip's page, and a summary from the Summary screen.";

/// `/shares` — the screen.
#[component]
pub fn Shares() -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let shares = use_resource(move || async move { api::list_shares(&archive()).await });

    rsx! {
        h1 { "Shares" }
        match &*shares.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load the shares: {err}" } },
            Some(Ok(shares)) => rsx! { ShareList { shares: shares.clone() } },
        }
    }
}

/// The active shares, newest first. A stopped one leaves the list at once —
/// it is gone on the archive too, so there is nothing to fetch again.
#[component]
fn ShareList(shares: Vec<ActiveShare>) -> Element {
    let mut listed = use_signal(|| shares.clone());

    if listed.read().is_empty() {
        return rsx! { p { "{NONE_ACTIVE}" } };
    }
    rsx! {
        for share in listed() {
            ShareRow {
                key: "{share.id}",
                share,
                on_stopped: move |id| listed.write().retain(|share| share.id != id),
            }
        }
    }
}

/// One share: what it is called, the trips or the tags' summary it reaches
/// (US-82), when it was made, its link, and stopping it.
#[component]
fn ShareRow(share: ActiveShare, on_stopped: EventHandler<i64>) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut arming = use_signal(|| false);
    let mut stopping = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let id = share.id;
    let title = share.label.as_deref().unwrap_or(UNTITLED);
    let reaches = if share.tags.is_empty() {
        share.trip_names.join(", ")
    } else {
        format!("Summary of {}", share.tags.join(", "))
    };
    let created = format::date(Some(&share.created_at));
    let url = api::share_link(&archive(), &share.token);
    let used = usage(&share);

    rsx! {
        article { class: "share", id: "share-{id}",
            h2 { "{title}" }
            p { "{reaches}" }
            p { class: "muted", "Created {created}" }
            p { class: "share-usage", "{used}" }
            ShareLink { url, expires_at: share.expires_at.clone() }
            if arming() {
                ConfirmStop {
                    busy: stopping(),
                    on_confirm: move |_| async move {
                        // One stop per confirmation: a second would answer 404
                        // for the share the first one stopped.
                        if stopping() {
                            return;
                        }
                        stopping.set(true);
                        match api::stop_share(&archive(), id).await {
                            Ok(()) => on_stopped.call(id),
                            Err(err) => {
                                arming.set(false);
                                error.set(Some(err.to_string()));
                            }
                        }
                        stopping.set(false);
                    },
                    on_cancel: move |_| arming.set(false),
                }
            } else {
                button {
                    r#type: "button",
                    class: "quiet danger stop-share",
                    onclick: move |_| {
                        error.set(None);
                        arming.set(true);
                    },
                    "Stop sharing"
                }
            }
            if let Some(message) = error() {
                p { class: "error", "Could not stop this share: {message}" }
            }
        }
    }
}

/// How the link was used (US-70): how often it was opened, when last, and
/// on what.
fn usage(share: &ActiveShare) -> String {
    if share.opens == 0 {
        return "Not opened yet".to_string();
    }
    let times = if share.opens == 1 {
        "once".to_string()
    } else {
        format!("{} times", share.opens)
    };
    let last = format::date(share.last_opened_at.as_deref());
    // Two user agents can be the same device and browser in different
    // versions; the owner is told each once.
    let mut devices: Vec<String> = Vec::new();
    for device in share.user_agents.iter().map(|agent| format::device(agent)) {
        if !devices.contains(&device) {
            devices.push(device);
        }
    }
    if devices.is_empty() {
        format!("Opened {times}, last on {last}")
    } else {
        format!("Opened {times}, last on {last} · {}", devices.join(", "))
    }
}

/// The confirmation, in the page like deleting a trip's (US-9).
#[component]
fn ConfirmStop(
    #[props(default)] busy: bool,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    rsx! {
        p { class: "confirm",
            "Stop this share? Its link stops working for good; this cannot be undone. "
            button {
                r#type: "button",
                class: "danger",
                disabled: busy,
                onclick: move |_| on_confirm.call(()),
                "Stop it"
            }
            button { r#type: "button", onclick: move |_| on_cancel.call(()), "Cancel" }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::create_share;
    use crate::test_support::{import_sample, render, render_against_archive, serve_test_archive};
    use trip_archive_types::{CreateShare, ShareExpiry};

    fn a_share(id: i64, label: Option<&str>, expires_at: Option<&str>) -> ActiveShare {
        ActiveShare {
            id,
            token: format!("token{id}"),
            label: label.map(str::to_string),
            trip_names: vec!["Day one".to_string(), "Day two".to_string()],
            tags: Vec::new(),
            created_at: "2026-09-25T12:00:00Z".to_string(),
            expires_at: expires_at.map(str::to_string),
            opens: 0,
            last_opened_at: None,
            user_agents: Vec::new(),
        }
    }

    #[test]
    fn us70_a_share_says_how_often_its_link_was_opened_and_on_what() {
        let html = render(|| {
            let share = ActiveShare {
                opens: 3,
                last_opened_at: Some("2026-09-26T08:15:00Z".to_string()),
                user_agents: vec![
                    "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) \
                     AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1"
                        .to_string(),
                ],
                ..a_share(7, None, None)
            };
            rsx! { ShareRow { share, on_stopped: move |_| {} } }
        });

        assert!(
            html.contains("Opened 3 times, last on 2026-09-26"),
            "{html}"
        );
        assert!(html.contains("iPhone · Safari"), "{html}");
    }

    #[test]
    fn us70_a_share_nobody_opened_says_so() {
        let html = render(|| {
            rsx! { ShareRow { share: a_share(7, None, None), on_stopped: move |_| {} } }
        });

        assert!(html.contains("Not opened yet"), "{html}");
    }

    #[test]
    fn us69_a_share_shows_its_trips_dates_and_link() {
        let html = render(|| {
            rsx! {
                ShareRow {
                    share: a_share(7, Some("For Kari"), Some("2026-10-25T12:00:00Z")),
                    on_stopped: move |_| {},
                }
            }
        });

        assert!(html.contains("For Kari"), "{html}");
        assert!(html.contains("Day one"), "{html}");
        assert!(html.contains("Day two"), "{html}");
        assert!(html.contains("Created 2026-09-25"), "{html}");
        assert!(html.contains("works until 2026-10-25"), "{html}");
        assert!(html.contains("/app/s/token7"), "{html}");
        assert!(html.contains("Copy"), "{html}");
    }

    #[test]
    fn us82_a_share_of_tags_shows_its_tags_in_place_of_trips() {
        let html = render(|| {
            let share = ActiveShare {
                trip_names: Vec::new(),
                tags: vec!["alps".to_string(), "norway".to_string()],
                ..a_share(7, Some("Summer"), None)
            };
            rsx! { ShareRow { share, on_stopped: move |_| {} } }
        });

        assert!(html.contains("Summary of alps, norway"), "{html}");
        assert!(!html.contains("Day one"), "{html}");
        assert!(html.contains("Stop sharing"), "{html}");
    }

    #[test]
    fn us69_a_share_without_a_label_or_expiry_says_so() {
        let html = render(|| {
            rsx! { ShareRow { share: a_share(7, None, None), on_stopped: move |_| {} } }
        });

        assert!(html.contains(UNTITLED), "{html}");
        assert!(html.contains("until you stop it"), "{html}");
    }

    #[test]
    fn us69_stopping_is_offered_but_not_armed() {
        let html = render(|| {
            rsx! { ShareRow { share: a_share(7, None, None), on_stopped: move |_| {} } }
        });

        assert!(html.contains("Stop sharing"), "{html}");
        assert!(!html.contains("cannot be undone"), "{html}");
    }

    #[test]
    fn us69_stopping_is_confirmed_first_and_says_it_is_for_good() {
        let html = render(|| {
            rsx! { ConfirmStop { on_confirm: move |_| {}, on_cancel: move |_| {} } }
        });

        assert!(html.contains("stops working"), "{html}");
        assert!(html.contains("cannot be undone"), "{html}");
        assert!(html.contains("Stop it"), "{html}");
        assert!(html.contains("Cancel"), "{html}");
    }

    #[test]
    fn us69_no_active_shares_says_so() {
        let html = render(|| rsx! { ShareList { shares: Vec::new() } });
        assert!(html.contains("No active shares"), "{html}");
    }

    #[tokio::test]
    async fn us69_the_screen_lists_the_archives_active_shares() {
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
        create_share(
            &archive,
            &CreateShare {
                trip_ids: vec![id],
                tags: Vec::new(),
                label: Some("For Kari".to_string()),
                expiry: ShareExpiry::Never,
            },
        )
        .await
        .expect("share");

        let html = render_against_archive(
            &archive,
            || rsx! { Shares {} },
            |html| html.contains("For Kari"),
        )
        .await;

        assert!(html.contains("Oslo Hills Walk"), "{html}");
        assert!(!html.contains("No active shares"), "{html}");
    }
}
