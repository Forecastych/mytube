//! The one HTTP client for the whole app. Every request to Google/YouTube goes
//! through it, so the DPI-bypass proxy plugs in here (`Proxy::https`) later.

use std::time::Duration;

/// Plain desktop Chrome. Unofficial endpoints are always hit anonymously:
/// never attach the user's cookies or OAuth token to them.
pub const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";

/// Anonymous "consent given" cookie: skips the EU consent interstitial on
/// youtube.com pages. Not tied to any account.
pub const CONSENT_COOKIE: &str = "SOCS=CAI";

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(25))
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .expect("http client")
}
