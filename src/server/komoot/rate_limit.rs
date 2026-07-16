//! Rate limiting for authenticated Komoot API requests (US-23, ADR-0021): a
//! minimum spacing between consecutive requests, backing off further after a
//! `429`. Deliberately not applied to `fetch_photo_bytes` (`komoot.rs`) —
//! that hits a public, unauthenticated CloudFront URL, a different service
//! with no reason to share Komoot's own throttle. Split out of `komoot.rs`
//! to keep that file under the repo's 500-line cap (mirrors
//! `komoot_sync.rs` -> `komoot_sync/tests.rs`).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::config::komoot::{DEFAULT_RATE_LIMIT_BACKOFF, MIN_REQUEST_INTERVAL};

/// Tracks "next allowed request time" for `KomootHttpClient`'s authenticated
/// call sites (`send_authed`, `authed_patch`, `authed_delete`). One instance
/// per `KomootHttpClient`, so its budget is per-process (ADR-0021 accepts
/// that a concurrent `komoot_backfill` and "Sync now" wouldn't share one).
pub struct Throttle {
    next_allowed: Mutex<Instant>,
}

impl Throttle {
    pub fn new() -> Self {
        Self {
            next_allowed: Mutex::new(Instant::now()),
        }
    }

    /// Blocks (real sleep) until the minimum interval since the last
    /// request has elapsed, then reserves the next slot — call before
    /// sending an authenticated request.
    pub fn wait_before_request(&self) {
        let mut next = self.next_allowed.lock().unwrap();
        let (wait, new_next) = wait_and_advance(Instant::now(), *next, MIN_REQUEST_INTERVAL);
        *next = new_next;
        drop(next);
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }

    /// Call after a response comes back: on a `429`, extends the
    /// next-allowed time further (honoring `Retry-After` if Komoot sent
    /// one, else `DEFAULT_RATE_LIMIT_BACKOFF`). Any other status is a no-op
    /// — `wait_before_request` already reserved the right slot, and
    /// re-deriving it from the response's (necessarily later) arrival time
    /// would stretch every request by its own round-trip latency on top of
    /// `MIN_REQUEST_INTERVAL`.
    pub fn record_response(&self, status: reqwest::StatusCode, retry_after: Option<&str>) {
        if status != reqwest::StatusCode::TOO_MANY_REQUESTS {
            return;
        }
        let mut next = self.next_allowed.lock().unwrap();
        *next = backoff_after_response(Instant::now(), *next, retry_after);
    }

    #[cfg(test)]
    fn peek(&self) -> Instant {
        *self.next_allowed.lock().unwrap()
    }
}

/// Pure: how long to sleep now, and the new next-allowed instant to store.
/// Chains reservations off the *previous* next-allowed instant (not just
/// `now`), so back-to-back calls stay spaced by `min_interval` even when the
/// caller didn't actually sleep the full wait (e.g. in a test).
fn wait_and_advance(
    now: Instant,
    next_allowed: Instant,
    min_interval: Duration,
) -> (Duration, Instant) {
    let wait = next_allowed.saturating_duration_since(now);
    (wait, now.max(next_allowed) + min_interval)
}

/// Pure: the new next-allowed instant after a `429` — only called for that
/// status (see `record_response`). Backs off using `Retry-After` if
/// present, else `DEFAULT_RATE_LIMIT_BACKOFF`, and never shortens an
/// already-longer reservation.
fn backoff_after_response(
    now: Instant,
    current_next_allowed: Instant,
    retry_after: Option<&str>,
) -> Instant {
    let advance = parse_retry_after(retry_after).unwrap_or(DEFAULT_RATE_LIMIT_BACKOFF);
    current_next_allowed.max(now + advance)
}

