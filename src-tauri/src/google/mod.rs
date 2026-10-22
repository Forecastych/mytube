//! Sign-in with the user's own Google Cloud OAuth client, and the Data API
//! calls that need it (subscriptions, trending). Optional: without it the
//! app runs on RSS alone.

pub mod api;
pub mod commands;
pub mod oauth;
pub mod subscriptions;
pub mod trending;
pub mod vault;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{oneshot, Mutex};

use crate::error::{Error, Result};
use crate::feed::rank::Candidate;

struct Access {
    token: String,
    until: Instant,
}

#[derive(Default)]
pub struct GoogleState {
    access: Mutex<Option<Access>>,
    /// Set while a sign-in waits for the browser; sending cancels it.
    cancel: std::sync::Mutex<Option<oneshot::Sender<()>>>,
    /// Trending charts by `REGION:category`, see `trending::TTL_SECS`.
    charts: Mutex<HashMap<String, (Instant, Arc<Vec<Candidate>>)>>,
}

/// Refresh this long before Google's expiry, so a token never dies mid-request.
const MARGIN: Duration = Duration::from_secs(60);

impl GoogleState {
    async fn remember(&self, token: String, expires_in: u64) {
        let until = Instant::now() + Duration::from_secs(expires_in).saturating_sub(MARGIN);
        *self.access.lock().await = Some(Access { token, until });
    }

    async fn forget(&self) {
        *self.access.lock().await = None;
    }

    /// A valid access token, refreshed if needed. Errors if not signed in.
    pub async fn access_token(&self, http: &reqwest::Client) -> Result<String> {
        let mut access = self.access.lock().await;
        if let Some(a) = access.as_ref().filter(|a| a.until > Instant::now()) {
            return Ok(a.token.clone());
        }
        let not_signed_in = || Error::msg("Sign in with Google in Settings first.");
        let client = vault::client()?.ok_or_else(not_signed_in)?;
        let refresh = vault::refresh_token()?.ok_or_else(not_signed_in)?;
        let tokens = match oauth::refresh(http, &client, &refresh).await {
            Ok(t) => t,
            Err(e) => {
                if e.to_string() == oauth::SESSION_ENDED {
                    vault::clear_refresh_token()?;
                }
                return Err(e);
            }
        };
        let until = Instant::now() + Duration::from_secs(tokens.expires_in).saturating_sub(MARGIN);
        *access = Some(Access { token: tokens.access_token.clone(), until });
        Ok(tokens.access_token)
    }

    /// A trending chart, from cache when fresh. `region` may be `GLOBAL`.
    pub async fn chart(&self, http: &reqwest::Client, region: &str, category: Option<u32>) -> Result<Vec<Candidate>> {
        // Held across the fetch: two screens asking at once make one request.
        let mut charts = self.charts.lock().await;
        let ttl = Duration::from_secs(trending::TTL_SECS);
        charts.retain(|_, (at, _)| at.elapsed() < ttl);
        let mut parts = Vec::new();
        for r in trending::parts(region) {
            let key = format!("{r}:{}", category.unwrap_or(0));
            let chart = match charts.get(&key) {
                Some((_, c)) => c.clone(),
                None => {
                    let token = self.access_token(http).await?;
                    let c = Arc::new(trending::fetch(http, &token, r, category).await?);
                    charts.insert(key, (Instant::now(), c.clone()));
                    c
                }
            };
            parts.push(chart.as_ref().clone());
        }
        Ok(if parts.len() == 1 { parts.pop().unwrap_or_default() } else { trending::interleave(parts) })
    }
}
