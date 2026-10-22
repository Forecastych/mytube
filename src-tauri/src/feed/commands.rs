//! Tauri commands for the Algorithm screen.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::State;

use super::rank::{self, Candidate, Ranked};
use super::rules::{BlockedChannel, Mix, Rules, Source};
use super::store;
use super::topics::{self, Topic};
use crate::commands::{refresh_rss, AppState};
use crate::db::{self, NEW_WINDOW_SECS};
use crate::error::{Error, Result};
use crate::google::{self, subscriptions, trending, vault, GoogleState};
use crate::youtube::{channel, related, rss};

const PREVIEW_LEN: usize = 5;
const HOME_LIMIT: usize = 120;
/// Cached uploads per source that Home ranks.
const HOME_POOL: u32 = 600;
/// Subscriptions can run into hundreds of channels: their RSS refreshes less
/// often than favorites, and only this many per Home load, stalest first.
const SUB_TTL_SECS: i64 = 60 * 60;
const SUB_REFRESH: u32 = 24;
/// Cached favorite uploads the preview ranks: all of them, in practice.
const PREVIEW_POOL: u32 = 1000;
/// Related lists Home asks for: each is one Innertube call, cached for an hour.
const RELATED_SEEDS: usize = 4;

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeFeed {
    videos: Vec<Candidate>,
    /// Dropped by the rules.
    hidden: u32,
    signed_in: bool,
    /// Subscribed channels imported so far.
    subscriptions: u32,
    /// Subscription channels whose RSS failed this time.
    failed: Vec<String>,
    /// A source that couldn't load (sign-in expired, quota): the rest still shows.
    notice: Option<String>,
}

/// Videos whose related lists feed the Related source: the latest favorite
/// uploads that pass the rules, or subscription uploads without favorites.
fn related_seeds(conn: &rusqlite::Connection, rules: &Rules, new_since: i64) -> Result<Vec<String>> {
    for list in [store::favorite_candidates(conn, new_since, 40)?, store::subscription_candidates(conn, new_since, 40)?] {
        let seeds: Vec<String> = rank::filter(list, rules).videos.into_iter().take(RELATED_SEEDS).map(|c| c.id).collect();
        if !seeds.is_empty() {
            return Ok(seeds);
        }
    }
    Ok(Vec::new())
}

/// The mixed feed under the chips on Home. `source` narrows it to one
/// source; `region` is the Trending fallback until that screen is opened.
#[tauri::command]
pub async fn home_feed(
    state: State<'_, AppState>,
    google: State<'_, GoogleState>,
    source: Option<Source>,
    region: Option<String>,
) -> Result<HomeFeed> {
    let t = now();
    let signed_in = vault::refresh_token()?.is_some();
    let mut notice = None;
    let wants = |s: Source| source.is_none_or(|x| x == s);

    if signed_in && wants(Source::Subscriptions) && t - subscriptions::synced_at(&*lock(&state)?)? > subscriptions::SYNC_TTL_SECS {
        if let Err(e) = google::commands::sync_subscriptions(&state, &google).await {
            notice = Some(format!("Couldn't update subscriptions: {e}"));
        }
    }
    let failed = if wants(Source::Subscriptions) {
        let stale = subscriptions::stale(&*lock(&state)?, t - SUB_TTL_SECS, SUB_REFRESH)?;
        refresh_rss(&state, stale, t).await?
    } else {
        Vec::new()
    };

    let rules = store::load_rules(&*lock(&state)?)?;
    let mut pool = Vec::new();
    if signed_in && wants(Source::Trending) && (source.is_some() || rules.mix.trending > 0) {
        let saved = trending::saved_region(&*lock(&state)?)?;
        let region = saved.or(region.filter(|r| trending::valid_region(r))).unwrap_or_else(|| "US".into());
        match google.chart(&state.http, &region, None).await {
            Ok(chart) => pool.extend(chart),
            Err(e) => notice = notice.or(Some(format!("Couldn't load Trending: {e}"))),
        }
    }

    let new_since = t - NEW_WINDOW_SECS;
    if wants(Source::Related) && (source.is_some() || rules.mix.related > 0) {
        let seeds = related_seeds(&*lock(&state)?, &rules, new_since)?;
        let (videos, failed) = related::for_seeds(&state.http, &seeds).await;
        if failed > 0 && videos.is_empty() {
            notice = notice.or(Some("Couldn't load related videos".into()));
        }
        pool.extend(videos);
    }

    let conn = lock(&state)?;
    if wants(Source::Favorites) {
        pool.extend(store::favorite_candidates(&conn, new_since, HOME_POOL)?);
    }
    if wants(Source::Subscriptions) {
        pool.extend(store::subscription_candidates(&conn, new_since, HOME_POOL)?);
    }
    let rules = match source {
        // One source: all of the feed is that source.
        Some(s) => {
            let only = |x: Source| if x == s { 100 } else { 0 };
            let mix = Mix {
                favorites: only(Source::Favorites),
                subscriptions: only(Source::Subscriptions),
                trending: only(Source::Trending),
                related: only(Source::Related),
            };
            Rules { mix, ..rules }
        }
        None => rules,
    };
    let ranked = rank::rank(pool, &rules, HOME_LIMIT);
    Ok(HomeFeed {
        videos: ranked.videos,
        hidden: ranked.hidden,
        signed_in,
        subscriptions: subscriptions::count(&conn)?,
        failed,
        notice,
    })
}
