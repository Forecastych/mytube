//! Search through Innertube, like comments: anonymous, no Data API quota
//! (`search.list` would cost 100 units a query).
//!
//! Results come as a list of sections; each entry is a renderer. Videos and
//! channels are taken, playlists, Shorts shelves and ads are skipped. Inside
//! an entry `find_all` looks for the renderer wherever it sits, so a moved
//! wrapper doesn't break parsing.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::State;

use super::comments::{find_all, token_in, CLIENT_VERSION};
use crate::commands::AppState;
use crate::error::{Error, Result};
use crate::feed::rank::{self, Candidate};
use crate::feed::rules::Source;
use crate::feed::store;

const SEARCH: &str = "https://www.youtube.com/youtubei/v1/search?prettyPrint=false";
const MAX_QUERY: usize = 200;
/// Going back from a video to the results shouldn't ask YouTube again.
const CACHE_TTL: Duration = Duration::from_secs(10 * 60);
const CACHE_SIZE: usize = 20;

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChannelHit {
    pub id: String,
    pub title: String,
    pub avatar: Option<String>,
    /// "@kurzgesagt"
    pub handle: Option<String>,
    /// "25.7M subscribers"
    pub subscribers: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct Raw {
    channels: Vec<ChannelHit>,
    videos: Vec<Candidate>,
    next: Option<String>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub channels: Vec<ChannelHit>,
    pub videos: Vec<Candidate>,
    /// Dropped by the Algorithm rules.
    pub hidden: u32,
    pub next: Option<String>,
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn cache() -> &'static Mutex<HashMap<String, (Instant, Raw)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, Raw)>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

async fn post(http: &reqwest::Client, mut body: Value) -> Result<Value> {
    body["context"] = json!({ "client": { "clientName": "WEB", "clientVersion": CLIENT_VERSION, "hl": "en" } });
    let res = http
        .post(SEARCH)
        .header("Content-Type", "application/json")
        .header("X-YouTube-Client-Name", "1")
        .header("X-YouTube-Client-Version", CLIENT_VERSION)
        .body(body.to_string())
        .send()
        .await?;
    if !res.status().is_success() {
        return Err(Error::msg(format!("YouTube answered {} for search", res.status())));
    }
    let bytes = res.bytes().await?;
    serde_json::from_slice(&bytes).map_err(|e| Error::msg(format!("Bad search answer: {e}")))
}

/// First page for `query`, or the page behind `next`. Rules are applied on
/// every call, so a channel blocked a minute ago is gone from cached results too.
#[tauri::command]
pub async fn search(state: State<'_, AppState>, query: String, next: Option<String>) -> Result<SearchPage> {
    let query = query.trim();
    if query.is_empty() || query.len() > MAX_QUERY {
        return Err(Error::msg("Bad search query"));
    }
    let key = match &next {
        Some(token) => format!("next:{token}"),
        None => format!("q:{}", query.to_lowercase()),
    };
    let cached = cache()
        .lock()
        .ok()
        .and_then(|c| c.get(&key).filter(|(at, _)| at.elapsed() < CACHE_TTL).map(|(_, r)| r.clone()));
    let raw = match cached {
        Some(r) => r,
        None => {
            let body = match &next {
                Some(token) => json!({ "continuation": token }),
                None => json!({ "query": query }),
            };
            let raw = parse(&post(&state.http, body).await?, now());
            if let Ok(mut c) = cache().lock() {
                c.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
                if c.len() >= CACHE_SIZE {
                    let oldest = c.iter().min_by_key(|(_, (at, _))| *at).map(|(k, _)| k.clone());
                    oldest.map(|k| c.remove(&k));
                }
                c.insert(key, (Instant::now(), raw.clone()));
            }
            raw
        }
    };

    let rules = {
        let conn = state.db.lock().map_err(|_| Error::msg("Database lock poisoned"))?;
        store::load_rules(&conn)?
    };
    let blocked: HashSet<&str> = rules.blocked_channels.iter().map(|c| c.id.as_str()).collect();
    let channels = raw.channels.into_iter().filter(|c| !blocked.contains(c.id.as_str())).collect();
    let feed = rank::filter(raw.videos, &rules);
    Ok(SearchPage { channels, videos: feed.videos, hidden: feed.hidden, next: raw.next })
}

fn parse(v: &Value, now: i64) -> Raw {
    // First page: the result list; later pages: an append action.
    let mut sections: Vec<&Value> = Vec::new();
    let first = &v["contents"]["twoColumnSearchResultsRenderer"]["primaryContents"]["sectionListRenderer"]["contents"];
    sections.extend(first.as_array().into_iter().flatten());
    for cmd in v["onResponseReceivedCommands"].as_array().into_iter().flatten() {
        sections.extend(cmd["appendContinuationItemsAction"]["continuationItems"].as_array().into_iter().flatten());
    }

    let mut raw = Raw::default();
    let mut seen = HashSet::new();
    for section in sections {
        if let Some(c) = section.get("continuationItemRenderer") {
            raw.next = token_in(c);
            continue;
        }
        for entry in section["itemSectionRenderer"]["contents"].as_array().into_iter().flatten() {
            if let Some(c) = entry.get("channelRenderer") {
                raw.channels.extend(channel(c));
                continue;
            }
            let mut found = Vec::new();
            find_all(entry, "videoRenderer", &mut found);
            for r in found {
                if let Some(c) = video(r, now) {
                    if seen.insert(c.id.clone()) {
                        raw.videos.push(c);
                    }
                }
            }
        }
    }
    raw
}

/// `{simpleText}` or `{runs: [{text}]}`.
fn text(v: &Value) -> Option<String> {
    if let Some(s) = v["simpleText"].as_str() {
        return Some(s.to_owned());
    }
    let runs = v["runs"].as_array()?;
    let s: String = runs.iter().filter_map(|r| r["text"].as_str()).collect();
    (!s.is_empty()).then_some(s)
}

fn https(url: &str) -> String {
    if url.starts_with("//") { format!("https:{url}") } else { url.to_owned() }
}

fn last_thumb(v: &Value) -> Option<String> {
    v["thumbnails"].as_array()?.last()?["url"].as_str().map(https)
}

fn video(r: &Value, now: i64) -> Option<Candidate> {
    let id = r["videoId"].as_str()?.to_owned();
    if !crate::player::is_video_id(&id) {
        return None;
    }
    let owner = &r["ownerText"]["runs"][0];
    let live = r["badges"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|b| b["metadataBadgeRenderer"]["style"] == "BADGE_STYLE_TYPE_LIVE_NOW");
    let published = text(&r["publishedTimeText"]).and_then(|t| ago_secs(&t)).map(|s| now - s);
    Some(Candidate {
        channel_id: owner["navigationEndpoint"]["browseEndpoint"]["browseId"].as_str()?.to_owned(),
        channel_title: owner["text"].as_str().unwrap_or_default().to_owned(),
        channel_avatar: last_thumb(&r["channelThumbnailSupportedRenderers"]["channelThumbnailWithLinkRenderer"]["thumbnail"]),
        title: text(&r["title"]).unwrap_or_default(),
        // Live streams have no date: "now" is the honest answer.
        published: published.unwrap_or(if live { now } else { 0 }),
        views: text(&r["viewCountText"]).and_then(|t| count(&t)),
        duration: text(&r["lengthText"]).and_then(|t| clock_secs(&t)),
        category: None,
        is_short: r["navigationEndpoint"].get("reelWatchEndpoint").is_some(),
        is_new: false,
        source: Source::Related,
        watched: false,
        channel_rank: None,
        id,
    })
}

fn channel(r: &Value) -> Option<ChannelHit> {
    let id = r["channelId"].as_str()?.to_owned();
    // YouTube now puts the handle in `subscriberCountText` and the count in
    // `videoCountText`; look at what the text says, not where it is.
    let texts: Vec<String> = [&r["subscriberCountText"], &r["videoCountText"]].into_iter().filter_map(text).collect();
    Some(ChannelHit {
        id,
        title: text(&r["title"]).unwrap_or_default(),
        avatar: last_thumb(&r["thumbnail"]),
        handle: texts.iter().find(|t| t.starts_with('@')).cloned(),
        subscribers: texts.iter().find(|t| t.contains("subscriber")).cloned(),
    })
}

/// "3 days ago", "Streamed 2 weeks ago" → seconds.
fn ago_secs(t: &str) -> Option<i64> {
    let words: Vec<&str> = t.split_whitespace().collect();
    let i = words.iter().position(|w| w.chars().all(|c| c.is_ascii_digit()))?;
    let n: i64 = words[i].parse().ok()?;
    let unit = words.get(i + 1)?.trim_end_matches('s');
    let secs = match unit {
        "second" => 1,
        "minute" => 60,
        "hour" => 3600,
        "day" => 86_400,
        "week" => 7 * 86_400,
        "month" => 30 * 86_400,
        "year" => 365 * 86_400,
        _ => return None,
    };
    Some(n * secs)
}

/// "3,634,811 views" → 3634811; "No views" → 0.
fn count(t: &str) -> Option<i64> {
    if t.starts_with("No ") {
        return Some(0);
    }
    let digits: String = t.split_whitespace().next()?.chars().filter(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// "1:02:03" → 3723.
fn clock_secs(t: &str) -> Option<u32> {
    t.trim().split(':').try_fold(0u32, |acc, p| Some(acc * 60 + p.parse::<u32>().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vr(id: &str, channel: &str, extra: Value) -> Value {
        let mut v = json!({ "videoRenderer": {
            "videoId": id,
            "title": { "runs": [{ "text": format!("Video {id}") }] },
            "ownerText": { "runs": [{ "text": "Chan", "navigationEndpoint": { "browseEndpoint": { "browseId": channel } } }] },
            "publishedTimeText": { "simpleText": "2 days ago" },
            "lengthText": { "simpleText": "1:02:03" },
            "viewCountText": { "simpleText": "3,634,811 views" },
            "navigationEndpoint": { "watchEndpoint": { "videoId": id } }
        }});
        if let (Some(o), Some(e)) = (v["videoRenderer"].as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        v
    }

    #[test]
    fn parses_first_page() {
        let v = json!({ "contents": { "twoColumnSearchResultsRenderer": { "primaryContents": { "sectionListRenderer": { "contents": [
            { "itemSectionRenderer": { "contents": [
                { "channelRenderer": {
                    "channelId": "UCsXVk37bltHxD1rDPwtNM8Q",
                    "title": { "simpleText": "Kurzgesagt" },
                    "thumbnail": { "thumbnails": [{ "url": "//yt3.googleusercontent.com/a=s88" }, { "url": "//yt3.googleusercontent.com/a=s176" }] },
                    "subscriberCountText": { "simpleText": "@kurzgesagt" },
                    "videoCountText": { "simpleText": "25.7M subscribers" }
                }},
                vr("PXbVn-f_wuM", "UCa", json!({})),
                { "shelfRenderer": { "content": { "verticalListRenderer": { "items": [
                    vr("PXbVn-f_wuM", "UCa", json!({})),
                    vr("aaaaaaaaaaa", "UCb", json!({ "publishedTimeText": null, "lengthText": null, "viewCountText": { "runs": [{ "text": "1,234" }, { "text": " watching" }] },
                        "badges": [{ "metadataBadgeRenderer": { "style": "BADGE_STYLE_TYPE_LIVE_NOW" } }] }))
                ]}}}},
                vr("bbbbbbbbbbb", "UCc", json!({ "navigationEndpoint": { "reelWatchEndpoint": {} } })),
                { "lockupViewModel": { "contentId": "PL123" } }
            ]}},
            { "continuationItemRenderer": { "continuationEndpoint": { "continuationCommand": { "token": "NEXT" } } } }
        ]}}}}});
        let r = parse(&v, 1_000_000);
        assert_eq!(r.channels.len(), 1);
        let c = &r.channels[0];
        assert_eq!(c.avatar.as_deref(), Some("https://yt3.googleusercontent.com/a=s176"));
        assert_eq!(c.handle.as_deref(), Some("@kurzgesagt"));
        assert_eq!(c.subscribers.as_deref(), Some("25.7M subscribers"));

        let ids: Vec<_> = r.videos.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, ["PXbVn-f_wuM", "aaaaaaaaaaa", "bbbbbbbbbbb"], "shelf duplicates are dropped");
        let first = &r.videos[0];
        assert_eq!(first.duration, Some(3723));
        assert_eq!(first.views, Some(3_634_811));
        assert_eq!(first.published, 1_000_000 - 2 * 86_400);
        assert_eq!(first.channel_id, "UCa");
        let live = &r.videos[1];
        assert_eq!((live.published, live.duration, live.views), (1_000_000, None, Some(1234)));
        assert!(r.videos[2].is_short);
        assert_eq!(r.next.as_deref(), Some("NEXT"));
    }

    #[test]
    fn parses_next_page() {
        let v = json!({ "onResponseReceivedCommands": [{ "appendContinuationItemsAction": { "continuationItems": [
            { "itemSectionRenderer": { "contents": [vr("ccccccccccc", "UCa", json!({}))] } },
            { "continuationItemRenderer": { "continuationEndpoint": { "continuationCommand": { "token": "MORE" } } } }
        ]}}]});
        let r = parse(&v, 0);
        assert_eq!(r.videos.len(), 1);
        assert_eq!(r.next.as_deref(), Some("MORE"));
    }

    #[test]
    fn reads_texts() {
        assert_eq!(ago_secs("Streamed 3 weeks ago"), Some(21 * 86_400));
        assert_eq!(ago_secs("1 year ago"), Some(365 * 86_400));
        assert_eq!(ago_secs("yesterday"), None);
        assert_eq!(count("No views"), Some(0));
        assert_eq!(clock_secs("3:34"), Some(214));
        assert_eq!(clock_secs("LIVE"), None);
    }

    /// `SEARCH_FILE=answer.json cargo test search::tests::real_file -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_file() {
        let path = std::env::var("SEARCH_FILE").expect("SEARCH_FILE");
        let v: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let r = parse(&v, now());
        for c in &r.channels {
            println!("channel {} {:?} {:?} {:?}", c.title, c.handle, c.subscribers, c.avatar.is_some());
        }
        for v in &r.videos {
            println!("{} {:>8?}s {:>10?} views {:>9}s ago short={} {} | {}", v.id, v.duration, v.views, now() - v.published, v.is_short, v.channel_title, v.title);
        }
        println!("next: {}", r.next.is_some());
        assert!(!r.videos.is_empty());
    }
}
