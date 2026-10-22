//! Tauri commands for Settings → Account.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;
use tauri_plugin_opener::OpenerExt;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use super::vault::{self, Client};
use super::{api, oauth, subscriptions, trending, GoogleState};
use crate::feed::rank::{self, Ranked};
use crate::feed::store;
use crate::commands::AppState;
use crate::error::{Error, Result};

const ACCOUNT_KEY: &str = "google.account";

/// Who is signed in, for the Account card. Not secret: lives in SQLite.
#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    title: String,
    avatar: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleStatus {
    /// The Client ID is shown back so the user sees which project is in use.
    /// The secret never leaves Rust.
    client_id: Option<String>,
    account: Option<Account>,
}

fn lock(state: &AppState) -> Result<std::sync::MutexGuard<'_, Connection>> {
    state.db.lock().map_err(|_| Error::msg("Database lock poisoned"))
}

fn load_account(conn: &Connection) -> Result<Option<Account>> {
    let json: Option<String> =
        conn.query_row("SELECT value FROM settings WHERE key = ?1", [ACCOUNT_KEY], |r| r.get(0)).optional()?;
    Ok(json.and_then(|s| serde_json::from_str(&s).ok()))
}

fn save_account(conn: &Connection, account: Option<&Account>) -> Result<()> {
    match account {
        Some(a) => conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![ACCOUNT_KEY, serde_json::to_string(a).map_err(|e| Error::msg(e.to_string()))?],
        )?,
        None => conn.execute("DELETE FROM settings WHERE key = ?1", [ACCOUNT_KEY])?,
    };
    Ok(())
}

fn status(state: &AppState) -> Result<GoogleStatus> {
    let client_id = vault::client()?.map(|c| c.id);
    // The refresh token is the truth: a cached name without it is stale.
    let account = if vault::refresh_token()?.is_some() { load_account(&*lock(state)?)? } else { None };
    Ok(GoogleStatus { client_id, account })
}

#[tauri::command]
pub fn google_status(state: State<'_, AppState>) -> Result<GoogleStatus> {
    status(&state)
}

/// Desktop-app clients look like `<digits>-<hash>.apps.googleusercontent.com`.
fn valid_client_id(id: &str) -> bool {
    id.strip_suffix(".apps.googleusercontent.com")
        .is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
}

#[tauri::command]
pub async fn google_set_client(
    state: State<'_, AppState>,
    google: State<'_, GoogleState>,
    client_id: String,
    client_secret: String,
) -> Result<GoogleStatus> {
    let (id, secret) = (client_id.trim(), client_secret.trim());
    if !valid_client_id(id) {
        return Err(Error::msg("That isn't a Client ID: it ends with .apps.googleusercontent.com"));
    }
    if secret.is_empty() || secret.len() > 200 || secret.contains(char::is_whitespace) {
        return Err(Error::msg("Paste the Client secret from the same OAuth client."));
    }
    // A refresh token belongs to the client that issued it.
    sign_out_local(&state, &google).await?;
    vault::set_client(&Client { id: id.into(), secret: secret.into() })?;
    status(&state)
}

async fn sign_out_local(state: &AppState, google: &GoogleState) -> Result<()> {
    google.forget().await;
    vault::clear_refresh_token()?;
    save_account(&*lock(state)?, None)
}

