//! Sign-in with the user's own Google Cloud OAuth client, and the Data API
//! calls that need it (subscriptions, trending). Optional: without it the
//! app runs on RSS alone.

pub mod api;
pub mod commands;
pub mod oauth;
pub mod vault;

use std::time::{Duration, Instant};

use tokio::sync::{oneshot, Mutex};

use crate::error::{Error, Result};

struct Access {
    token: String,
    until: Instant,
}

#[derive(Default)]
pub struct GoogleState {
    access: Mutex<Option<Access>>,
    /// Set while a sign-in waits for the browser; sending cancels it.
    cancel: std::sync::Mutex<Option<oneshot::Sender<()>>>,
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
}
