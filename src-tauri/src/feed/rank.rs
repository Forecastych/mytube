//! `rank(candidates, rules) -> feed`. Pure: no network, no DB, no clock.
//!
//! 1. One video from several sources is kept once, under the strongest source.
//! 2. Hard filters drop what the user blocked.
//! 3. Each source is ordered on its own: favorites and subscriptions by
//!    freshness, trending and related keep the order they came in (chart
//!    position, relevance).
//! 4. Sources are interleaved by the Feed mix shares, not by a shared score.

use std::collections::HashSet;

use serde::Serialize;

use super::rules::{Rules, Source};
use super::text::{self, Pattern};
use super::topics;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub channel_id: String,
    pub channel_title: String,
    pub channel_avatar: Option<String>,
    pub title: String,
    /// unix seconds
    pub published: i64,
    pub views: Option<i64>,
    /// Seconds. Unknown for RSS-only videos.
    pub duration: Option<u32>,
    /// Data API `categoryId`. Unknown for RSS-only videos.
    pub category: Option<u32>,
    pub is_short: bool,
    pub is_new: bool,
    pub source: Source,
    #[serde(skip)]
    pub watched: bool,
    /// The user's order of favorite channels.
    #[serde(skip)]
    pub channel_rank: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Channel,
    Topic,
    Word,
    Short,
    TooShort,
    Watched,
}

/// `Rules` compiled once per ranking.
pub struct Filters {
    channels: HashSet<String>,
    categories: HashSet<u32>,
    patterns: Vec<(Reason, Pattern)>,
    hide_shorts: bool,
    min_secs: u32,
    hide_watched: bool,
}

impl Filters {
    pub fn new(rules: &Rules) -> Self {
        let mut categories = HashSet::new();
        let mut patterns = Vec::new();
        for t in &rules.blocked_topics {
            match topics::find(t) {
                Some(topic) => {
                    categories.extend(topic.categories);
                    patterns.extend(topic.words.iter().filter_map(|w| Pattern::new(w)).map(|p| (Reason::Topic, p)));
                }
                // A custom topic is just a word.
                None => patterns.extend(Pattern::new(t).map(|p| (Reason::Topic, p))),
            }
        }
        patterns.extend(rules.blocked_words.iter().filter_map(|w| Pattern::new(w)).map(|p| (Reason::Word, p)));

        Self {
            channels: rules.blocked_channels.iter().map(|c| c.id.clone()).collect(),
            categories,
            patterns,
            hide_shorts: rules.hide_shorts,
            min_secs: rules.min_minutes * 60,
            hide_watched: rules.hide_watched,
        }
    }

