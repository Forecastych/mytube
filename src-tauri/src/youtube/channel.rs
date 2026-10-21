//! Turn whatever the user pasted (link, @handle, UC… id) into a channel id,
//! title and avatar — without the Data API, anonymously.

use reqwest::Url;

use crate::error::{Error, Result};
use crate::net::CONSENT_COOKIE;

pub struct ChannelInfo {
    pub id: String,
    pub title: String,
    pub avatar: Option<String>,
}

const BAD_INPUT: &str = "Paste a channel link like youtube.com/@name";

/// The channel page is ~3 MB but the tags we need sit near the start
/// (~820 KB in 2026). Stop reading once all are found, or at this cap.
const MAX_PAGE_BYTES: usize = 2 * 1024 * 1024;

pub fn is_channel_id(s: &str) -> bool {
    s.len() == 24
        && s.starts_with("UC")
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Normalise input to a channel page URL on youtube.com. Rejects other hosts,
/// so we never fetch arbitrary URLs.
pub fn page_url(input: &str) -> Result<Url> {
    let s = input.trim();
    if is_channel_id(s) {
        return Url::parse(&format!("https://www.youtube.com/channel/{s}")).map_err(|_| Error::msg(BAD_INPUT));
    }
    if s.starts_with('@') {
        return Url::parse(&format!("https://www.youtube.com/{s}")).map_err(|_| Error::msg(BAD_INPUT));
    }

    let with_scheme = if s.contains("://") { s.to_owned() } else { format!("https://{s}") };
    let url = Url::parse(&with_scheme).map_err(|_| Error::msg(BAD_INPUT))?;
    if !matches!(url.host_str(), Some("youtube.com" | "www.youtube.com" | "m.youtube.com")) {
        return Err(Error::msg(BAD_INPUT));
    }

    let segs: Vec<&str> = url.path_segments().map(|p| p.filter(|x| !x.is_empty()).collect()).unwrap_or_default();
    let path = match segs.as_slice() {
        [h, ..] if h.starts_with('@') => format!("/{h}"),
        ["channel", id, ..] if is_channel_id(id) => format!("/channel/{id}"),
        [kind @ ("c" | "user"), name, ..] => format!("/{kind}/{name}"),
        _ => return Err(Error::msg(BAD_INPUT)),
    };
    let mut out = Url::parse("https://www.youtube.com").expect("static url");
    out.set_path(&path);
    Ok(out)
}

pub async fn resolve(http: &reqwest::Client, input: &str) -> Result<ChannelInfo> {
    let url = page_url(input)?;
    let mut resp = http
        .get(url)
        .header(reqwest::header::COOKIE, CONSENT_COOKIE)
        .header(reqwest::header::ACCEPT_LANGUAGE, "en")
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(Error::msg("Channel not found"));
    }
    resp.error_for_status_ref()?;

    let mut buf: Vec<u8> = Vec::with_capacity(1024 * 1024);
    let mut canonical = None;
    let mut title = None;
    let mut image = None;

    while let Some(chunk) = resp.chunk().await? {
        // Rescan with overlap so a tag split across chunks is still found.
        let from = buf.len().saturating_sub(2048);
        buf.extend_from_slice(&chunk);
        let hay = &buf[from..];
        canonical = canonical.or_else(|| attr_after(hay, b"<link rel=\"canonical\" href=\""));
        title = title.or_else(|| attr_after(hay, b"<meta property=\"og:title\" content=\""));
        image = image.or_else(|| attr_after(hay, b"<meta property=\"og:image\" content=\""));
        if (canonical.is_some() && title.is_some() && image.is_some()) || buf.len() > MAX_PAGE_BYTES {
            break; // dropping `resp` aborts the rest of the download
        }
    }

    let id = canonical
        .as_deref()
        .and_then(|c| c.rsplit('/').next())
        .filter(|id| is_channel_id(id))
        .map(str::to_owned)
        .ok_or_else(|| Error::msg("Couldn't read the channel id from YouTube's page"))?;

    Ok(ChannelInfo {
        id,
        title: title.map(|t| decode_html(&t)).unwrap_or_default(),
        avatar: image.map(|u| sized_avatar(&decode_html(&u), 176)),
    })
}

