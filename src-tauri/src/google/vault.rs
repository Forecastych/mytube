//! The user's own OAuth client and refresh token, kept in the OS credential
//! store (Windows Credential Manager), never in SQLite or a file.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const SERVICE: &str = "MyTube";
const CLIENT: &str = "google.client";
const REFRESH: &str = "google.refresh";

#[derive(Clone, Serialize, Deserialize)]
pub struct Client {
    pub id: String,
    pub secret: String,
}

fn entry(user: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, user).map_err(store_error)
}

fn store_error(e: keyring::Error) -> Error {
    Error::msg(format!("Credential store: {e}"))
}

fn read(user: &str) -> Result<Option<String>> {
    match entry(user)?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(store_error(e)),
    }
}

fn write(user: &str, value: &str) -> Result<()> {
    entry(user)?.set_password(value).map_err(store_error)
}

fn delete(user: &str) -> Result<()> {
    match entry(user)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(store_error(e)),
    }
}

pub fn client() -> Result<Option<Client>> {
    // Unreadable JSON means someone edited it by hand: same as not set.
    Ok(read(CLIENT)?.and_then(|s| serde_json::from_str(&s).ok()))
}

pub fn set_client(client: &Client) -> Result<()> {
    write(CLIENT, &serde_json::to_string(client).map_err(|e| Error::msg(e.to_string()))?)
}

pub fn clear_client() -> Result<()> {
    delete(CLIENT)
}

pub fn refresh_token() -> Result<Option<String>> {
    read(REFRESH)
}

pub fn set_refresh_token(token: &str) -> Result<()> {
    write(REFRESH, token)
}

pub fn clear_refresh_token() -> Result<()> {
    delete(REFRESH)
}
