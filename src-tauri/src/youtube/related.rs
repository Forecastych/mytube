//! Related videos: the column next to the player on youtube.com, asked from
//! Innertube `next` without cookies. So it's "videos like this one", not
//! YouTube's guess about the user: no account, no history goes in.
//!
//! Each entry is a `lockupViewModel`: title and channel in
//! `lockupMetadataViewModel`, views and age as two rows of
//! `contentMetadataViewModel`, duration as a badge over the thumbnail.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::State;

use super::innertube::{self, find_all};
use super::search::{ago_secs, clock_secs, now};
use crate::commands::AppState;
use crate::error::{Error, Result};
use crate::feed::rank::{self, Candidate, Ranked};
use crate::feed::rules::Source;
use crate::feed::store;

/// Related lists change slowly; Home asks for several at once.
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);
const CACHE_SIZE: usize = 60;

fn cache() -> &'static Mutex<HashMap<String, (Instant, Vec<Candidate>)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, Vec<Candidate>)>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Videos YouTube shows next to `video_id`, unfiltered. Cached for an hour.
pub async fn fetch(http: &reqwest::Client, video_id: &str) -> Result<Vec<Candidate>> {
    if !crate::player::is_video_id(video_id) {
        return Err(Error::msg("Bad video id"));
    }
    let cached = cache()
        .lock()
        .ok()
        .and_then(|c| c.get(video_id).filter(|(at, _)| at.elapsed() < CACHE_TTL).map(|(_, v)| v.clone()));
    if let Some(v) = cached {
        return Ok(v);
    }
    let answer = innertube::post(http, "next", json!({ "videoId": video_id }), "related videos").await?;
    let videos = parse(&answer, now());
    if let Ok(mut c) = cache().lock() {
        c.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
        if c.len() >= CACHE_SIZE {
            let oldest = c.iter().min_by_key(|(_, (at, _))| *at).map(|(k, _)| k.clone());
            oldest.map(|k| c.remove(&k));
        }
        c.insert(video_id.to_owned(), (Instant::now(), videos.clone()));
    }
    Ok(videos)
}

/// Related videos of several seeds, interleaved so no seed takes over the
/// top. Seeds themselves are left out. Returns the seeds that failed too.
pub async fn for_seeds(http: &reqwest::Client, seeds: &[String]) -> (Vec<Candidate>, usize) {
    let tasks: Vec<_> = seeds
        .iter()
        .map(|id| {
            let (http, id) = (http.clone(), id.clone());
            tauri::async_runtime::spawn(async move { fetch(&http, &id).await })
        })
        .collect();
    let mut lists = Vec::new();
    let mut failed = 0;
    for task in tasks {
        match task.await {
            Ok(Ok(list)) => lists.push(list.into_iter()),
            Ok(Err(e)) => {
                eprintln!("related: {e}");
                failed += 1;
            }
            Err(_) => failed += 1,
        }
    }
    let mut out = Vec::new();
    loop {
        let before = out.len();
        for list in &mut lists {
            out.extend(list.next());
        }
        if out.len() == before {
            break;
        }
    }
    out.retain(|c| !seeds.contains(&c.id));
    (out, failed)
}

/// Up next on the Watch screen, through the Algorithm rules.
#[tauri::command]
pub async fn related(state: State<'_, AppState>, id: String) -> Result<Ranked> {
    let videos = fetch(&state.http, &id).await?;
    let rules = {
        let conn = state.db.lock().map_err(|_| Error::msg("Database lock poisoned"))?;
        store::load_rules(&conn)?
    };
    Ok(rank::filter(videos.into_iter().filter(|v| v.id != id).collect(), &rules))
}

fn parse(v: &Value, now: i64) -> Vec<Candidate> {
    let mut lockups = Vec::new();
    find_all(&v["contents"]["twoColumnWatchNextResults"]["secondaryResults"], "lockupViewModel", &mut lockups);
    let mut seen = std::collections::HashSet::new();
    lockups.into_iter().filter_map(|l| video(l, now)).filter(|c| seen.insert(c.id.clone())).collect()
}

fn video(l: &Value, now: i64) -> Option<Candidate> {
    // Mixes and playlists come as lockups too.
    if l["contentType"] != "LOCKUP_CONTENT_TYPE_VIDEO" {
        return None;
    }
    let id = l["contentId"].as_str()?.to_owned();
    if !crate::player::is_video_id(&id) {
        return None;
    }
    let md = &l["metadata"]["lockupMetadataViewModel"];
    let avatar = &md["image"]["decoratedAvatarViewModel"];
    // Collaborations ("X and 5 more") have no single channel: skipped, since
    // every card links to its channel.
    let channel_id = avatar["rendererContext"]["commandContext"]["onTap"]["innertubeCommand"]["browseEndpoint"]["browseId"]
        .as_str()
        .filter(|id| super::channel::is_channel_id(id))?
        .to_owned();
    let rows = md["metadata"]["contentMetadataViewModel"]["metadataRows"].as_array()?;
    let part = |row: usize, i: usize| -> Option<&Value> { rows.get(row)?["metadataParts"].as_array()?.get(i) };

    let mut badges = Vec::new();
    find_all(&l["contentImage"], "thumbnailBadgeViewModel", &mut badges);
    let duration = badges.iter().find_map(|b| b["text"].as_str().and_then(clock_secs));
    let age = spoken(part(1, 1)).and_then(ago_secs);
    let live = badges.iter().any(|b| b["text"].as_str().is_some_and(|t| t.eq_ignore_ascii_case("live")));

    Some(Candidate {
        channel_id,
        channel_title: spoken(part(0, 0)).unwrap_or_default().to_owned(),
        channel_avatar: avatar["avatar"]["avatarViewModel"]["image"]["sources"][0]["url"].as_str().map(str::to_owned),
        title: md["title"]["content"].as_str().unwrap_or_default().to_owned(),
        published: age.map(|s| now - s).unwrap_or(if live { now } else { 0 }),
        views: spoken(part(1, 0)).and_then(spoken_count),
        duration,
        category: None,
        is_short: false,
        is_new: false,
        source: Source::Related,
        watched: false,
        channel_rank: None,
        id,
    })
}

