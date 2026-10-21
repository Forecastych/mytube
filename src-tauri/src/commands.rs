use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;
use tokio::sync::Semaphore;

use crate::db::{self, Channel, Video};
use crate::error::{Error, Result};
use crate::youtube::{channel, rss};

pub struct AppState {
    pub db: Mutex<Connection>,
    pub http: reqwest::Client,
}

/// RSS is cached by YouTube for 15 min anyway; refetching sooner is wasted traffic.
const RSS_TTL_SECS: i64 = 15 * 60;
/// Even a manual refresh doesn't refetch a channel more often than this.
const RSS_MIN_SECS: i64 = 60;
/// Parallel RSS requests — enough to be quick, few enough not to look like a bot.
const RSS_PARALLEL: usize = 6;
const FEED_LIMIT: u32 = 120;

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
    videos: Vec<Video>,
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

    let videos = db::favorites_feed(&*lock(&state)?, t, FEED_LIMIT)?;
    Ok(FavoritesFeed { videos, failed })
}
