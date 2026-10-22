//! Playback: yt-dlp finds the streams, the webview plays them through MSE,
//! and the bytes come through our `stream://` protocol (googlevideo sends no
//! CORS headers, so the webview can't fetch it directly).

pub mod proxy;
pub mod sidx;
pub mod sponsorblock;
pub mod ytdlp;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::{Error, Result};
use ytdlp::{Chapter, Format, Info};

/// Stream URLs live ~6 h; refetch well before that.
const INFO_TTL: Duration = Duration::from_secs(60 * 60);
const INFO_CACHE: usize = 16;

pub fn is_video_id(s: &str) -> bool {
    s.len() == 11 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Where a `stream://` key points.
#[derive(Clone)]
pub struct Source {
    pub url: String,
    pub headers: Vec<(String, String)>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// Path under `stream://` — `<video id>.<format id>`.
    pub key: String,
    /// Full codec string for `MediaSource.isTypeSupported`, e.g. `avc1.640028`.
    pub codec: String,
    pub mime: &'static str,
    pub height: u32,
    pub fps: u32,
    pub bitrate: u32,
    /// Audio only: `en-US`, `ru`...
    pub lang: Option<String>,
    /// Audio only: "English (United States) original".
    pub label: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WatchInfo {
    pub id: String,
    pub title: String,
    pub channel: String,
    pub channel_id: Option<String>,
    pub description: String,
    pub duration: f64,
    pub views: Option<i64>,
    pub likes: Option<i64>,
    pub published: Option<i64>,
    pub is_live: bool,
    pub chapters: Vec<Chapter>,
    /// MP4 (H.264 / AV1) video-only tracks, best first. The page drops the
    /// ones the webview can't decode.
    pub video: Vec<Track>,
    /// M4A audio, one per language (dubbed videos have several), the
    /// original first.
    pub audio: Vec<Track>,
    /// Muxed 360p MP4: plays in a bare `<video src>` if MSE fails.
    pub progressive: Option<Track>,
}

struct Entry {
    at: Instant,
    info: Arc<WatchInfo>,
    /// Best of everything, any codec: what "Open in mpv" plays.
    mpv: Option<(Source, Option<Source>)>,
}

#[derive(Default)]
pub struct Player {
    infos: Mutex<HashMap<String, Entry>>,
    sources: Mutex<HashMap<String, Source>>,
}

fn source(f: &Format) -> Source {
    Source {
        url: f.url.clone().unwrap_or_default(),
        headers: f.http_headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
    }
}

fn track(video_id: &str, f: &Format, mime: &'static str, codec: &str) -> Track {
    Track {
        key: format!("{video_id}.{}", f.format_id),
        codec: codec.to_owned(),
        mime,
        height: f.height.unwrap_or(0),
        fps: f.fps.unwrap_or(0.0).round() as u32,
        bitrate: (f.tbr.unwrap_or(0.0) * 1000.0) as u32,
        lang: None,
        label: None,
    }
}

fn kbps(f: &Format) -> i64 {
    (f.tbr.unwrap_or(0.0) * 1000.0) as i64
}

/// Best M4A per language; the original (yt-dlp's highest language
/// preference) first, the rest by name. DRC variants only when nothing else.
fn audio_tracks<'a>(direct: &[&'a Format]) -> Vec<&'a Format> {
    let rank = |f: &Format| (!f.format_id.contains("drc"), kbps(f));
    let mut best: Vec<&Format> = Vec::new();
    let m4a = direct.iter().filter(|f| f.vcodec().is_empty() && f.acodec().starts_with("mp4a") && f.ext.as_deref() == Some("m4a"));
    for f in m4a {
        match best.iter_mut().find(|b| b.language == f.language) {
            Some(b) if rank(f) > rank(b) => *b = f,
            Some(_) => {}
            None => best.push(f),
        }
    }
    best.sort_by_cached_key(|f| (std::cmp::Reverse(f.language_preference.unwrap_or(-1)), audio_label(f)));
    best
}

/// The track's name from yt-dlp's note: "English (United States) original
/// (default), medium" -> "English (United States) original". A note without a
/// name starts with the quality; then the language code stands in.
fn audio_label(f: &Format) -> Option<String> {
    let lang = f.language.as_deref()?;
    let first = f.format_note.as_deref().unwrap_or_default().split(", ").next().unwrap_or_default();
    let name = first.replace("(default)", "").replace("(original)", "");
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let quality = matches!(name.as_str(), "" | "ultralow" | "low" | "medium" | "high");
    Some(if quality { lang.to_owned() } else { name })
}

impl Player {
    pub fn source(&self, key: &str) -> Option<Source> {
        self.sources.lock().ok()?.get(key).cloned()
    }

