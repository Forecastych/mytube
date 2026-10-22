//! Trending: the Data API's `mostPopular` chart (YouTube dropped its own
//! page in 2025). 2 quota units per chart: the videos, then their channels'
//! avatars. Cached, so flipping tabs costs nothing.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use super::api;
use crate::error::{Error, Result};
use crate::feed::rank::Candidate;
use crate::feed::rules::Source;
use crate::player::is_video_id;
use crate::youtube::channel::is_channel_id;

/// The chart moves through the day, not by the minute.
pub const TTL_SECS: u64 = 30 * 60;
/// No worldwide chart in the API: "Global" interleaves these.
const GLOBAL: [&str; 5] = ["US", "GB", "IN", "BR", "DE"];
pub const GLOBAL_KEY: &str = "GLOBAL";
const REGION_KEY: &str = "trending.region";

/// `RU`, `US`… or `GLOBAL`.
pub fn valid_region(r: &str) -> bool {
    r == GLOBAL_KEY || (r.len() == 2 && r.bytes().all(|b| b.is_ascii_uppercase()))
}

/// Regions one chart is made of.
pub fn parts(region: &str) -> Vec<&str> {
    if region == GLOBAL_KEY { GLOBAL.to_vec() } else { vec![region] }
}

#[derive(Deserialize)]
struct Page<T> {
    #[serde(default = "Vec::new")]
    items: Vec<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoItem {
    id: String,
    snippet: VideoSnippet,
    #[serde(default)]
    content_details: Details,
    #[serde(default)]
    statistics: Stats,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoSnippet {
    published_at: String,
    channel_id: String,
    title: String,
    #[serde(default)]
    channel_title: String,
    category_id: Option<String>,
}

#[derive(Deserialize, Default)]
struct Details {
    duration: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Stats {
    view_count: Option<String>,
}

#[derive(Deserialize)]
struct ChannelItem {
    id: String,
    snippet: ChannelSnippet,
}

#[derive(Deserialize)]
struct ChannelSnippet {
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

/// ISO 8601 `PT1H2M3S` → seconds. `P0D` (live) and junk → None.
fn parse_duration(s: &str) -> Option<u32> {
    let t = s.strip_prefix("P")?;
    let (days, t) = match t.split_once('D') {
        Some((d, rest)) => (d.parse::<u32>().ok()?, rest),
        None => (0, t),
    };
    let t = t.strip_prefix('T').unwrap_or(t);
    let (mut secs, mut num) = (days * 86_400, 0u32);
    for c in t.chars() {
        match c {
            '0'..='9' => num = num * 10 + c.to_digit(10)?,
            'H' => (secs, num) = (secs + num * 3600, 0),
            'M' => (secs, num) = (secs + num * 60, 0),
            'S' => (secs, num) = (secs + num, 0),
            _ => return None,
        }
    }
    (secs > 0).then_some(secs)
}

/// The API doesn't flag Shorts: short, or tagged as one.
fn looks_short(duration: Option<u32>, title: &str) -> bool {
    match duration {
        Some(d) if d <= 60 => true,
        Some(d) if d <= 180 => title.to_lowercase().contains("#short"),
        _ => false,
    }
}

/// One region's chart, top 50, in chart order.
pub async fn fetch(http: &reqwest::Client, token: &str, region: &str, category: Option<u32>) -> Result<Vec<Candidate>> {
    let mut path = format!("videos?part=snippet,contentDetails,statistics&chart=mostPopular&maxResults=50&regionCode={region}");
    if let Some(c) = category {
        path.push_str(&format!("&videoCategoryId={c}"));
    }
    let page: Page<VideoItem> = match api::get(http, token, &path).await {
        Ok(p) => p,
        // Not every category has a chart in every region.
        Err(e) if e.to_string().contains("chart") || e.to_string().contains("HTTP 404") => Page { items: Vec::new() },
        Err(e) => return Err(e),
    };

    let mut videos: Vec<Candidate> = page
        .items
        .into_iter()
        .filter(|v| is_video_id(&v.id) && is_channel_id(&v.snippet.channel_id))
        .map(|v| {
            let duration = v.content_details.duration.as_deref().and_then(parse_duration);
            Candidate {
                is_short: looks_short(duration, &v.snippet.title),
                id: v.id,
                channel_id: v.snippet.channel_id,
                channel_title: v.snippet.channel_title,
                channel_avatar: None,
                title: v.snippet.title,
                published: OffsetDateTime::parse(&v.snippet.published_at, &Rfc3339).map(|d| d.unix_timestamp()).unwrap_or(0),
                views: v.statistics.view_count.and_then(|s| s.parse().ok()),
                duration,
                category: v.snippet.category_id.and_then(|s| s.parse().ok()),
                is_new: false,
                source: Source::Trending,
                watched: false,
                channel_rank: None,
            }
        })
        .collect();
    if videos.is_empty() {
        return Ok(videos);
    }

    // Avatars: one more unit for up to 50 channels. Not worth failing over.
    let mut ids: Vec<&str> = Vec::new();
    let mut seen = HashSet::new();
    for v in &videos {
        if seen.insert(v.channel_id.as_str()) {
            ids.push(&v.channel_id);
        }
    }
    let path = format!("channels?part=snippet&maxResults=50&id={}", ids.join(","));
    match api::get::<Page<ChannelItem>>(http, token, &path).await {
        Ok(page) => {
            let avatars: HashMap<String, String> =
                page.items.into_iter().filter_map(|c| Some((c.id, c.snippet.thumbnails.default?.url))).collect();
            for v in &mut videos {
                v.channel_avatar = avatars.get(&v.channel_id).cloned();
            }
        }
        Err(e) => eprintln!("trending avatars: {e}"),
    }
    Ok(videos)
}

/// Round-robin over several charts, each video once.
pub fn interleave(charts: Vec<Vec<Candidate>>) -> Vec<Candidate> {
    let mut iters: Vec<_> = charts.into_iter().map(|c| c.into_iter()).collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    loop {
        let mut any = false;
        for it in &mut iters {
            if let Some(v) = it.next() {
                any = true;
                if seen.insert(v.id.clone()) {
                    out.push(v);
                }
            }
        }
        if !any {
            return out;
        }
    }
}

pub fn check_category(c: Option<u32>) -> Result<()> {
    match c {
        Some(c) if c == 0 || c > 99 => Err(Error::msg("Bad category")),
        _ => Ok(()),
    }
}

/// The region last opened on the Trending screen; Home mixes in its chart.
pub fn saved_region(conn: &Connection) -> Result<Option<String>> {
    let r: Option<String> =
        conn.query_row("SELECT value FROM settings WHERE key = ?1", [REGION_KEY], |r| r.get(0)).optional()?;
    Ok(r.filter(|r| valid_region(r)))
}

pub fn save_region(conn: &Connection, region: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![REGION_KEY, region],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(parse_duration("PT4M13S"), Some(253));
        assert_eq!(parse_duration("PT1H"), Some(3600));
        assert_eq!(parse_duration("P1DT2S"), Some(86_402));
        assert_eq!(parse_duration("P0D"), None);
        assert_eq!(parse_duration("garbage"), None);
    }

    #[test]
    fn shorts() {
        assert!(looks_short(Some(45), "anything"));
        assert!(looks_short(Some(120), "Wow #Shorts"));
        assert!(!looks_short(Some(120), "A two-minute trailer"));
        assert!(!looks_short(None, "#shorts"));
    }

    #[test]
    fn regions() {
        assert!(valid_region("RU") && valid_region("GLOBAL"));
        assert!(!valid_region("ru") && !valid_region("USA") && !valid_region("R&"));
    }
}