    /// Why `c` is hidden, or `None` if it stays.
    pub fn reason(&self, c: &Candidate) -> Option<Reason> {
        if self.channels.contains(&c.channel_id) {
            return Some(Reason::Channel);
        }
        if self.hide_watched && c.watched {
            return Some(Reason::Watched);
        }
        if self.hide_shorts && c.is_short {
            return Some(Reason::Short);
        }
        if c.duration.is_some_and(|d| d > 0 && d < self.min_secs) {
            return Some(Reason::TooShort);
        }
        if c.category.is_some_and(|cat| self.categories.contains(&cat)) {
            return Some(Reason::Topic);
        }
        if !self.patterns.is_empty() {
            let words = text::words(&c.title);
            if let Some((reason, _)) = self.patterns.iter().find(|(_, p)| p.is_match(&words)) {
                return Some(*reason);
            }
        }
        None
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ranked {
    pub videos: Vec<Candidate>,
    /// Dropped by the filters ("3 hidden by your filters").
    pub hidden: u32,
}

/// Filters only, order kept. For single-source screens (Trending, Favorites).
pub fn filter(candidates: Vec<Candidate>, rules: &Rules) -> Ranked {
    let f = Filters::new(rules);
    let before = candidates.len();
    let videos: Vec<_> = candidates.into_iter().filter(|c| f.reason(c).is_none()).collect();
    Ranked { hidden: (before - videos.len()) as u32, videos }
}

/// The mixed Home feed, at most `limit` videos.
pub fn rank(candidates: Vec<Candidate>, rules: &Rules, limit: usize) -> Ranked {
    // 1. Dedupe: walk sources strongest first, so a favorite that also trends
    // stays a favorite. Input order within a source is kept.
    let mut by_source: [Vec<Candidate>; 4] = Default::default();
    for c in candidates {
        by_source[slot(c.source)].push(c);
    }
    let mut seen = HashSet::new();
    for s in Source::ALL {
        by_source[slot(s)].retain(|c| seen.insert(c.id.clone()));
    }

    // 2. Filter.
    let f = Filters::new(rules);
    let mut hidden = 0u32;
    for list in &mut by_source {
        list.retain(|c| {
            let keep = f.reason(c).is_none();
            hidden += u32::from(!keep);
            keep
        });
    }

    // 3. Order within each source.
    by_source[slot(Source::Favorites)].sort_by_key(|c| {
        // Same day: the user's channel order decides; otherwise newer first.
        (std::cmp::Reverse(c.published.div_euclid(86_400)), c.channel_rank.unwrap_or(u32::MAX), std::cmp::Reverse(c.published))
    });
    by_source[slot(Source::Subscriptions)].sort_by_key(|c| std::cmp::Reverse(c.published));

    // 4. Mix: smooth weighted round-robin. Over any window each source gets
    // about its share; an exhausted source drops out and the rest fill in.
    let mut queues: Vec<(u32, i64, std::vec::IntoIter<Candidate>)> = Source::ALL
        .into_iter()
        .filter_map(|s| {
            let w = rules.mix.share(s);
            let list = std::mem::take(&mut by_source[slot(s)]);
            (w > 0 && !list.is_empty()).then(|| (w, 0, list.into_iter()))
        })
        .collect();

    let mut videos = Vec::with_capacity(limit);
    while videos.len() < limit && !queues.is_empty() {
        let total: i64 = queues.iter().map(|q| i64::from(q.0)).sum();
        for q in &mut queues {
            q.1 += i64::from(q.0);
        }
        // `max_by_key` returns the last max; reverse so ties go to the stronger source.
        let (i, _) = queues.iter().enumerate().rev().max_by_key(|(_, q)| q.1).expect("non-empty");
        queues[i].1 -= total;
        match queues[i].2.next() {
            Some(c) => videos.push(c),
            None => {
                queues.remove(i);
            }
        }
    }

    Ranked { videos, hidden }
}

fn slot(s: Source) -> usize {
    match s {
        Source::Favorites => 0,
        Source::Subscriptions => 1,
        Source::Related => 2,
        Source::Trending => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::rules::{BlockedChannel, Mix};

    fn v(id: &str, source: Source, published: i64) -> Candidate {
        Candidate {
            id: id.into(),
            channel_id: format!("UC{id:x<22}"),
            channel_title: "ch".into(),
            channel_avatar: None,
            title: format!("Video {id}"),
            published,
            views: None,
            duration: None,
            category: None,
            is_short: false,
            is_new: false,
            source,
            watched: false,
            channel_rank: None,
        }
    }

    fn ids(r: &Ranked) -> Vec<&str> {
        r.videos.iter().map(|c| c.id.as_str()).collect()
    }

    fn rules_with_mix(f: u32, s: u32, t: u32, r: u32) -> Rules {
        Rules { mix: Mix { favorites: f, subscriptions: s, trending: t, related: r }, ..Rules::default() }
    }

    #[test]
    fn filters_each_rule() {
        let rules = Rules {
            blocked_topics: vec!["cooking".into(), "sports".into()],
            blocked_words: vec!["top 10".into()],
            blocked_channels: vec![BlockedChannel { id: format!("UC{:x<22}", "bad"), title: "Bad".into(), avatar: None }],
            ..Rules::default()
        };
        let f = Filters::new(&rules);
        let mut c = v("ok", Source::Trending, 0);
        assert_eq!(f.reason(&c), None);

        c.title = "TOP-10 phones".into();
        assert_eq!(f.reason(&c), Some(Reason::Word));
        c.title = "Лучший рецепт плова".into();
        assert_eq!(f.reason(&c), Some(Reason::Topic));
        c.title = "Neutral".into();
        c.category = Some(17);
        assert_eq!(f.reason(&c), Some(Reason::Topic));
        c.category = None;

        c.duration = Some(120);
        assert_eq!(f.reason(&c), Some(Reason::TooShort));
        c.duration = Some(0); // live or unknown: never "too short"
        assert_eq!(f.reason(&c), None);

        c.is_short = true;
        assert_eq!(f.reason(&c), Some(Reason::Short));
        c.is_short = false;
        c.watched = true;
        assert_eq!(f.reason(&c), Some(Reason::Watched));

        let bad = v("bad", Source::Trending, 0);
        assert_eq!(f.reason(&bad), Some(Reason::Channel));
    }

    #[test]
    fn off_switches_keep_everything() {
        let rules = Rules { hide_shorts: false, hide_watched: false, min_minutes: 0, ..Rules::default() };
        let mut c = v("a", Source::Trending, 0);
        c.is_short = true;
        c.watched = true;
        c.duration = Some(30);
        assert_eq!(Filters::new(&rules).reason(&c), None);
    }

    #[test]
    fn dedupe_keeps_strongest_source() {
        let r = rank(vec![v("x", Source::Trending, 5), v("x", Source::Favorites, 5)], &Rules::default(), 10);
        assert_eq!(r.videos.len(), 1);
        assert_eq!(r.videos[0].source, Source::Favorites);
    }

    #[test]
    fn mix_follows_shares() {
        let mut cands = Vec::new();
        for i in 0..50 {
            cands.push(v(&format!("f{i}"), Source::Favorites, 1000 - i));
            cands.push(v(&format!("t{i}"), Source::Trending, 0));
        }
        let r = rank(cands, &rules_with_mix(75, 0, 25, 0), 20);
        let favs = r.videos.iter().filter(|c| c.source == Source::Favorites).count();
        assert_eq!(favs, 15);
        // Interleaved, not 15 favorites then 5 trending.
        assert!(ids(&r)[..4].iter().any(|id| id.starts_with('t')));
        // Trending keeps chart order.
        let t: Vec<_> = ids(&r).into_iter().filter(|id| id.starts_with('t')).collect();
        assert_eq!(t, ["t0", "t1", "t2", "t3", "t4"]);
    }

    #[test]
    fn zero_share_and_exhausted_sources() {
        let cands = vec![v("f1", Source::Favorites, 3), v("t1", Source::Trending, 0), v("t2", Source::Trending, 0)];
        assert_eq!(ids(&rank(cands.clone(), &rules_with_mix(100, 0, 0, 0), 10)), ["f1"]);
        // Favorites run out after one: trending fills the rest.
        assert_eq!(ids(&rank(cands, &rules_with_mix(90, 0, 10, 0), 10)), ["f1", "t1", "t2"]);
    }

    #[test]
    fn favorites_by_day_then_channel_order() {
        let day = 86_400 * 20_000;
        let mut a = v("a", Source::Favorites, day + 100);
        a.channel_rank = Some(2);
        let mut b = v("b", Source::Favorites, day + 50);
        b.channel_rank = Some(0);
        let mut old = v("old", Source::Favorites, day - 10);
        old.channel_rank = Some(0);
        let r = rank(vec![old, a, b], &rules_with_mix(100, 0, 0, 0), 10);
        assert_eq!(ids(&r), ["b", "a", "old"]);
    }

    #[test]
    fn counts_hidden() {
        let mut s = v("s", Source::Trending, 0);
        s.is_short = true;
        let r = filter(vec![s, v("ok", Source::Trending, 0)], &Rules::default());
        assert_eq!((ids(&r), r.hidden), (vec!["ok"], 1));
    }
}
