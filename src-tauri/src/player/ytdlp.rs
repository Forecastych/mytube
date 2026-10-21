//! Stream URLs come from yt-dlp: it is the first thing fixed whenever YouTube
//! changes its protection. It always runs anonymously: no cookies, no user
//! config (a user's config could add `--cookies-from-browser`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use tokio::process::Command;

use crate::error::{Error, Result};

const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Info {
    pub id: String,
    pub title: String,
    pub channel: Option<String>,
    pub uploader: Option<String>,
    pub channel_id: Option<String>,
    pub description: Option<String>,
    pub duration: Option<f64>,
    pub view_count: Option<i64>,
    pub like_count: Option<i64>,
    pub timestamp: Option<i64>,
    pub upload_date: Option<String>,
    pub is_live: Option<bool>,
    pub chapters: Option<Vec<Chapter>>,
    pub formats: Vec<Format>,
}

#[derive(Deserialize, Clone, serde::Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct Chapter {
    pub start_time: f64,
    pub end_time: f64,
    pub title: String,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Format {
    pub format_id: String,
    pub url: Option<String>,
    pub protocol: Option<String>,
    pub ext: Option<String>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub tbr: Option<f64>,
    pub dynamic_range: Option<String>,
    pub language_preference: Option<i64>,
    pub has_drm: Option<bool>,
    pub http_headers: HashMap<String, String>,
}

impl Format {
    fn codec(c: &Option<String>) -> &str {
        match c.as_deref() {
            None | Some("none") => "",
            Some(s) => s,
        }
    }
    pub fn vcodec(&self) -> &str {
        Self::codec(&self.vcodec)
    }
    pub fn acodec(&self) -> &str {
        Self::codec(&self.acodec)
    }
    /// A plain HTTPS URL we can proxy with Range requests.
    pub fn direct(&self) -> bool {
        self.url.is_some() && self.protocol.as_deref() == Some("https") && self.has_drm != Some(true)
    }
    pub fn sdr(&self) -> bool {
        matches!(self.dynamic_range.as_deref(), None | Some("SDR"))
    }
}

/// The bundled sidecar sits next to our exe; in dev, fall back to PATH.
fn binary() -> PathBuf {
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)) {
        let p = dir.join("yt-dlp.exe");
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from("yt-dlp")
}

pub async fn fetch(video_id: &str) -> Result<Info> {
    let mut cmd = Command::new(binary());
    cmd.args(["--ignore-config", "--no-cookies", "--no-cookies-from-browser"])
        .args(["--no-warnings", "--no-playlist", "--dump-json"])
        .arg("--")
        .arg(format!("https://www.youtube.com/watch?v={video_id}"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash

    let child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::msg("yt-dlp is missing. Reinstall MyTube or put yt-dlp on PATH.")
        } else {
            Error::msg(format!("Couldn't start yt-dlp: {e}"))
        }
    })?;
    let out = tokio::time::timeout(TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| Error::msg("YouTube didn't answer in time. Check your connection and try again."))?
        .map_err(|e| Error::msg(format!("yt-dlp failed: {e}")))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        eprintln!("yt-dlp {video_id}: {}", stderr.trim());
        return Err(Error::msg(human_error(&stderr)));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| Error::msg(format!("Unexpected yt-dlp output: {e}")))
}

/// yt-dlp errors are written for developers; turn the common ones into
/// something the user can act on.
fn human_error(stderr: &str) -> String {
    let line = stderr.lines().rev().find(|l| l.starts_with("ERROR:")).unwrap_or(stderr).trim();
    let has = |s: &str| line.contains(s);
    if has("not a bot") {
        "YouTube didn't give the video to this connection and asked to sign in. This happens on VPN and \
         datacenter IPs: open MyTube from your home connection."
            .into()
    } else if has("confirm your age") || has("age-restricted") {
        "This video is age-restricted. MyTube plays anonymously, so it can't open it.".into()
    } else if has("Private video") {
        "This video is private.".into()
    } else if has("members-only") || has("Join this channel") {
        "This video is for channel members only.".into()
    } else if has("Premieres in") || has("will begin in") || has("live event will begin") {
        "This video hasn't started yet.".into()
    } else if has("HTTP Error 429") {
        "YouTube is rate-limiting this connection. Try again in a few minutes.".into()
    } else if has("timed out") || has("Unable to download") || has("getaddrinfo") || has("connection") {
        "Couldn't reach YouTube. Check your connection and try again.".into()
    } else if has("Video unavailable") || has("not available") {
        "This video is unavailable.".into()
    } else {
        line.trim_start_matches("ERROR:").trim().trim_start_matches(|c| c != ' ').trim().to_owned()
    }
}

/// Upload time in unix seconds: `timestamp` when present, else the date.
pub fn published(info: &Info) -> Option<i64> {
    info.timestamp.or_else(|| {
        let d = info.upload_date.as_deref().filter(|d| d.len() == 8)?;
        let month = time::Month::try_from(d[4..6].parse::<u8>().ok()?).ok()?;
        let date = time::Date::from_calendar_date(d[..4].parse().ok()?, month, d[6..].parse().ok()?).ok()?;
        Some(date.midnight().assume_utc().unix_timestamp())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_check_is_explained() {
        let e = human_error(
            "ERROR: [youtube] abc: Sign in to confirm you\u{2019}re not a bot. Use --cookies-from-browser",
        );
        assert!(e.contains("VPN"));
    }

    #[test]
    fn unknown_error_drops_extractor_prefix() {
        assert_eq!(human_error("ERROR: [youtube] abc: Something odd"), "abc: Something odd");
    }

    #[test]
    fn date_fallback() {
        let info = Info { upload_date: Some("20240102".into()), ..Default::default() };
        assert_eq!(published(&info), Some(1_704_153_600));
    }
}
