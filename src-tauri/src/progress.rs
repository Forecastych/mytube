//! Where each video was left off: "continue from 12:34" on Watch and the thin
//! bar on cards. Local only; it never changes what the feed recommends.

use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::State;

use crate::commands::AppState;
use crate::error::{Error, Result};
use crate::player::is_video_id;

/// Run by `db::migrate`.
pub const MIGRATION: &str = "CREATE TABLE progress (
        video_id   TEXT PRIMARY KEY,
        position   REAL NOT NULL,
        duration   REAL NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX progress_by_time ON progress(updated_at DESC);";

/// Below this nothing is remembered: a peek is not "started".
const MIN_POSITION: f64 = 15.0;
/// This close to the end counts as finished (end screens, outros).
const END_MARGIN: f64 = 30.0;
/// Older entries are dropped; cards only ever need recent ones.
const KEEP: i64 = 2000;

#[derive(Serialize, Debug, PartialEq)]
pub struct Progress {
    pub id: String,
    pub position: f64,
    pub duration: f64,
}

pub fn save(conn: &Connection, id: &str, position: f64, duration: f64, now: i64) -> Result<()> {
    let finished = duration > 0.0 && (position > duration - END_MARGIN || position > duration * 0.95);
    if !position.is_finite() || !duration.is_finite() || position < MIN_POSITION || finished {
        conn.execute("DELETE FROM progress WHERE video_id = ?1", [id])?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO progress (video_id, position, duration, updated_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(video_id) DO UPDATE SET position = excluded.position,
             duration = excluded.duration, updated_at = excluded.updated_at",
        params![id, position, duration, now],
    )?;
    Ok(())
}

pub fn list(conn: &Connection) -> Result<Vec<Progress>> {
    conn.execute(
        "DELETE FROM progress WHERE video_id NOT IN
           (SELECT video_id FROM progress ORDER BY updated_at DESC LIMIT ?1)",
        [KEEP],
    )?;
    let mut st = conn.prepare_cached("SELECT video_id, position, duration FROM progress")?;
    let rows = st.query_map([], |r| Ok(Progress { id: r.get(0)?, position: r.get(1)?, duration: r.get(2)? }))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn db(state: &AppState) -> Result<std::sync::MutexGuard<'_, Connection>> {
    state.db.lock().map_err(|_| Error::msg("Database lock poisoned"))
}

#[tauri::command]
pub fn save_progress(state: State<'_, AppState>, id: String, position: f64, duration: f64) -> Result<()> {
    if !is_video_id(&id) {
        return Err(Error::msg("Bad video id"));
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    save(&*db(&state)?, &id, position, duration, now)
}

#[tauri::command]
pub fn progress_list(state: State<'_, AppState>) -> Result<Vec<Progress>> {
    list(&*db(&state)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(MIGRATION).unwrap();
        c
    }

    #[test]
    fn keeps_only_the_middle() {
        let c = conn();
        save(&c, "a", 5.0, 600.0, 1).unwrap();
        save(&c, "b", 120.0, 600.0, 1).unwrap();
        save(&c, "c", 590.0, 600.0, 1).unwrap();
        save(&c, "d", f64::NAN, 600.0, 1).unwrap();
        assert_eq!(list(&c).unwrap(), [Progress { id: "b".into(), position: 120.0, duration: 600.0 }]);

        // Watching to the end forgets it.
        save(&c, "b", 580.0, 600.0, 2).unwrap();
        assert!(list(&c).unwrap().is_empty());
    }

    #[test]
    fn updates_in_place() {
        let c = conn();
        save(&c, "a", 60.0, 600.0, 1).unwrap();
        save(&c, "a", 90.0, 600.0, 2).unwrap();
        assert_eq!(list(&c).unwrap()[0].position, 90.0);
    }
}
