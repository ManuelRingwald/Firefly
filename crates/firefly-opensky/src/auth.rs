//! OAuth2 client-credentials token management for the OpenSky Network adapter
//! (ADR 0024).
//!
//! OpenSky switched off HTTP Basic auth and now accepts **only** the OAuth2
//! client-credentials flow: a `client_id`/`client_secret` pair is exchanged at a
//! Keycloak token endpoint for a short-lived (~30 min) bearer access token, which
//! is then sent on every API request. This module owns that token's lifecycle:
//!
//!   - [`TokenCache`] holds the current token and its expiry and decides, per
//!     request, whether the cached token can be reused or a fresh one must be
//!     fetched — proactively, a [`SKEW`] before expiry, so a request never carries
//!     an almost-expired token.
//!   - [`fetch_token_http`] performs the actual exchange against the endpoint.
//!   - The reactive path (a `401` despite a non-expired cached token — a revoked
//!     or server-side-expired token) lives in the poller: it invalidates the cache
//!     and retries once.
//!
//! The fetch step is injected into [`TokenCache::token`] as a closure rather than
//! hard-wired, so the cache's reuse/refresh state machine is unit-testable without
//! any network or real clock.

use std::future::Future;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio::sync::Mutex;

/// OpenSky's OAuth2 token endpoint (Keycloak, `client_credentials` grant). The
/// default; overridable via `FIREFLY_OPENSKY_TOKEN_URL` for testing or a future
/// realm change.
pub(crate) const DEFAULT_TOKEN_URL: &str =
    "https://auth.opensky-network.org/auth/realms/opensky-network/protocol/openid-connect/token";

/// How long before a token's stated expiry we already treat it as due for
/// refresh. Covers clock skew and the round-trip of the request the token is
/// attached to, so a token never expires mid-flight.
const SKEW: Duration = Duration::from_secs(60);

/// A token-acquisition failure (network error, non-2xx from the token endpoint,
/// or an unparseable token response).
#[derive(Debug)]
pub enum AuthError {
    /// HTTP-layer failure talking to the token endpoint.
    Http(reqwest::Error),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // The endpoint URL carries no secret (credentials go in the POST body),
            // so surfacing the reqwest error is safe — it never prints the token.
            AuthError::Http(e) => write!(f, "OAuth2 token request failed: {e}"),
        }
    }
}

impl std::error::Error for AuthError {}

impl From<reqwest::Error> for AuthError {
    fn from(e: reqwest::Error) -> Self {
        AuthError::Http(e)
    }
}

/// A freshly obtained token and its lifetime in seconds, as returned by the token
/// endpoint.
pub(crate) struct FetchedToken {
    pub access_token: String,
    pub expires_in: u64,
}

/// The subset of the token endpoint's JSON response we consume. A missing
/// `expires_in` defaults to 0, which makes the token refresh on next use rather
/// than be trusted indefinitely.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: u64,
}

/// The cached token together with the instant it should be refreshed at.
struct CachedToken {
    token: String,
    expires_at: Instant,
}

/// A thread-safe cache of the current OAuth2 access token.
///
/// `token` returns a valid token, fetching a new one only when none is cached or
/// the cached one is within [`SKEW`] of expiry. The lock is held across the fetch
/// so concurrent callers coalesce onto a single token request rather than
/// stampeding the endpoint.
pub(crate) struct TokenCache {
    inner: Mutex<Option<CachedToken>>,
}

impl TokenCache {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    /// Return a usable access token, calling `fetch` to obtain a new one when the
    /// cache is empty or the cached token is due for refresh. The freshly fetched
    /// token is stored before being returned.
    pub(crate) async fn token<F, Fut>(&self, fetch: F) -> Result<String, AuthError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<FetchedToken, AuthError>>,
    {
        let mut guard = self.inner.lock().await;
        if let Some(cached) = guard.as_ref() {
            if !needs_refresh(Instant::now(), cached.expires_at, SKEW) {
                return Ok(cached.token.clone());
            }
        }
        let fetched = fetch().await?;
        let expires_at = Instant::now() + Duration::from_secs(fetched.expires_in);
        let token = fetched.access_token.clone();
        *guard = Some(CachedToken {
            token: fetched.access_token,
            expires_at,
        });
        Ok(token)
    }