    pub async fn info(&self, video_id: &str, force: bool) -> Result<Arc<WatchInfo>> {
        if !is_video_id(video_id) {
            return Err(Error::msg("Bad video id"));
        }
        if !force {
            let infos = self.infos.lock().map_err(|_| Error::msg("Player lock poisoned"))?;
            if let Some(e) = infos.get(video_id).filter(|e| e.at.elapsed() < INFO_TTL) {
                return Ok(e.info.clone());
            }
        }

        let raw = ytdlp::fetch(video_id).await?;
        let (info, sources, mpv) = build(raw);
        let info = Arc::new(info);

        let mut infos = self.infos.lock().map_err(|_| Error::msg("Player lock poisoned"))?;
        let mut srcs = self.sources.lock().map_err(|_| Error::msg("Player lock poisoned"))?;
        if infos.len() >= INFO_CACHE {
            // Drop the oldest video together with its stream keys.
            if let Some(old) = infos.iter().min_by_key(|(_, e)| e.at).map(|(k, _)| k.clone()) {
                infos.remove(&old);
                srcs.retain(|k, _| !k.starts_with(&format!("{old}.")));
            }
        }
        srcs.extend(sources);
        infos.insert(video_id.to_owned(), Entry { at: Instant::now(), info: info.clone(), mpv });
        Ok(info)
    }

    pub fn mpv_sources(&self, video_id: &str) -> Option<(Source, Option<Source>)> {
        self.infos.lock().ok()?.get(video_id)?.mpv.clone()
    }

    /// Byte layout of a DASH track, read from the head of the file.
    pub async fn index(&self, http: &reqwest::Client, key: &str) -> Result<sidx::Index> {
        let src = self.source(key).ok_or_else(|| Error::msg("Stream expired, reload the video"))?;
        let mut want = 64 * 1024u64;
        for _ in 0..4 {
            let body = proxy::fetch_range(http, &src, 0, want - 1).await?.bytes().await?;
            match sidx::parse(&body).map_err(Error::msg)? {
                sidx::Parse::Done(ix) => return Ok(ix),
                sidx::Parse::NeedBytes(_) if (body.len() as u64) < want => {
                    return Err(Error::msg("Stream ended before its index"));
                }
                sidx::Parse::NeedBytes(n) => want = n.max(want * 2),
            }
        }
        Err(Error::msg("Stream index is too large"))
    }
}

type Built = (WatchInfo, Vec<(String, Source)>, Option<(Source, Option<Source>)>);

fn build(raw: Info) -> Built {
    let id = raw.id.clone();
    let mut sources = Vec::new();
    let direct: Vec<&Format> = raw.formats.iter().filter(|f| f.direct()).collect();

    // Video-only DASH MP4 the webview can decode: H.264 and AV1. VP9 lives in
    // WebM, which needs a different index parser; mpv covers it.
    let mut video: Vec<(&Format, Track)> = direct
        .iter()
        .filter(|f| f.acodec().is_empty() && f.ext.as_deref() == Some("mp4") && f.sdr())
        .filter(|f| f.vcodec().starts_with("avc1") || f.vcodec().starts_with("av01"))
        .map(|f| (*f, track(&id, f, "video/mp4", f.vcodec())))
        .collect();
    video.sort_by(|(_, a), (_, b)| (b.height, b.fps, b.bitrate).cmp(&(a.height, a.fps, a.bitrate)));

    let best_audio = |pred: &dyn Fn(&Format) -> bool| {
        direct
            .iter()
            .filter(|f| f.vcodec().is_empty() && !f.acodec().is_empty() && pred(f))
            .max_by_key(|f| (f.language_preference.unwrap_or(-1), kbps(f)))
            .copied()
    };
    let audio = audio_tracks(&direct);

    let progressive = direct
        .iter()
        .filter(|f| !f.vcodec().is_empty() && !f.acodec().is_empty() && f.ext.as_deref() == Some("mp4"))
        .max_by_key(|f| f.height.unwrap_or(0))
        .copied();

    for f in video.iter().map(|(f, _)| *f).chain(audio.iter().copied()).chain(progressive) {
        sources.push((format!("{id}.{}", f.format_id), source(f)));
    }

    let mpv_video = direct
        .iter()
        .filter(|f| !f.vcodec().is_empty())
        .max_by_key(|f| (f.height.unwrap_or(0), f.fps.unwrap_or(0.0) as u32, (f.tbr.unwrap_or(0.0)) as u32))
        .copied();
    let mpv = mpv_video.map(|v| {
        let a = if v.acodec().is_empty() { best_audio(&|_| true).map(source) } else { None };
        (source(v), a)
    });

    let info = WatchInfo {
        published: ytdlp::published(&raw),
        title: raw.title,
        channel: raw.channel.or(raw.uploader).unwrap_or_default(),
        channel_id: raw.channel_id,
        description: raw.description.unwrap_or_default(),
        duration: raw.duration.unwrap_or(0.0),
        views: raw.view_count,
        likes: raw.like_count,
        is_live: raw.is_live.unwrap_or(false),
        chapters: raw.chapters.unwrap_or_default(),
        video: video.into_iter().map(|(_, t)| t).collect(),
        audio: audio
            .iter()
            .map(|f| Track { lang: f.language.clone(), label: audio_label(f), ..track(&id, f, "audio/mp4", f.acodec()) })
            .collect(),
        progressive: progressive.map(|f| track(&id, f, "video/mp4", f.vcodec())),
        id,
    };
    (info, sources, mpv)
}

