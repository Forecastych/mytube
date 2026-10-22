//! What the user set on the Algorithm screen. Stored as one JSON value, so new
//! fields only need a `Default`: old saves keep loading.

use serde::{Deserialize, Serialize};

use super::{text, topics};
use crate::player::is_video_id;
use crate::youtube::channel::is_channel_id;

/// Per list. Far above any hand-made list, low enough to keep `rank` cheap.
pub const MAX_ITEMS: usize = 300;
/// Single videos pile up faster (one click each); a set lookup keeps it cheap.
pub const MAX_VIDEOS: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Favorites,
    Subscriptions,
    Trending,
    Related,
}

impl Source {
    /// When one video comes from several sources, the first one here wins.
    pub const ALL: [Source; 4] = [Source::Favorites, Source::Subscriptions, Source::Related, Source::Trending];
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedChannel {
    pub id: String,
    pub title: String,
    pub avatar: Option<String>,
}

/// One video hidden from the card menu. The title is only for the list on
/// the Algorithm screen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockedVideo {
    pub id: String,
    pub title: String,
}

/// Shares of the Home feed in percent. Always sums to 100 after `sanitize`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Mix {
    pub favorites: u32,
    pub subscriptions: u32,
    pub trending: u32,
    pub related: u32,
}

impl Default for Mix {
    fn default() -> Self {
        Self { favorites: 30, subscriptions: 25, trending: 25, related: 20 }
    }
}

impl Mix {
    pub fn share(&self, s: Source) -> u32 {
        match s {
            Source::Favorites => self.favorites,
            Source::Subscriptions => self.subscriptions,
            Source::Trending => self.trending,
            Source::Related => self.related,
        }
    }

    /// Scale to a sum of 100 with the largest-remainder method.
    fn normalize(&mut self) {
        let v = [self.favorites, self.subscriptions, self.trending, self.related].map(u64::from);
        let sum: u64 = v.iter().sum();
        if sum == 0 {
            *self = Self::default();
            return;
        }
        if sum == 100 {
            return;
        }
        let mut out = v.map(|x| x * 100 / sum);
        let mut left = 100 - out.iter().sum::<u64>();
        let mut by_remainder: Vec<usize> = (0..4).collect();
        // Stable: ties go to the earlier source.
        by_remainder.sort_by_key(|&i| std::cmp::Reverse(v[i] * 100 % sum));
        for i in by_remainder {
            if left == 0 {
                break;
            }
            out[i] += 1;
            left -= 1;
        }
        let [f, s, t, r] = out.map(|x| x as u32);
        *self = Self { favorites: f, subscriptions: s, trending: t, related: r };
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Rules {
    /// Ids from `topics::TOPICS`; anything else is matched like a blocked word.
    pub blocked_topics: Vec<String>,
    pub blocked_words: Vec<String>,
    pub blocked_channels: Vec<BlockedChannel>,
    /// Oldest first; past `MAX_VIDEOS` the oldest are dropped.
    pub blocked_videos: Vec<BlockedVideo>,
    pub mix: Mix,
    pub hide_shorts: bool,
    /// Hide videos shorter than this; 0 = off. Applies only where the
    /// duration is known (Data API details), RSS doesn't carry it.
    pub min_minutes: u32,
    pub hide_watched: bool,
    /// Off by default: watching a video never changes the feed.
    pub watch_affects: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            blocked_topics: Vec::new(),
            blocked_words: Vec::new(),
            blocked_channels: Vec::new(),
            blocked_videos: Vec::new(),
            mix: Mix::default(),
            hide_shorts: true,
            min_minutes: 3,
            hide_watched: true,
            watch_affects: false,
        }
    }
}

