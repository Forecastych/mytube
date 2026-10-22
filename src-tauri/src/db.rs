//! SQLite in the app data dir. One connection behind a mutex; queries are
//! tiny, so they run inline. Never hold the lock across an `.await`.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::Result;
use crate::youtube::rss::Feed;

/// A video counts as NEW for this long after publishing, until watched.
pub const NEW_WINDOW_SECS: i64 = 7 * 24 * 3600;

const MIGRATIONS: &[&str] = &[
    // v1
    "CREATE TABLE channels (
        id         TEXT PRIMARY KEY,
        title      TEXT NOT NULL,
        avatar     TEXT,
        favorite   INTEGER NOT NULL DEFAULT 0,
        position   INTEGER NOT NULL DEFAULT 0,
        fetched_at INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE videos (
        id         TEXT PRIMARY KEY,
        channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
        title      TEXT NOT NULL,
        published  INTEGER NOT NULL,
        views      INTEGER,
        is_short   INTEGER NOT NULL
    );
    CREATE INDEX videos_by_channel ON videos(channel_id, published DESC);
    CREATE INDEX videos_by_published ON videos(published DESC);
    CREATE TABLE watched (
        video_id   TEXT PRIMARY KEY,
        watched_at INTEGER NOT NULL
    );",
    // v2: feed rules (as JSON under a key) and Data API details for filters.
    "CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    ALTER TABLE videos ADD COLUMN duration INTEGER;
    ALTER TABLE videos ADD COLUMN category INTEGER;",
    // v3: channels imported from the user's YouTube subscriptions. A channel
    // can be a favorite, a subscription or both; it stays while either holds.
    "ALTER TABLE channels ADD COLUMN subscribed INTEGER NOT NULL DEFAULT 0;",
];

pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA temp_store = MEMORY;",
    )?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        conn.execute_batch(&format!("BEGIN; {sql} PRAGMA user_version = {}; COMMIT;", i + 1))?;
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub title: String,
    pub avatar: Option<String>,
    pub new_count: u32,
}

pub fn favorites(conn: &Connection, now: i64) -> Result<Vec<Channel>> {
    let mut stmt = conn.prepare_cached(
        "SELECT c.id, c.title, c.avatar,
                (SELECT COUNT(*) FROM videos v
                  WHERE v.channel_id = c.id AND v.is_short = 0 AND v.published > ?1
                    AND NOT EXISTS (SELECT 1 FROM watched w WHERE w.video_id = v.id))
           FROM channels c
          WHERE c.favorite = 1
          ORDER BY c.position",
    )?;
    let rows = stmt.query_map([now - NEW_WINDOW_SECS], |r| {
        Ok(Channel { id: r.get(0)?, title: r.get(1)?, avatar: r.get(2)?, new_count: r.get(3)? })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn is_favorite(conn: &Connection, id: &str) -> Result<bool> {
    let fav: Option<bool> = conn
        .query_row("SELECT favorite FROM channels WHERE id = ?1", [id], |r| r.get(0))
        .optional()?;
    Ok(fav == Some(true))
}

pub fn add_favorite(conn: &Connection, id: &str, title: &str, avatar: Option<&str>) -> Result<()> {
    conn.execute(
        "INSERT INTO channels (id, title, avatar, favorite, position)
         VALUES (?1, ?2, ?3, 1, (SELECT COALESCE(MAX(position), -1) + 1 FROM channels))
         ON CONFLICT(id) DO UPDATE SET
           title = excluded.title, avatar = excluded.avatar, favorite = 1",
        params![id, title, avatar],
    )?;
    Ok(())
}

pub fn remove_favorite(conn: &Connection, id: &str) -> Result<()> {
    // A subscription keeps its row and cached videos; otherwise the delete
    // cascades to the videos.
    conn.execute("UPDATE channels SET favorite = 0 WHERE id = ?1", [id])?;
    conn.execute("DELETE FROM channels WHERE id = ?1 AND subscribed = 0", [id])?;
    Ok(())
}

/// Favorites whose RSS was fetched before `older_than`.
pub fn stale_favorites(conn: &Connection, older_than: i64) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare_cached("SELECT id FROM channels WHERE favorite = 1 AND fetched_at < ?1")?;
    let rows = stmt.query_map([older_than], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn store_feed(conn: &mut Connection, channel_id: &str, feed: &Feed, now: i64) -> Result<()> {
    let tx = conn.transaction()?;
    {
        let mut upsert = tx.prepare_cached(
            "INSERT INTO videos (id, channel_id, title, published, views, is_short)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET title = excluded.title, views = excluded.views",
        )?;
        for e in &feed.entries {
            upsert.execute(params![e.id, channel_id, e.title, e.published, e.views, e.is_short])?;
        }
        tx.execute("UPDATE channels SET fetched_at = ?2 WHERE id = ?1", params![channel_id, now])?;
    }
    tx.commit()?;
    Ok(())
}

pub fn mark_watched(conn: &Connection, video_id: &str, now: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO watched (video_id, watched_at) VALUES (?1, ?2)
         ON CONFLICT(video_id) DO UPDATE SET watched_at = excluded.watched_at",
        params![video_id, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfavoriting_keeps_a_subscription() {
        let c = Connection::open_in_memory().unwrap();
        migrate(&c).unwrap();
        c.execute_batch(
            "INSERT INTO channels (id, title, favorite, subscribed) VALUES ('UCfav', 'F', 1, 0), ('UCboth', 'B', 1, 1);",
        )
        .unwrap();
        remove_favorite(&c, "UCfav").unwrap();
        remove_favorite(&c, "UCboth").unwrap();
        let left: Vec<(String, bool)> = c
            .prepare("SELECT id, favorite FROM channels")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(left, [("UCboth".to_string(), false)]);
    }
}
