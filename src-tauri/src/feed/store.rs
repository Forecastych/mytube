//! Rules and feed candidates in SQLite. Tables come from schema v2 in
//! `db::MIGRATIONS` (`settings`, `videos.duration`, `videos.category`).

use rusqlite::{params, Connection, OptionalExtension};

use super::rank::Candidate;
use super::rules::{Rules, Source};
use crate::error::{Error, Result};

const RULES_KEY: &str = "algorithm.rules";

pub fn load_rules(conn: &Connection) -> Result<Rules> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [RULES_KEY], |r| r.get(0))
        .optional()?;
    let Some(raw) = raw else { return Ok(Rules::default()) };
    // A broken save must not take Home down with it: fall back to defaults.
    let mut rules = serde_json::from_str::<Rules>(&raw).unwrap_or_else(|e| {
        eprintln!("feed rules: {e}, using defaults");
        Rules::default()
    });
    rules.sanitize();
    Ok(rules)
}

/// Sanitizes, stores and returns what was stored.
pub fn save_rules(conn: &Connection, mut rules: Rules) -> Result<Rules> {
    rules.sanitize();
    let json = serde_json::to_string(&rules).map_err(|e| Error::msg(e.to_string()))?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![RULES_KEY, json],
    )?;
    Ok(rules)
}

/// Cached uploads of favorite channels, newest first, before any filtering.
/// A video is NEW if published after `new_since` and not watched.
pub fn favorite_candidates(conn: &Connection, new_since: i64, limit: u32) -> Result<Vec<Candidate>> {
    let mut stmt = conn.prepare_cached(
        "SELECT v.id, v.channel_id, c.title, c.avatar, v.title, v.published, v.views,
                v.duration, v.category, v.is_short, w.video_id IS NOT NULL, c.position
           FROM videos v
           JOIN channels c ON c.id = v.channel_id
           LEFT JOIN watched w ON w.video_id = v.id
          WHERE c.favorite = 1
          ORDER BY v.published DESC
          LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |r| {
        let published: i64 = r.get(5)?;
        let watched: bool = r.get(10)?;
        Ok(Candidate {
            id: r.get(0)?,
            channel_id: r.get(1)?,
            channel_title: r.get(2)?,
            channel_avatar: r.get(3)?,
            title: r.get(4)?,
            published,
            views: r.get(6)?,
            duration: r.get(7)?,
            category: r.get(8)?,
            is_short: r.get(9)?,
            is_new: published > new_since && !watched,
            source: Source::Favorites,
            watched,
            channel_rank: r.get(11)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v1 + v2 as `db::MIGRATIONS` will have them.
    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE channels (id TEXT PRIMARY KEY, title TEXT NOT NULL, avatar TEXT,
                 favorite INTEGER NOT NULL DEFAULT 0, position INTEGER NOT NULL DEFAULT 0,
                 fetched_at INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE videos (id TEXT PRIMARY KEY, channel_id TEXT NOT NULL, title TEXT NOT NULL,
                 published INTEGER NOT NULL, views INTEGER, is_short INTEGER NOT NULL);
             CREATE TABLE watched (video_id TEXT PRIMARY KEY, watched_at INTEGER NOT NULL);
             CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             ALTER TABLE videos ADD COLUMN duration INTEGER;
             ALTER TABLE videos ADD COLUMN category INTEGER;",
        )
        .unwrap();
        c
    }

    #[test]
    fn rules_roundtrip_and_default() {
        let c = conn();
        assert_eq!(load_rules(&c).unwrap(), Rules::default());
        let saved = save_rules(&c, Rules { blocked_words: vec![" VLOG ".into()], ..Rules::default() }).unwrap();
        assert_eq!(saved.blocked_words, ["vlog"]);
        assert_eq!(load_rules(&c).unwrap(), saved);
    }

    #[test]
    fn candidates_carry_watched_and_new() {
        let c = conn();
        c.execute_batch(
            "INSERT INTO channels (id, title, favorite, position) VALUES ('UCa', 'A', 1, 3), ('UCb', 'B', 0, 0);
             INSERT INTO videos (id, channel_id, title, published, is_short) VALUES
                 ('new', 'UCa', 'n', 200, 0), ('seen', 'UCa', 's', 150, 0),
                 ('old', 'UCa', 'o', 50, 0), ('other', 'UCb', 'x', 300, 0);
             INSERT INTO watched VALUES ('seen', 1);",
        )
        .unwrap();
        let v = favorite_candidates(&c, 100, 50).unwrap();
        let got: Vec<_> = v.iter().map(|c| (c.id.as_str(), c.is_new, c.watched)).collect();
        assert_eq!(got, [("new", true, false), ("seen", false, true), ("old", false, false)]);
        assert_eq!(v[0].channel_rank, Some(3));
    }
}