    /// Drop any cached token so the next [`token`](Self::token) call fetches a
    /// fresh one. Used on a `401` to recover from a token the server rejected
    /// before its stated expiry (revocation, server-side expiry).
    pub(crate) async fn invalidate(&self) {
        *self.inner.lock().await = None;
    }
}

/// Whether a token expiring at `expires_at` should be refreshed now: true once we
/// are within `skew` of (or past) the expiry. Pure, so the refresh decision is
/// tested without sleeping.
fn needs_refresh(now: Instant, expires_at: Instant, skew: Duration) -> bool {
    now + skew >= expires_at
}

/// Exchange a `client_id`/`client_secret` for an access token via the OAuth2
/// client-credentials grant. The credentials travel in the form-encoded POST body
/// (never the URL); the returned token's lifetime drives the cache's expiry.
pub(crate) async fn fetch_token_http(
    client: &reqwest::Client,
    token_url: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<FetchedToken, AuthError> {
    let resp = client
        .post(token_url)
        .form(&[
            ("grant_type", "client_credentials"),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ])
        .send()
        .await?
        .error_for_status()?;
    let body: TokenResponse = resp.json().await?;
    Ok(FetchedToken {
        access_token: body.access_token,
        expires_in: body.expires_in,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A valid, far-from-expiry token is reused without re-fetching.
    #[tokio::test]
    async fn reuses_cached_token_until_due() {
        let cache = TokenCache::new();
        let calls = AtomicUsize::new(0);
        let fetch = |calls: &AtomicUsize| {
            calls.fetch_add(1, Ordering::SeqCst);
            async {
                Ok::<_, AuthError>(FetchedToken {
                    access_token: "tok".to_string(),
                    expires_in: 1800,
                })
            }
        };

        let a = cache.token(|| fetch(&calls)).await.unwrap();
        let b = cache.token(|| fetch(&calls)).await.unwrap();
        assert_eq!(a, "tok");
        assert_eq!(b, "tok");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "second call must reuse");
    }

    /// After invalidation (the `401` recovery path) the next call fetches anew.
    #[tokio::test]
    async fn refetches_after_invalidate() {
        let cache = TokenCache::new();
        let calls = AtomicUsize::new(0);
        let fetch = |calls: &AtomicUsize| {
            calls.fetch_add(1, Ordering::SeqCst);
            async {
                Ok::<_, AuthError>(FetchedToken {
                    access_token: "tok".to_string(),
                    expires_in: 1800,
                })
            }
        };

        cache.token(|| fetch(&calls)).await.unwrap();
        cache.invalidate().await;
        cache.token(|| fetch(&calls)).await.unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "invalidate forces a refetch"
        );
    }

    /// A fetch error propagates and leaves the cache empty (no poisoned state).
    #[tokio::test]
    async fn fetch_error_propagates() {
        let cache = TokenCache::new();
        let res = cache
            .token(|| async {
                Err::<FetchedToken, _>(AuthError::Http(
                    reqwest::Client::new()
                        .get("http://%%%")
                        .build()
                        .map(|_| unreachable!())
                        .unwrap_err(),
                ))
            })
            .await;
        assert!(res.is_err());
    }

    /// The refresh decision: reuse while comfortably valid, refresh within the
    /// skew window or once expired.
    #[test]
    fn needs_refresh_respects_skew() {
        let now = Instant::now();
        assert!(
            !needs_refresh(now, now + Duration::from_secs(1800), SKEW),
            "a long-lived token is not yet due"
        );
        assert!(
            needs_refresh(now, now + Duration::from_secs(30), SKEW),
            "within the skew window → refresh"
        );
        assert!(
            needs_refresh(now, now, Duration::from_secs(0)),
            "an already-expired token → refresh"
        );
    }
}
