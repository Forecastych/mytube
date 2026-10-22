//! The user's YouTube subscriptions. The list comes from the Data API (1 unit
//! per 50 channels); their uploads then come from RSS, like favorites, so the
//! feed keeps working after sign-out.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;

use super::api;
use crate::error::Result;
use crate::youtube::channel::is_channel_id;

/// 50 per page: up to 2500 channels, more than YouTube lets anyone subscribe to.
const MAX_PAGES: usize = 50;
const SYNCED_KEY: &str = "google.subscriptions_synced";
/// The list rarely changes; once a day is plenty, the Import button forces it.
pub const SYNC_TTL_SECS: i64 = 24 * 3600;

pub struct Sub {
    pub id: String,
    pub title: String,
    pub avatar: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    #[serde(default)]
    items: Vec<Item>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct Item {
    snippet: Snippet,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snippet {
    title: String,
    resource_id: ResourceId,
    #[serde(default)]
    thumbnails: Thumbs,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceId {
    #[serde(default)]
    channel_id: String,
}

#[derive(Deserialize, Default)]
struct Thumbs {
    default: Option<Thumb>,
}

#[derive(Deserialize)]
struct Thumb {
    url: String,
}

pub async fn fetch(http: &reqwest::Client, token: &str) -> Result<Vec<Sub>> {
    let mut subs = Vec::new();
    let mut page_token: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let mut path = "subscriptions?part=snippet&mine=true&maxResults=50".to_string();
        if let Some(t) = &page_token {
            path.push_str("&pageToken=");
            path.push_str(&super::oauth::encode(t));
        }
        let page: Page = api::get(http, token, &path).await?;
        subs.extend(page.items.into_iter().filter(|i| is_channel_id(&i.snippet.resource_id.channel_id)).map(|i| Sub {
            id: i.snippet.resource_id.channel_id,
            title: i.snippet.title,
            avatar: i.snippet.thumbnails.default.map(|t| t.url),
        }));
        page_token = page.next_page_token;
        if page_token.is_none() {
            break;
        }
    }
    Ok(subs)
}

/// Replaces the subscribed set. Unsubscribed channels go, with their cached
/// videos, unless they are favorites.
pub fn store(conn: &mut Connection, subs: &[Sub], now: i64) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute("UPDATE channels SET subscribed = 0 WHERE subscribed = 1", [])?;
    {
        let mut upsert = tx.prepare_cached(
            "INSERT INTO channels (id, title, avatar, subscribed) VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(id) DO UPDATE SET title = excluded.title, avatar = excluded.avatar, subscribed = 1",
        )?;
        for s in subs {
            upsert.execute(params![s.id, s.title, s.avatar])?;
        }
    }
    tx.execute("DELETE FROM channels WHERE subscribed = 0 AND favorite = 0", [])?;
    tx.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![SYNCED_KEY, now.to_string()],
    )?;
    tx.commit()?;
    Ok(())
}

/// When the list was last imported; 0 if never.
pub fn synced_at(conn: &Connection) -> Result<i64> {
    let v: Option<String> =
        conn.query_row("SELECT value FROM settings WHERE key = ?1", [SYNCED_KEY], |r| r.get(0)).optional()?;
    Ok(v.and_then(|s| s.parse().ok()).unwrap_or(0))
}

pub fn count(conn: &Connection) -> Result<u32> {
    Ok(conn.query_row("SELECT COUNT(*) FROM channels WHERE subscribed = 1", [], |r| r.get(0))?)
}

/// Subscriptions (not favorites, those refresh on their own) whose RSS is
/// older than `older_than`, stalest first, at most `limit`.
pub fn stale(conn: &Connection, older_than: i64, limit: u32) -> Result<Vec<String>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id FROM channels WHERE subscribed = 1 AND favorite = 0 AND fetched_at < ?1
          ORDER BY fetched_at LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![older_than, limit], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE channels (id TEXT PRIMARY KEY, title TEXT NOT NULL, avatar TEXT,
                 favorite INTEGER NOT NULL DEFAULT 0, position INTEGER NOT NULL DEFAULT 0,
                 fetched_at INTEGER NOT NULL DEFAULT 0, subscribed INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE videos (id TEXT PRIMARY KEY,
                 channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE);
             CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .unwrap();
        c
    }

    fn sub(id: &str) -> Sub {
        Sub { id: id.into(), title: id.into(), avatar: None }
    }

    #[test]
    fn store_replaces_the_set_and_keeps_favorites() {
        let mut c = conn();
        c.execute_batch(
            "INSERT INTO channels (id, title, favorite, subscribed) VALUES
                 ('fav', 'F', 1, 1), ('gone', 'G', 0, 1), ('stay', 'S', 0, 1);
             INSERT INTO videos VALUES ('v1', 'gone'), ('v2', 'fav');",
        )
        .unwrap();
        store(&mut c, &[sub("stay"), sub("new")], 500).unwrap();

        let rows: Vec<(String, bool, bool)> = c
            .prepare("SELECT id, favorite, subscribed FROM channels ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let got: Vec<_> = rows.iter().map(|(id, f, s)| (id.as_str(), *f, *s)).collect();
        assert_eq!(got, [("fav", true, false), ("new", false, true), ("stay", false, true)]);
        let videos: u32 = c.query_row("SELECT COUNT(*) FROM videos", [], |r| r.get(0)).unwrap();
        assert_eq!(videos, 1, "the unsubscribed channel's videos go with it");
        assert_eq!(synced_at(&c).unwrap(), 500);
        assert_eq!(count(&c).unwrap(), 2);
    }

    #[test]
    fn stale_skips_favorites_and_fresh() {
        let c = conn();
        c.execute_batch(
            "INSERT INTO channels (id, title, favorite, subscribed, fetched_at) VALUES
                 ('a', 'A', 0, 1, 10), ('b', 'B', 0, 1, 5), ('fav', 'F', 1, 1, 0), ('fresh', 'R', 0, 1, 900);",
        )
        .unwrap();
        assert_eq!(stale(&c, 100, 10).unwrap(), ["b", "a"]);
        assert_eq!(stale(&c, 100, 1).unwrap(), ["b"]);
    }
}
