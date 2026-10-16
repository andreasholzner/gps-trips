//! US-103 — the Proposed links page, against a real archive (ADR-0012).

use super::*;
use crate::test_support::{import_sample, render, render_against_archive, serve_test_archive};
use trip_archive_types::{ActivityType, LinkedTrip, Proposal, TripKind};

fn a_proposal(trip: i64, plan: i64) -> ProposedLink {
    let trip_named = |id, name: &str| LinkedTrip {
        id,
        name: name.to_string(),
        activity_type: ActivityType::Hiking,
        start_date: None,
        proposal: None,
    };
    ProposedLink {
        trip: trip_named(trip, "Monday"),
        trip_kind: TripKind::Recorded,
        other: ProposedEnd::PlannedTrip(trip_named(plan, "Plan")),
        proposal: Proposal {
            plan_covered: Some(0.92),
            trip_on_plan: Some(0.85),
            points_reached: None,
        },
    }
}

#[test]
fn us103_select_all_selects_every_listed_proposal_and_clears_them() {
    let listed = vec![a_proposal(1, 3), a_proposal(2, 3)];
    let pairs: BTreeSet<LinkPair> = listed.iter().map(ProposedLink::pair).collect();

    assert!(!all_selected(&listed, &BTreeSet::new()));
    assert!(all_selected(&listed, &pairs));
    assert!(!all_selected(&[], &BTreeSet::new()), "nothing to select");
}

#[test]
fn us103_an_entry_leads_to_its_trip_with_the_other_end_ticked() {
    let html = render(|| {
        let selected = use_signal(BTreeSet::new);
        rsx! { Entry { link: a_proposal(1, 3), selected } }
    });

    assert!(html.contains(r#"href="/trips/1/showing/trip-3""#), "{html}");
    assert!(html.contains("Monday"), "{html}");
    assert!(html.contains("Plan"), "{html}");
    assert!(
        html.contains("92 % of the plan · 85 % of the trip"),
        "{html}"
    );
    assert!(html.contains(r#"type="checkbox""#), "{html}");
}

#[tokio::test]
async fn us103_the_page_lists_every_proposal_to_confirm_or_dismiss_together() {
    let (archive, _dir) = serve_test_archive().await;
    let plan = import_sample(&archive, &[("kind", "planned"), ("name", "The plan")]).await;
    let monday = import_sample(&archive, &[("name", "Monday")]).await;
    import_sample(&archive, &[("name", "Tuesday")]).await;

    let html = render_against_archive(
        &archive,
        || rsx! { ProposedLinks {} },
        |html| html.contains("Tuesday"),
    )
    .await;

    assert!(html.contains("<h1>Proposed links</h1>"), "{html}");
    assert_eq!(html.matches("</span>The plan").count(), 2, "{html}");
    assert!(
        html.contains(&format!(r#"href="/trips/{monday}/showing/trip-{plan}""#)),
        "{html}"
    );
    assert!(html.contains(r#"aria-label="Select all""#), "{html}");
    assert!(html.contains(r#"id="confirm-proposals""#), "{html}");
    assert!(html.contains(r#"id="dismiss-proposals""#), "{html}");
}

#[tokio::test]
async fn us103_nothing_proposed_says_so() {
    let (archive, _dir) = serve_test_archive().await;

    let html = render_against_archive(
        &archive,
        || rsx! { ProposedLinks {} },
        |html| html.contains("Nothing"),
    )
    .await;

    assert!(html.contains("Nothing is proposed"), "{html}");
    assert!(!html.contains(r#"id="confirm-proposals""#), "{html}");
}
