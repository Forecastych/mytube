use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;
use tokio::sync::Semaphore;

use crate::db::{self, Channel, NEW_WINDOW_SECS};
use crate::error::{Error, Result};
use crate::feed::rank::{self, Candidate};
use crate::feed::store;
use crate::player::{self, sidx, sponsorblock, ytdlp, Player, WatchInfo};
use crate::youtube::{channel, comments, rss};

pub struct AppState {
    pub db: Mutex<Connection>,
    pub http: reqwest::Client,
    pub player: Player,
}

/// RSS is cached by YouTube for 15 min anyway; refetching sooner is wasted traffic.
const RSS_TTL_SECS: i64 = 15 * 60;
/// Even a manual refresh doesn't refetch a channel more often than this.
const RSS_MIN_SECS: i64 = 60;
/// Parallel RSS requests — enough to be quick, few enough not to look like a bot.
const RSS_PARALLEL: usize = 6;
const FEED_LIMIT: usize = 120;
/// Cached uploads the rules run over before the feed is cut to `FEED_LIMIT`.
const FEED_POOL: u32 = 1000;

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn lock(state: &AppState) -> Result<std::sync::MutexGuard<'_, Connection>> {
    state.db.lock().map_err(|_| Error::msg("Database lock poisoned"))
}

#[tauri::command]
pub fn favorites_list(state: State<'_, AppState>) -> Result<Vec<Channel>> {
    db::favorites(&*lock(&state)?, now())
}

#[tauri::command]
pub async fn favorite_add(state: State<'_, AppState>, input: String) -> Result<Channel> {
    let info = channel::resolve(&state.http, &input).await?;
    if db::is_favorite(&*lock(&state)?, &info.id)? {
        return Err(Error::msg(format!("{} is already in favorites", info.title)));
    }
    let feed = rss::fetch(&state.http, &info.id).await?;
    let title = if info.title.is_empty() { feed.title.clone() } else { info.title };

    let mut conn = lock(&state)?;
    db::add_favorite(&conn, &info.id, &title, info.avatar.as_deref())?;
    db::store_feed(&mut conn, &info.id, &feed, now())?;
    db::favorites(&conn, now())?
        .into_iter()
        .find(|c| c.id == info.id)
        .ok_or_else(|| Error::msg("Channel vanished after insert"))
}

#[tauri::command]
pub fn favorite_remove(state: State<'_, AppState>, id: String) -> Result<()> {
    db::remove_favorite(&*lock(&state)?, &id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoritesFeed {
    /// Newest first, after the Algorithm rules.
    videos: Vec<Candidate>,
    /// Dropped by the rules.
    hidden: u32,
    /// Channel ids whose RSS failed this time; their cached videos are still shown.
    failed: Vec<String>,
}

#[tauri::command]
pub async fn favorites_feed(state: State<'_, AppState>, force: bool) -> Result<FavoritesFeed> {
    let t = now();
    let ttl = if force { RSS_MIN_SECS } else { RSS_TTL_SECS };
    let stale = db::stale_favorites(&*lock(&state)?, t - ttl)?;

    let sem = Arc::new(Semaphore::new(RSS_PARALLEL));
    let tasks: Vec<_> = stale
        .into_iter()
        .map(|id| {
            let http = state.http.clone();
            let sem = sem.clone();
            tauri::async_runtime::spawn(async move {
                let _permit = sem.acquire_owned().await;
                let res = rss::fetch(&http, &id).await;
                (id, res)
            })
        })
        .collect();

    let mut failed = Vec::new();
    for task in tasks {
        let Ok((id, res)) = task.await else { continue };
        match res {
            Ok(feed) => db::store_feed(&mut *lock(&state)?, &id, &feed, t)?,
            Err(e) => {
                eprintln!("rss {id}: {e}");
                failed.push(id);
            }
        }
    }

    let conn = lock(&state)?;
    let rules = store::load_rules(&conn)?;
    let pool = store::favorite_candidates(&conn, t - NEW_WINDOW_SECS, FEED_POOL)?;
    let mut feed = rank::filter(pool, &rules);
    feed.videos.truncate(FEED_LIMIT);
    Ok(FavoritesFeed { videos: feed.videos, hidden: feed.hidden, failed })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Watch {
    #[serde(flatten)]
    info: Arc<WatchInfo>,
    is_favorite: bool,
}

/// Runs yt-dlp (only when the user opens a video, never to prefetch).
#[tauri::command]
pub async fn watch_info(state: State<'_, AppState>, id: String, force: bool) -> Result<Watch> {
    let info = state.player.info(&id, force).await?;
    let is_favorite = match &info.channel_id {
        Some(c) => db::is_favorite(&*lock(&state)?, c)?,
        None => false,
    };
    Ok(Watch { info, is_favorite })
}

#[tauri::command]
pub async fn stream_index(state: State<'_, AppState>, key: String) -> Result<sidx::Index> {
    state.player.index(&state.http, &key).await
}

#[tauri::command]
pub async fn sponsor_segments(state: State<'_, AppState>, id: String) -> Result<Vec<sponsorblock::Segment>> {
    if !player::is_video_id(&id) {
        return Err(Error::msg("Bad video id"));
    }
    sponsorblock::segments(&state.http, &id).await
}

#[tauri::command]
pub fn open_in_mpv(state: State<'_, AppState>, id: String, title: String, start: f64) -> Result<()> {
    player::open_in_mpv(&state.player, &id, &title, start)
}

#[tauri::command]
pub async fn ytdlp_version() -> Result<String> {
    ytdlp::version().await
}

#[tauri::command]
pub async fn ytdlp_update() -> Result<String> {
    ytdlp::update().await
}

#[tauri::command]
pub async fn comments(state: State<'_, AppState>, id: String) -> Result<comments::Page> {
    if !player::is_video_id(&id) {
        return Err(Error::msg("Bad video id"));
    }
    comments::first(&state.http, &id).await
}

/// Next page, another sort order, or a thread's replies.
#[tauri::command]
pub async fn comments_more(state: State<'_, AppState>, token: String) -> Result<comments::Page> {
    comments::more(&state.http, &token).await
}

#[tauri::command]
pub fn mark_watched(state: State<'_, AppState>, id: String) -> Result<()> {
    if !player::is_video_id(&id) {
        return Err(Error::msg("Bad video id"));
    }
    db::mark_watched(&*lock(&state)?, &id, now())
}