#[tauri::command]
pub async fn google_sign_in(state: State<'_, AppState>, google: State<'_, GoogleState>, app: tauri::AppHandle) -> Result<GoogleStatus> {
    let client = vault::client()?.ok_or_else(|| Error::msg("Paste your Client ID and secret first."))?;
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(|e| Error::msg(format!("Sign-in listener: {e}")))?;
    let port = listener.local_addr().map_err(|e| Error::msg(e.to_string()))?.port();
    let attempt = oauth::Attempt::new(port)?;

    let (tx, rx) = oneshot::channel();
    if let Ok(mut c) = google.cancel.lock() {
        // A second click replaces the first attempt.
        if let Some(old) = c.replace(tx) {
            let _ = old.send(());
        }
    }
    app.opener().open_url(attempt.url(&client), None::<&str>).map_err(|e| Error::msg(format!("Couldn't open the browser: {e}")))?;

    let code = tokio::select! {
        r = oauth::wait_for_code(listener, &attempt) => r,
        _ = tokio::time::sleep(oauth::WAIT) => Err(Error::msg("Sign-in timed out. Try again.")),
        _ = rx => Err(Error::msg("Sign-in cancelled.")),
    };
    if let Ok(mut c) = google.cancel.lock() {
        c.take();
    }
    let tokens = oauth::exchange(&state.http, &client, &attempt, &code?).await?;

    if !tokens.scope.split(' ').any(|s| s == oauth::SCOPE) {
        oauth::revoke(&state.http, &tokens.access_token).await;
        return Err(Error::msg("MyTube needs read access to YouTube. Sign in again and tick the YouTube box."));
    }
    let refresh = tokens.refresh_token.ok_or_else(|| Error::msg("Google sent no refresh token. Sign in again."))?;
    vault::set_refresh_token(&refresh)?;
    google.remember(tokens.access_token.clone(), tokens.expires_in).await;

    let account = match api::my_channel(&state.http, &tokens.access_token).await? {
        Some((title, avatar)) => Account { title, avatar },
        None => Account { title: "Google account without a YouTube channel".into(), avatar: None },
    };
    save_account(&*lock(&state)?, Some(&account))?;
    status(&state)
}

#[tauri::command]
pub fn google_sign_in_cancel(google: State<'_, GoogleState>) {
    if let Some(tx) = google.cancel.lock().ok().and_then(|mut c| c.take()) {
        let _ = tx.send(());
    }
}

#[tauri::command]
pub async fn google_sign_out(state: State<'_, AppState>, google: State<'_, GoogleState>) -> Result<GoogleStatus> {
    if let Some(token) = vault::refresh_token()? {
        // Revoking the refresh token also kills its access tokens.
        oauth::revoke(&state.http, &token).await;
    }
    sign_out_local(&state, &google).await?;
    status(&state)
}

/// Signs out and removes the Client ID and secret from this computer.
#[tauri::command]
pub async fn google_forget_client(state: State<'_, AppState>, google: State<'_, GoogleState>) -> Result<GoogleStatus> {
    google_sign_out(state.clone(), google.clone()).await?;
    vault::clear_client()?;
    status(&state)
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Imports the subscription list (not the videos: those come from RSS on
/// Home). Returns how many channels the user is subscribed to.
pub async fn sync_subscriptions(state: &AppState, google: &GoogleState) -> Result<u32> {
    let token = google.access_token(&state.http).await?;
    let subs = subscriptions::fetch(&state.http, &token).await?;
    let mut conn = lock(state)?;
    subscriptions::store(&mut conn, &subs, now())?;
    subscriptions::count(&conn)
}

#[tauri::command]
pub async fn subscriptions_sync(state: State<'_, AppState>, google: State<'_, GoogleState>) -> Result<u32> {
    sync_subscriptions(&state, &google).await
}

/// A chart after the Algorithm rules, in chart order.
#[tauri::command]
pub async fn trending(
    state: State<'_, AppState>,
    google: State<'_, GoogleState>,
    region: String,
    category: Option<u32>,
) -> Result<Ranked> {
    if !trending::valid_region(&region) {
        return Err(Error::msg("Bad region"));
    }
    trending::check_category(category)?;
    let chart = google.chart(&state.http, &region, category).await?;
    let conn = lock(&state)?;
    trending::save_region(&conn, &region)?;
    let rules = store::load_rules(&conn)?;
    Ok(rank::filter(chart, &rules))
}

#[cfg(test)]
mod tests {
    use super::valid_client_id;

    #[test]
    fn client_id_shape() {
        assert!(valid_client_id("123456-abc123def.apps.googleusercontent.com"));
        assert!(!valid_client_id(".apps.googleusercontent.com"));
        assert!(!valid_client_id("GOCSPX-secret"));
        assert!(!valid_client_id("123 456.apps.googleusercontent.com"));
    }
}