/// The spelled-out text ("2 months ago") over the compact one ("2mo ago").
fn spoken(p: Option<&Value>) -> Option<&str> {
    let p = p?;
    p["accessibilityLabel"].as_str().or(p["text"]["content"].as_str())
}

/// "1.3 billion views" / "92 thousand views" / "1,234 views" / "1.3M" → a number.
fn spoken_count(t: &str) -> Option<i64> {
    if t.starts_with("No ") {
        return Some(0);
    }
    let mut words = t.split_whitespace();
    let first = words.next()?.replace(',', "");
    let (num, suffix) = match first.find(|c: char| c.is_ascii_alphabetic()) {
        Some(i) => (first[..i].to_owned(), first[i..].to_owned()),
        None => (first, words.next().unwrap_or_default().to_owned()),
    };
    let n: f64 = num.parse().ok()?;
    let mult = match suffix.to_ascii_lowercase().as_str() {
        "k" | "thousand" => 1e3,
        "m" | "million" => 1e6,
        "b" | "billion" => 1e9,
        _ => 1.0,
    };
    Some((n * mult).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lockup(id: &str, channel: Option<&str>, views: &str, age: &str, duration: &str) -> Value {
        let mut avatar = json!({ "avatar": { "avatarViewModel": { "image": { "sources": [{ "url": "https://yt3.ggpht.com/a=s68" }] } } } });
        if let Some(c) = channel {
            avatar["rendererContext"] = json!({ "commandContext": { "onTap": { "innertubeCommand": { "browseEndpoint": { "browseId": c } } } } });
        }
        json!({ "lockupViewModel": {
            "contentId": id,
            "contentType": "LOCKUP_CONTENT_TYPE_VIDEO",
            "contentImage": { "thumbnailViewModel": { "overlays": [{ "thumbnailBottomOverlayViewModel": { "badges": [{ "thumbnailBadgeViewModel": { "text": duration } }] } }] } },
            "metadata": { "lockupMetadataViewModel": {
                "title": { "content": format!("Video {id}") },
                "image": { "decoratedAvatarViewModel": avatar },
                "metadata": { "contentMetadataViewModel": { "metadataRows": [
                    { "metadataParts": [{ "text": { "content": "Chan" } }] },
                    { "metadataParts": [
                        { "text": { "content": "x" }, "accessibilityLabel": views },
                        { "text": { "content": "x" }, "accessibilityLabel": age }
                    ]}
                ]}}
            }}
        }})
    }

    #[test]
    fn parses_lockups() {
        let v = json!({ "contents": { "twoColumnWatchNextResults": { "secondaryResults": { "secondaryResults": { "results": [
            lockup("pCJ9JGG0GQI", Some("UCJDFBaUcOVUtT8nrKSkAzAQ"), "1.3 billion views", "2 months ago", "13:42"),
            lockup("pCJ9JGG0GQI", Some("UCJDFBaUcOVUtT8nrKSkAzAQ"), "1 view", "1 day ago", "1:00"),
            lockup("aaaaaaaaaaa", None, "5 views", "1 day ago", "1:00"),
            { "lockupViewModel": { "contentId": "RDxyz", "contentType": "LOCKUP_CONTENT_TYPE_PLAYLIST" } },
            lockup("bbbbbbbbbbb", Some("UCmSp4bDxS9R0jpeZEvkut2g"), "92 thousand views", "Streamed 1 hour ago", "1:02:03"),
            { "continuationItemRenderer": {} }
        ]}}}}});
        let r = parse(&v, 10_000_000);
        let ids: Vec<_> = r.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["pCJ9JGG0GQI", "bbbbbbbbbbb"], "duplicates, collabs and playlists are dropped");
        assert_eq!(r[0].views, Some(1_300_000_000));
        assert_eq!(r[0].duration, Some(822));
        assert_eq!(r[0].published, 10_000_000 - 60 * 86_400);
        assert_eq!(r[0].channel_title, "Chan");
        assert_eq!(r[1].views, Some(92_000));
        assert_eq!(r[1].published, 10_000_000 - 3600);
    }

    #[test]
    fn reads_counts() {
        assert_eq!(spoken_count("3.3M"), Some(3_300_000));
        assert_eq!(spoken_count("1,234 views"), Some(1234));
        assert_eq!(spoken_count("No views"), Some(0));
        assert_eq!(spoken_count("12K"), Some(12_000));
    }

    /// Hits YouTube: `cargo test related::tests::live -- --ignored --nocapture`,
    /// another video through `RELATED_ID=...`.
    #[tokio::test]
    #[ignore]
    async fn live() {
        let http = crate::net::client();
        let id = std::env::var("RELATED_ID").unwrap_or_else(|_| "dQw4w9WgXcQ".into());
        let r = fetch(&http, &id).await.unwrap();
        for c in &r {
            println!("{} {:?}s {:?} views {} | {}", c.id, c.duration, c.views, c.channel_title, c.title);
        }
        assert!(r.len() > 5);
    }
}
