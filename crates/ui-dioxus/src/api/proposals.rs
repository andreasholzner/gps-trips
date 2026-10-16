//! Links the archive proposes from the tracks' geometry (US-103): every
//! proposal, how many, a selection confirmed or dismissed, and a check the
//! owner starts from a trip or a collection.

use serde::Serialize;
use trip_archive_types::{LinkPair, LinkPairs, ProposalCount, ProposedLink};

use super::{get_json, ok_or_error, ApiClient, ApiError};

/// What a check the owner starts looks for links for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CheckFrom {
    Trip(i64),
    Collection(i64),
}

/// `GET /api/link-proposals` — every proposal.
pub async fn list_proposals(archive: &ApiClient) -> Result<Vec<ProposedLink>, ApiError> {
    get_json(archive, archive.url("/api/link-proposals")).await
}

/// `GET /api/link-proposals/count` — how many.
pub async fn proposal_count(archive: &ApiClient) -> Result<i64, ApiError> {
    get_json::<ProposalCount>(archive, archive.url("/api/link-proposals/count"))
        .await
        .map(|counted| counted.count)
}

/// `POST /api/link-proposals/confirm` — make every pair a link, all or none.
pub async fn confirm_proposals(archive: &ApiClient, pairs: &[LinkPair]) -> Result<(), ApiError> {
    settle(archive, "confirm", pairs).await
}

/// `POST /api/link-proposals/dismiss` — drop every pair, all or none.
pub async fn dismiss_proposals(archive: &ApiClient, pairs: &[LinkPair]) -> Result<(), ApiError> {
    settle(archive, "dismiss", pairs).await
}

async fn settle(archive: &ApiClient, action: &str, pairs: &[LinkPair]) -> Result<(), ApiError> {
    let body = LinkPairs {
        pairs: pairs.to_vec(),
    };
    post(archive, &format!("/api/link-proposals/{action}"), &body).await?;
    Ok(())
}

/// `POST …/:id/link-check` — look for links, as if it had just entered the
/// archive. How many were proposed.
pub async fn look_for_links(archive: &ApiClient, from: CheckFrom) -> Result<i64, ApiError> {
    let path = match from {
        CheckFrom::Trip(id) => format!("/api/trips/{id}/link-check"),
        CheckFrom::Collection(id) => format!("/api/collections/{id}/link-check"),
    };
    let url = archive.url(&path);
    post(archive, &path, &serde_json::json!({}))
        .await?
        .json::<ProposalCount>()
        .await
        .map(|counted| counted.count)
        .map_err(|err| ApiError::new(format!("{url} returned unreadable JSON: {err}")))
}

async fn post(
    archive: &ApiClient,
    path: &str,
    body: &impl Serialize,
) -> Result<reqwest::Response, ApiError> {
    let url = archive.url(path);
    let response = archive
        .post(&url)
        .json(body)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;
    ok_or_error(archive, &url, response).await
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{import_sample, serve_test_archive};
    use trip_archive_types::LinkOther;

    #[tokio::test]
    async fn us103_proposals_are_listed_counted_and_settled() {
        let (archive, _dir) = serve_test_archive().await;
        let plan = import_sample(&archive, &[("kind", "planned")]).await;
        let monday = import_sample(&archive, &[]).await;
        let tuesday = import_sample(&archive, &[]).await;

        let listed = list_proposals(&archive).await.unwrap();

        assert_eq!(proposal_count(&archive).await.unwrap(), 2);
        let pairs: Vec<LinkPair> = listed.iter().map(ProposedLink::pair).collect();
        let to_plan = |trip_id| LinkPair {
            trip_id,
            other: LinkOther::Trip(plan),
        };
        assert!(pairs.contains(&to_plan(monday)) && pairs.contains(&to_plan(tuesday)));
        confirm_proposals(&archive, &[to_plan(monday)])
            .await
            .unwrap();
        dismiss_proposals(&archive, &[to_plan(tuesday)])
            .await
            .unwrap();
        assert_eq!(proposal_count(&archive).await.unwrap(), 0);
        let err = dismiss_proposals(&archive, &[to_plan(tuesday)]).await;
        assert!(err.is_err(), "no longer proposed");
    }

    #[tokio::test]
    async fn us103_a_check_from_a_trip_says_how_many_it_proposed() {
        let (archive, _dir) = serve_test_archive().await;
        let plan = import_sample(&archive, &[("kind", "planned")]).await;
        let monday = import_sample(&archive, &[]).await;
        let to_plan = LinkPair {
            trip_id: monday,
            other: LinkOther::Trip(plan),
        };
        dismiss_proposals(&archive, &[to_plan]).await.unwrap();

        assert_eq!(
            look_for_links(&archive, CheckFrom::Trip(monday))
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            look_for_links(&archive, CheckFrom::Trip(monday))
                .await
                .unwrap(),
            0
        );
        assert!(look_for_links(&archive, CheckFrom::Collection(99))
            .await
            .is_err());
    }
}