impl Rules {
    /// Clean whatever came from the UI: trim, lowercase, dedupe, cap.
    pub fn sanitize(&mut self) {
        self.blocked_topics = clean_words(&self.blocked_topics, |s| match topics::find(s) {
            Some(t) => Some(t.id.to_owned()),
            None => text::normalize(s),
        });
        self.blocked_words = clean_words(&self.blocked_words, text::normalize);

        let mut seen = std::collections::HashSet::new();
        self.blocked_channels.retain(|c| is_channel_id(&c.id) && seen.insert(c.id.clone()));
        self.blocked_channels.truncate(MAX_ITEMS);
        for c in &mut self.blocked_channels {
            c.title = c.title.trim().chars().take(200).collect();
            if c.avatar.as_deref().is_some_and(|a| !a.starts_with("https://")) {
                c.avatar = None;
            }
        }

        // Walk newest first so a re-block keeps its latest position.
        let mut seen = std::collections::HashSet::new();
        let mut videos: Vec<BlockedVideo> = std::mem::take(&mut self.blocked_videos)
            .into_iter()
            .rev()
            .filter(|v| is_video_id(&v.id) && seen.insert(v.id.clone()))
            .take(MAX_VIDEOS)
            .collect();
        videos.reverse();
        for v in &mut videos {
            v.title = v.title.trim().chars().take(200).collect();
        }
        self.blocked_videos = videos;

        self.mix.normalize();
        self.min_minutes = self.min_minutes.min(60);
    }
}

fn clean_words(list: &[String], norm: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    list.iter()
        .filter_map(|s| norm(s))
        .filter(|s| seen.insert(s.clone()))
        .take(MAX_ITEMS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mix(f: u32, s: u32, t: u32, r: u32) -> Mix {
        let mut m = Mix { favorites: f, subscriptions: s, trending: t, related: r };
        m.normalize();
        m
    }

    #[test]
    fn mix_sums_to_100() {
        assert_eq!(mix(30, 25, 25, 20), Mix::default());
        assert_eq!(mix(0, 0, 0, 0), Mix::default());
        assert_eq!(mix(1, 1, 1, 0), Mix { favorites: 34, subscriptions: 33, trending: 33, related: 0 });
        assert_eq!(mix(50, 50, 50, 50), Mix { favorites: 25, subscriptions: 25, trending: 25, related: 25 });
        assert_eq!(mix(7, 0, 0, 0), Mix { favorites: 100, subscriptions: 0, trending: 0, related: 0 });
    }

    #[test]
    fn sanitize_cleans_lists() {
        let mut r = Rules {
            blocked_topics: vec!["cooking".into(), "Cooking".into(), " Fishing ".into(), "".into()],
            blocked_words: vec!["Top 10".into(), "top  10".into(), "???".into(), "влог*".into()],
            blocked_channels: vec![
                BlockedChannel { id: "UCxxxxxxxxxxxxxxxxxxxxxx".into(), title: " A ".into(), avatar: Some("javascript:x".into()) },
                BlockedChannel { id: "UCxxxxxxxxxxxxxxxxxxxxxx".into(), title: "dup".into(), avatar: None },
                BlockedChannel { id: "not-a-channel".into(), title: "B".into(), avatar: None },
            ],
            ..Rules::default()
        };
        r.sanitize();
        assert_eq!(r.blocked_topics, ["cooking", "fishing"]);
        assert_eq!(r.blocked_words, ["top 10", "влог*"]);
        assert_eq!(r.blocked_channels.len(), 1);
        assert_eq!(r.blocked_channels[0].title, "A");
        assert_eq!(r.blocked_channels[0].avatar, None);
    }

    #[test]
    fn sanitize_caps_blocked_videos() {
        let v = |id: String| BlockedVideo { id, title: " t ".into() };
        let mut r = Rules {
            blocked_videos: (0..MAX_VIDEOS + 5).map(|i| v(format!("{i:0>11}"))).collect(),
            ..Rules::default()
        };
        r.blocked_videos.push(v("bad id".into()));
        r.blocked_videos.push(v(format!("{:0>11}", 0))); // re-blocked: moves to the end
        r.sanitize();
        assert_eq!(r.blocked_videos.len(), MAX_VIDEOS);
        assert_eq!(r.blocked_videos.last().unwrap().id, format!("{:0>11}", 0));
        assert_eq!(r.blocked_videos[0].id, format!("{:0>11}", 6)); // 1..=5 dropped as oldest
        assert_eq!(r.blocked_videos[0].title, "t");
    }

    #[test]
    fn old_saves_load_with_defaults() {
        let r: Rules = serde_json::from_str(r#"{"blockedWords":["vlog"]}"#).unwrap();
        assert_eq!(r.blocked_words, ["vlog"]);
        assert!(r.hide_shorts && !r.watch_affects);
        assert_eq!(r.mix, Mix::default());
    }
}