/// mpv: PATH first, then the default installer location.
fn mpv_binary() -> std::path::PathBuf {
    let default = std::path::PathBuf::from(r"C:\Program Files\MPV Player\mpv.exe");
    let on_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).map(|d| d.join("mpv.exe")).find(|p| p.is_file()))
        .unwrap_or_default();
    on_path.unwrap_or(default)
}

/// `audio`: the stream key of the track picked in the player, if any.
pub fn open_in_mpv(player: &Player, video_id: &str, title: &str, start: f64, audio: Option<&str>) -> Result<()> {
    if !is_video_id(video_id) {
        return Err(Error::msg("Bad video id"));
    }
    let mut cmd = std::process::Command::new(mpv_binary());
    cmd.arg(format!("--force-media-title={title}")).arg(format!("--start={}", start.max(0.0).floor()));
    match player.mpv_sources(video_id) {
        Some((video, audio_src)) => {
            if let Some(ua) = video.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("user-agent")) {
                cmd.arg(format!("--user-agent={}", ua.1));
            }
            // A muxed video has no separate audio to swap.
            let picked = audio.filter(|k| k.starts_with(&format!("{video_id}."))).and_then(|k| player.source(k));
            if let Some(a) = audio_src.map(|a| picked.unwrap_or(a)) {
                cmd.arg(format!("--audio-file={}", a.url));
            }
            cmd.arg("--ytdl=no").arg("--").arg(video.url);
        }
        // Live streams and the like: let mpv's own yt-dlp hook find them,
        // still without the user's yt-dlp config.
        None => {
            cmd.arg("--ytdl-raw-options=ignore-config=,no-cookies=")
                .arg("--")
                .arg(format!("https://www.youtube.com/watch?v={video_id}"));
        }
    }
    cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::msg("mpv isn't installed. Get it from mpv.io.")
        } else {
            Error::msg(format!("Couldn't start mpv: {e}"))
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(id: &str, lang: Option<&str>, pref: i64, tbr: f64, note: &str) -> Format {
        Format {
            format_id: id.into(),
            url: Some("https://x".into()),
            protocol: Some("https".into()),
            ext: Some("m4a".into()),
            vcodec: Some("none".into()),
            acodec: Some("mp4a.40.2".into()),
            tbr: Some(tbr),
            language: lang.map(Into::into),
            language_preference: Some(pref),
            format_note: Some(note.into()),
            ..Default::default()
        }
    }

    #[test]
    fn one_track_per_language_original_first() {
        let formats = [
            audio("139-0", Some("ru"), -1, 48.0, "Russian, low"),
            audio("140-0", Some("ru"), -1, 129.0, "Russian, medium"),
            audio("140-1", Some("en-US"), 10, 129.0, "English (United States) original (default), medium"),
            audio("140-drc", Some("en-US"), 10, 130.0, "English (United States) original (default), medium, DRC"),
            audio("140-2", Some("de-DE"), -1, 129.0, "German (Germany), medium"),
        ];
        let direct: Vec<&Format> = formats.iter().collect();
        let picked: Vec<(&str, Option<String>)> =
            audio_tracks(&direct).into_iter().map(|f| (f.format_id.as_str(), audio_label(f))).collect();
        assert_eq!(
            picked,
            [
                ("140-1", Some("English (United States) original".into())),
                ("140-2", Some("German (Germany)".into())),
                ("140-0", Some("Russian".into())),
            ]
        );
    }

    #[test]
    fn single_track_label_falls_back_to_code() {
        let f = audio("140", Some("en"), -1, 129.0, "medium");
        assert_eq!(audio_label(&f).as_deref(), Some("en"));
        assert_eq!(audio_label(&audio("140", None, -1, 129.0, "medium")), None);
        let direct = [&f];
        assert_eq!(audio_tracks(&direct).len(), 1);
    }
}
