//! Links the archive proposes (US-103): what a proposal is read by, the
//! signal that tells whatever lists or counts proposals to read them again,
//! and a check the owner starts.

use dioxus::prelude::*;
use trip_archive_types::Proposal;

use crate::api::{self, ApiClient, CheckFrom};

/// Bumped whenever proposals may have come or gone — confirmed, dismissed,
/// or a check started — so the navigation's count is read again. Provided
/// by the app; absent, nothing is told.
#[derive(Clone, Copy)]
pub struct ProposalsChanged(pub Signal<u64>);

impl ProposalsChanged {
    /// Tell whatever counts proposals that they may have changed.
    pub fn bump(self) {
        let ProposalsChanged(mut changed) = self;
        *changed.write() += 1;
    }
}

/// The app's signal, if it provides one — a hook, so read in a component's
/// body and bumped from its handlers.
pub fn use_proposals_changed() -> Option<ProposalsChanged> {
    try_use_context::<ProposalsChanged>()
}

/// What a proposal was proposed on, as the owner reads it: both shares of a
/// plan followed along its line, the share of its points of one judged by
/// them, nothing for a waypoint passed.
pub fn coverage(proposal: &Proposal) -> Option<String> {
    let percent = |share: f64| (share * 100.0).round() as i64;
    match proposal {
        Proposal {
            plan_covered: Some(plan),
            trip_on_plan: Some(trip),
            ..
        } => Some(format!(
            "{} % of the plan · {} % of the trip",
            percent(*plan),
            percent(*trip)
        )),
        Proposal {
            points_reached: Some(points),
            ..
        } => Some(format!(
            "{} % of the plan's points reached",
            percent(*points)
        )),
        _ => None,
    }
}

/// What a check the owner started found.
pub fn found(count: i64) -> String {
    match count {
        0 => "Nothing new to propose".to_string(),
        1 => "Proposed 1 link".to_string(),
        n => format!("Proposed {n} links"),
    }
}

/// Looking for links for a trip or a collection, as if it had just entered
/// the archive — what the owner starts when a plan, a stage or a recording
/// was there before the other. What it proposed shows where proposals are
/// listed, which the signal tells.
#[component]
pub fn LookForLinks(from: CheckFrom) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let changed = use_proposals_changed();
    let mut looking = use_signal(|| false);
    let mut told = use_signal(|| None::<Result<String, String>>);
    // The router shows the next one through this same scope; what was found
    // for one is not news about the next.
    use_effect(use_reactive!(|from| {
        let _ = from;
        told.set(None);
    }));

    rsx! {
        button {
            id: "look-for-links",
            r#type: "button",
            class: "quiet",
            disabled: looking(),
            onclick: move |_| async move {
                looking.set(true);
                let result = api::look_for_links(&archive(), from).await;
                looking.set(false);
                told.set(Some(result.map(found).map_err(|err| err.to_string())));
                if let Some(changed) = changed {
                    changed.bump();
                }
            },
            if looking() { "Looking for links…" } else { "Look for links" }
        }
        match told() {
            Some(Ok(message)) => rsx! { span { class: "muted", role: "status", "{message}" } },
            Some(Err(err)) => rsx! { span { class: "error", role: "status", "Could not look for links: {err}" } },
            None => rsx! {},
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    #[test]
    fn us103_a_check_tells_what_it_found() {
        assert_eq!(found(0), "Nothing new to propose");
        assert_eq!(found(1), "Proposed 1 link");
        assert_eq!(found(3), "Proposed 3 links");
    }

    #[test]
    fn us103_a_check_is_started_from_a_button() {
        let html = render(|| rsx! { LookForLinks { from: CheckFrom::Trip(1) } });

        assert!(html.contains(r#"id="look-for-links""#), "{html}");
        assert!(html.contains("Look for links"), "{html}");
    }
}
