//! SponsorBlock: segments of a video that its author's sponsors paid for.
//! Queried by a 4-char SHA-256 prefix of the video id, so the server never
//! learns which video is being watched.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::Result;

const API: &str = "https://sponsor.ajay.app/api/skipSegments";
const CATEGORIES: &str =
    r#"["sponsor","selfpromo","interaction","intro","outro","preview","music_offtopic","filler"]"#;

#[derive(Serialize, Debug, PartialEq)]
pub struct Segment {
    pub category: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Video {
    #[serde(rename = "videoID")]
    video_id: String,
    segments: Vec<RawSegment>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSegment {
    category: String,
    segment: [f64; 2],
    #[serde(default)]
    action_type: Option<String>,
}

fn prefix(video_id: &str) -> String {
    let hash = Sha256::digest(video_id.as_bytes());
    hash.iter().take(2).map(|b| format!("{b:02x}")).collect()
}

fn pick(body: &[u8], video_id: &str) -> Result<Vec<Segment>> {
    let videos: Vec<Video> = serde_json::from_slice(body).map_err(|e| crate::error::Error::msg(e.to_string()))?;
    Ok(videos
        .into_iter()
        .filter(|v| v.video_id == video_id)
        .flat_map(|v| v.segments)
        // "mute", "full", "chapter", "poi" aren't skips.
        .filter(|s| s.action_type.as_deref().is_none_or(|a| a == "skip"))
        .map(|s| Segment { category: s.category, start: s.segment[0], end: s.segment[1] })
        .collect())
}

pub async fn segments(http: &reqwest::Client, video_id: &str) -> Result<Vec<Segment>> {
    let url = reqwest::Url::parse_with_params(&format!("{API}/{}", prefix(video_id)), [("categories", CATEGORIES)])
        .map_err(|e| crate::error::Error::msg(e.to_string()))?;
    let resp = http.get(url).send().await?;
    // 404 = no segments for any video with this prefix.
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    let body = resp.error_for_status()?.bytes().await?;
    pick(&body, video_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_prefix() {
        // python: hashlib.sha256(b"dQw4w9WgXcQ").hexdigest()[:4]
        assert_eq!(prefix("dQw4w9WgXcQ"), "5f6b");
    }

    #[test]
    fn keeps_only_this_video_and_skips() {
        let body = br#"[
            {"videoID":"aaaaaaaaaaa","segments":[{"category":"sponsor","segment":[1,2],"actionType":"skip"}]},
            {"videoID":"bbbbbbbbbbb","segments":[
                {"category":"sponsor","segment":[10.5,20],"actionType":"skip"},
                {"category":"music_offtopic","segment":[0,5],"actionType":"mute"}]}
        ]"#;
        assert_eq!(
            pick(body, "bbbbbbbbbbb").unwrap(),
            vec![Segment { category: "sponsor".into(), start: 10.5, end: 20.0 }]
        );
    }
}
