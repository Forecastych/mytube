//! Tauri commands for the Algorithm screen.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::State;

use super::rank::{self, Ranked};
use super::rules::{BlockedChannel, Mix, Rules};
use super::store;
use super::topics::{self, Topic};
use crate::commands::AppState;
use crate::db::{self, NEW_WINDOW_SECS};
use crate::error::{Error, Result};
use crate::youtube::{channel, rss};

const PREVIEW_LEN: usize = 5;
/// Cached favorite uploads the preview ranks: all of them, in practice.
const PREVIEW_POOL: u32 = 1000;

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn lock(state: &AppState) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>> {
    state.db.lock().map_err(|_| Error::msg("Database lock poisoned"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlgorithmView {
    rules: Rules,
    topics: &'static [Topic],
    preview: Ranked,
}

/// Only favorites are cached locally, so the preview ranks them alone: it
/// shows what the filters do, the mix shows up on Home.
fn view(conn: &rusqlite::Connection, rules: Rules) -> Result<AlgorithmView> {
    let pool = store::favorite_candidates(conn, now() - NEW_WINDOW_SECS, PREVIEW_POOL)?;
    let only_favorites = Rules { mix: Mix { favorites: 100, subscriptions: 0, trending: 0, related: 0 }, ..rules.clone() };
    let preview = rank::rank(pool, &only_favorites, PREVIEW_LEN);
    Ok(AlgorithmView { rules, topics: topics::TOPICS, preview })
}

#[tauri::command]
pub fn algorithm_get(state: State<'_, AppState>) -> Result<AlgorithmView> {
    let conn = lock(&state)?;
    let rules = store::load_rules(&conn)?;
    view(&conn, rules)
}

#[tauri::command]
pub fn algorithm_set(state: State<'_, AppState>, rules: Rules) -> Result<AlgorithmView> {
    let conn = lock(&state)?;
    let rules = store::save_rules(&conn, rules)?;
    view(&conn, rules)
}

/// Turns a pasted link into a channel to block. Doesn't save: the screen
/// adds it to the rules and saves them as usual.
#[tauri::command]
pub async fn blocked_channel_resolve(state: State<'_, AppState>, input: String) -> Result<BlockedChannel> {
    let info = channel::resolve(&state.http, &input).await?;
    if db::is_favorite(&*lock(&state)?, &info.id)? {
        return Err(Error::msg(format!("{} is in your favorites. Remove it there first.", info.title)));
    }
    let title = if info.title.is_empty() {
        // Same fallback as adding a favorite: RSS always has the name.
        rss::fetch(&state.http, &info.id).await.map(|f| f.title).unwrap_or_else(|_| info.id.clone())
    } else {
        info.title
    };
    Ok(BlockedChannel { id: info.id, title, avatar: info.avatar })
}