/// Value of a quoted attribute right after `prefix`, if the closing quote is in `hay`.
fn attr_after(hay: &[u8], prefix: &[u8]) -> Option<String> {
    let start = find(hay, prefix)? + prefix.len();
    let len = hay[start..].iter().position(|&b| b == b'"')?;
    String::from_utf8(hay[start..start + len].to_vec()).ok()
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn decode_html(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    s.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// `…=s900-c-k-…` → `…=s176-c-k-…`: fetch a small avatar, not a 900 px one.
pub fn sized_avatar(url: &str, px: u32) -> String {
    let Some(eq) = url.rfind("=s") else { return url.to_owned() };
    let digits = url[eq + 2..].bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return url.to_owned();
    }
    format!("{}=s{px}{}", &url[..eq], &url[eq + 2 + digits..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_inputs() {
        let ok = |s: &str| page_url(s).unwrap().to_string();
        assert_eq!(ok("@veritasium"), "https://www.youtube.com/@veritasium");
        assert_eq!(ok("https://www.youtube.com/@veritasium/videos?x=1"), "https://www.youtube.com/@veritasium");
        assert_eq!(ok("youtube.com/channel/UCHnyfMqiRRG1u-2MsSQLbXA"), "https://www.youtube.com/channel/UCHnyfMqiRRG1u-2MsSQLbXA");
        assert_eq!(ok("UCHnyfMqiRRG1u-2MsSQLbXA"), "https://www.youtube.com/channel/UCHnyfMqiRRG1u-2MsSQLbXA");
        assert_eq!(ok("m.youtube.com/c/Foo"), "https://www.youtube.com/c/Foo");
        assert!(page_url("https://evil.example/@x").is_err());
        assert!(page_url("https://www.youtube.com/watch?v=abc").is_err());
    }

    #[test]
    fn resizes_avatar() {
        assert_eq!(
            sized_avatar("https://yt3.googleusercontent.com/abc=s900-c-k-c0x00ffffff-no-rj", 176),
            "https://yt3.googleusercontent.com/abc=s176-c-k-c0x00ffffff-no-rj"
        );
    }

    #[test]
    fn finds_attr_split_safe() {
        let html = br#"<link rel="canonical" href="https://www.youtube.com/channel/UCHnyfMqiRRG1u-2MsSQLbXA">"#;
        assert_eq!(
            attr_after(html, b"<link rel=\"canonical\" href=\"").as_deref(),
            Some("https://www.youtube.com/channel/UCHnyfMqiRRG1u-2MsSQLbXA")
        );
        assert_eq!(attr_after(&html[..40], b"<link rel=\"canonical\" href=\""), None);
    }
}

/// Hits real YouTube: `cargo test -- --ignored live`.
#[cfg(test)]
mod live {
    #[tokio::test]
    #[ignore]
    async fn live_resolve_and_rss() {
        let http = crate::net::client();
        let t = std::time::Instant::now();
        let info = super::resolve(&http, "https://www.youtube.com/@veritasium").await.unwrap();
        println!("resolve {:?}: {} {} {:?}", t.elapsed(), info.id, info.title, info.avatar);
        assert_eq!(info.id, "UCHnyfMqiRRG1u-2MsSQLbXA");
        let t = std::time::Instant::now();
        let feed = crate::youtube::rss::fetch(&http, &info.id).await.unwrap();
        let shorts = feed.entries.iter().filter(|e| e.is_short).count();
        println!("rss {:?}: {} entries, {shorts} shorts", t.elapsed(), feed.entries.len());
        assert!(!feed.entries.is_empty());
    }
}
