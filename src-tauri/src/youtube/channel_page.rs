//! The channel screen: header from Innertube `browse`, videos from RSS.
//! Anonymous `browse` answers the Videos tab with continuations only, so the
//! list is what RSS gives: the latest 15 uploads plus the latest 15 long ones.

use std::collections::HashSet;

use rusqlite::OptionalExtension;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::State;

use super::channel::{is_channel_id, sized_avatar};
use super::innertube;
use super::rss::{self, Feed};
use super::search::now;
use crate::commands::AppState;
use crate::db;
use crate::error::{Error, Result};
use crate::feed::rank::Candidate;
use crate::feed::rules::Source;
use crate::feed::store;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPage {
    pub id: String,
    pub title: String,
    pub avatar: Option<String>,
    pub banner: Option<String>,
    /// "@kurzgesagt"
    pub handle: Option<String>,
    /// "25.7M subscribers"
    pub subscribers: Option<String>,
    /// "394 videos"
    pub video_count: Option<String>,
    pub description: Option<String>,
    pub videos: Vec<Candidate>,
    /// Shorts and blocked videos left out.
    pub hidden: u32,
    pub is_favorite: bool,
    pub is_subscribed: bool,
    pub is_blocked: bool,
}

#[tauri::command]
pub async fn channel_page(state: State<'_, AppState>, id: String) -> Result<ChannelPage> {
    if !is_channel_id(&id) {
        return Err(Error::msg("Bad channel id"));
    }
    let (header, feed) = tokio::join!(
        innertube::post(&state.http, "browse", json!({ "browseId": id }), "channel"),
        rss::fetch(&state.http, &id),
    );
    let mut page = match &header {
        Ok(h) => parse_header(h),
        Err(e) => {
            eprintln!("channel {id} header: {e}");
            ChannelPage::default()
        }
    };
    if let (Err(e), Err(_)) = (&feed, &header) {
        return Err(Error::msg(format!("Couldn't open the channel: {e}")));
    }
    page.id = id.clone();

    let t = now();
    let mut conn = state.db.lock().map_err(|_| Error::msg("Database lock poisoned"))?;
    let flags: Option<(bool, bool)> = conn
        .query_row("SELECT favorite, subscribed FROM channels WHERE id = ?1", [&id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    (page.is_favorite, page.is_subscribed) = flags.unwrap_or_default();
    let rules = store::load_rules(&conn)?;
    page.is_blocked = rules.blocked_channels.iter().any(|c| c.id == id);

    let Ok(feed) = feed else { return Ok(page) };
    if page.title.is_empty() {
        page.title = feed.title.clone();
    }
    // A tracked channel's cache is refreshed for free.
    if flags.is_some() {
        db::store_feed(&mut conn, &id, &feed, t)?;
    }
    let watched = watched_ids(&conn, &feed)?;
    let blocked: HashSet<&str> = rules.blocked_videos.iter().map(|v| v.id.as_str()).collect();
    let avatar = page.avatar.as_deref().map(|a| sized_avatar(a, 88));
    let mut videos: Vec<Candidate> = feed
        .entries
        .into_iter()
        .map(|e| Candidate {
            watched: watched.contains(&e.id),
            channel_id: id.clone(),
            channel_title: page.title.clone(),
            channel_avatar: avatar.clone(),
            title: e.title,
            published: e.published,
            views: e.views,
            duration: None,
            category: None,
            is_short: e.is_short,
            is_new: false,
            source: Source::Subscriptions,
            channel_rank: None,
            id: e.id,
        })
        .collect();
    videos.sort_by_key(|v| std::cmp::Reverse(v.published));
    let before = videos.len();
    // The user came to see this channel: only Shorts and blocked videos go,
    // watched ones and blocked words stay.
    videos.retain(|v| !blocked.contains(v.id.as_str()) && !(rules.hide_shorts && v.is_short));
    page.hidden = (before - videos.len()) as u32;
    page.videos = videos;
    Ok(page)
}

fn watched_ids(conn: &rusqlite::Connection, feed: &Feed) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare_cached("SELECT 1 FROM watched WHERE video_id = ?1")?;
    let mut out = HashSet::new();
    for e in &feed.entries {
        if stmt.exists([&e.id])? {
            out.insert(e.id.clone());
        }
    }
    Ok(out)
}

fn parse_header(v: &Value) -> ChannelPage {
    let md = &v["metadata"]["channelMetadataRenderer"];
    let h = &v["header"]["pageHeaderRenderer"]["content"]["pageHeaderViewModel"];
    let parts: Vec<&str> = h["metadata"]["contentMetadataViewModel"]["metadataRows"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|r| r["metadataParts"].as_array().into_iter().flatten())
        .filter_map(|p| p["text"]["content"].as_str())
        .collect();
    let find = |f: &dyn Fn(&str) -> bool| parts.iter().find(|p| f(p)).map(|p| p.to_string());
    let banner = h["banner"]["imageBannerViewModel"]["image"]["sources"]
        .as_array()
        .and_then(|s| s.iter().filter(|s| s["width"].as_u64().unwrap_or(0) <= 2120).max_by_key(|s| s["width"].as_u64()))
        .and_then(|s| s["url"].as_str())
        .map(str::to_owned);
    let text = |v: &Value| v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
    ChannelPage {
        title: md["title"].as_str().unwrap_or_default().to_owned(),
        avatar: md["avatar"]["thumbnails"][0]["url"].as_str().map(|a| sized_avatar(a, 176)),
        banner,
        handle: find(&|p| p.starts_with('@')),
        subscribers: find(&|p| p.contains("subscriber")),
        video_count: find(&|p| p.ends_with(" videos") || p.ends_with(" video")),
        description: text(&md["description"]),
        ..ChannelPage::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_header() {
        let v = json!({
            "metadata": { "channelMetadataRenderer": {
                "title": "Kurzgesagt", "description": " Science. ",
                "avatar": { "thumbnails": [{ "url": "https://yt3.googleusercontent.com/a=s900-c-k-c0x00ffffff-no-rj" }] }
            }},
            "header": { "pageHeaderRenderer": { "content": { "pageHeaderViewModel": {
                "metadata": { "contentMetadataViewModel": { "metadataRows": [
                    { "metadataParts": [{ "text": { "content": "@kurzgesagt" } }] },
                    { "metadataParts": [{ "text": { "content": "25.7M subscribers" } }, { "text": { "content": "394 videos" } }] }
                ]}},
                "banner": { "imageBannerViewModel": { "image": { "sources": [
                    { "url": "b1060", "width": 1060 }, { "url": "b2120", "width": 2120 }, { "url": "b2560", "width": 2560 }
                ]}}}
            }}}}
        });
        let p = parse_header(&v);
        assert_eq!(p.title, "Kurzgesagt");
        assert_eq!(p.handle.as_deref(), Some("@kurzgesagt"));
        assert_eq!(p.subscribers.as_deref(), Some("25.7M subscribers"));
        assert_eq!(p.video_count.as_deref(), Some("394 videos"));
        assert_eq!(p.banner.as_deref(), Some("b2120"));
        assert_eq!(p.description.as_deref(), Some("Science."));
        assert!(p.avatar.unwrap().contains("=s176"));
        assert_eq!(parse_header(&json!({})).title, "");
    }

    /// Hits YouTube: `cargo test channel_page::tests::live -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live() {
        let http = crate::net::client();
        let v = innertube::post(&http, "browse", json!({ "browseId": "UCsXVk37bltHxD1rDPwtNM8Q" }), "channel").await.unwrap();
        let p = parse_header(&v);
        println!("{} {:?} {:?} {:?}
{:?}
{:?}", p.title, p.handle, p.subscribers, p.video_count, p.avatar, p.banner);
        assert!(!p.title.is_empty() && p.subscribers.is_some());
    }
}