/// Parses a `Retry-After` header value as whole seconds (Komoot has been
/// observed sending only integers; the HTTP spec also allows an HTTP-date,
/// which this app doesn't need to support for a personal, single-user
/// client). `None` if absent or unparseable, in which case
/// `DEFAULT_RATE_LIMIT_BACKOFF` is used instead.
fn parse_retry_after(value: Option<&str>) -> Option<Duration> {
    value
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = MIN_REQUEST_INTERVAL;

    #[test]
    fn wait_and_advance_does_not_wait_when_next_allowed_is_in_the_past() {
        let now = Instant::now();
        let next_allowed = now - Duration::from_secs(1);

        let (wait, new_next) = wait_and_advance(now, next_allowed, MIN);

        assert!(wait.is_zero());
        assert_eq!(new_next, now + MIN);
    }

    #[test]
    fn wait_and_advance_waits_out_a_reserved_future_slot() {
        let now = Instant::now();
        let next_allowed = now + Duration::from_millis(200);

        let (wait, new_next) = wait_and_advance(now, next_allowed, MIN);

        assert_eq!(wait, Duration::from_millis(200));
        // Chains off next_allowed, not `now` — a caller that fires many
        // requests in a tight loop stays spaced by exactly MIN apart
        // instead of drifting shorter.
        assert_eq!(new_next, next_allowed + MIN);
    }

    #[test]
    fn backoff_after_response_honors_retry_after_header() {
        let now = Instant::now();
        let current_next_allowed = now;

        let new_next = backoff_after_response(now, current_next_allowed, Some("30"));

        assert_eq!(new_next, now + Duration::from_secs(30));
    }

    #[test]
    fn backoff_after_response_without_retry_after_uses_the_default() {
        let now = Instant::now();

        let new_next = backoff_after_response(now, now, None);

        assert_eq!(new_next, now + DEFAULT_RATE_LIMIT_BACKOFF);
    }

    #[test]
    fn backoff_after_response_with_an_unparseable_retry_after_uses_the_default() {
        let now = Instant::now();

        let new_next = backoff_after_response(now, now, Some("not-a-number"));

        assert_eq!(new_next, now + DEFAULT_RATE_LIMIT_BACKOFF);
    }

    #[test]
    fn backoff_after_response_never_shortens_an_existing_longer_reservation() {
        // A second 429 (or an already-longer-than-default backoff in
        // flight) must not shrink the next-allowed time back down.
        let now = Instant::now();
        let current_next_allowed = now + Duration::from_secs(60);

        let new_next = backoff_after_response(now, current_next_allowed, Some("5"));

        assert_eq!(new_next, current_next_allowed);
    }

    // ── Throttle::record_response: the success path must not stretch the
    // reservation past what wait_before_request already set ─────────────

    #[test]
    fn record_response_on_a_non_429_status_leaves_the_reservation_untouched() {
        let throttle = Throttle::new();
        throttle.wait_before_request();
        let reserved = throttle.peek();

        // Simulate time passing during the request's own round trip before
        // the response comes back — the scenario that previously caused
        // record_response to re-derive (and stretch) the reservation from
        // the later "now" at response time.
        std::thread::sleep(Duration::from_millis(50));
        throttle.record_response(reqwest::StatusCode::OK, None);

        assert_eq!(
            throttle.peek(),
            reserved,
            "a successful response must not push the reservation further out \
             than wait_before_request already reserved"
        );
    }

    #[test]
    fn record_response_on_429_extends_the_reservation() {
        let throttle = Throttle::new();
        throttle.wait_before_request();

        throttle.record_response(reqwest::StatusCode::TOO_MANY_REQUESTS, Some("30"));
        let after_429 = Instant::now();

        assert!(
            throttle.peek() >= after_429 + Duration::from_secs(29),
            "a 429 must extend the reservation by roughly Retry-After"
        );
    }

    #[test]
    fn parse_retry_after_parses_whole_seconds() {
        assert_eq!(parse_retry_after(Some("7")), Some(Duration::from_secs(7)));
    }

    #[test]
    fn parse_retry_after_is_none_for_missing_or_unparseable_values() {
        assert_eq!(parse_retry_after(None), None);
        assert_eq!(parse_retry_after(Some("soon")), None);
        assert_eq!(parse_retry_after(Some("")), None);
    }
}
