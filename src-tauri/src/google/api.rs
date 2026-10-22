//! YouTube Data API v3 with the user's token. Read-only, see oauth::SCOPE.

use serde::Deserialize;

use crate::error::{Error, Result};

const BASE: &str = "https://www.googleapis.com/youtube/v3/";

#[derive(Deserialize)]
struct ErrorBody {
    error: ApiError,
}

#[derive(Deserialize)]
struct ApiError {
    #[serde(default)]
    message: String,
    #[serde(default)]
    errors: Vec<ErrorItem>,
}

#[derive(Deserialize)]
struct ErrorItem {
    #[serde(default)]
    reason: String,
}

fn explain(status: u16, bytes: &[u8]) -> Error {
    let Ok(body) = serde_json::from_slice::<ErrorBody>(bytes) else {
        return Error::msg(format!("YouTube API: HTTP {status}"));
    };
    let reason = body.error.errors.first().map(|e| e.reason.as_str()).unwrap_or("");
    Error::msg(match reason {
        "accessNotConfigured" | "SERVICE_DISABLED" => {
            "YouTube Data API v3 is off in your Google Cloud project. Enable it in APIs & Services → Library.".into()
        }
        "quotaExceeded" | "dailyLimitExceeded" => "Today's YouTube API quota is used up. It resets at midnight Pacific time.".into(),
        "insufficientPermissions" | "ACCESS_TOKEN_SCOPE_INSUFFICIENT" => {
            "MyTube wasn't allowed to read YouTube. Sign in again and tick the YouTube box.".into()
        }
        _ if body.error.message.is_empty() => format!("YouTube API: HTTP {status}"),
        _ => format!("YouTube API: {}", body.error.message),
    })
}

/// `path` is the method plus its query, e.g. `channels?part=snippet&mine=true`.
pub async fn get<T: for<'de> Deserialize<'de>>(http: &reqwest::Client, token: &str, path: &str) -> Result<T> {
    let resp = http.get(format!("{BASE}{path}")).bearer_auth(token).send().await?;
    let status = resp.status();
    let bytes = resp.bytes().await?;
    if !status.is_success() {
        return Err(explain(status.as_u16(), &bytes));
    }
    serde_json::from_slice(&bytes).map_err(|e| Error::msg(format!("Bad YouTube API reply: {e}")))
}

#[derive(Deserialize)]
struct Page<T> {
    #[serde(default = "Vec::new")]
    items: Vec<T>,
}

#[derive(Deserialize)]
struct ChannelItem {
    snippet: ChannelSnippet,
}

#[derive(Deserialize)]
struct ChannelSnippet {
    title: String,
    #[serde(default)]
    thumbnails: Thumbs,
}

#[derive(Deserialize, Default)]
struct Thumbs {
    default: Option<Thumb>,
}

#[derive(Deserialize)]
struct Thumb {
    url: String,
}

/// The signed-in user's own channel: name and avatar. 1 quota unit.
/// None for a Google account that has no YouTube channel.
pub async fn my_channel(http: &reqwest::Client, token: &str) -> Result<Option<(String, Option<String>)>> {
    let page: Page<ChannelItem> = get(http, token, "channels?part=snippet&mine=true").await?;
    Ok(page.items.into_iter().next().map(|c| (c.snippet.title, c.snippet.thumbnails.default.map(|t| t.url))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explains_disabled_api() {
        let body = br#"{"error":{"code":403,"message":"YouTube Data API v3 has not been used","errors":[{"reason":"accessNotConfigured"}]}}"#;
        assert!(explain(403, body).to_string().contains("Enable it"));
        assert_eq!(explain(500, b"oops").to_string(), "YouTube API: HTTP 500");
    }
}
