//! Built-in topics for "Blocked topics". A topic is a YouTube category (known
//! only when Data API details were fetched) plus a word list for titles, so it
//! also works on RSS-only videos. Data, not logic: extend freely.
//!
//! Words are deliberately narrow. A false "hidden" is worse than a miss: the
//! user can always add a word by hand.

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    pub id: &'static str,
    pub label: &'static str,
    /// Data API `snippet.categoryId`s that mean this topic outright.
    #[serde(skip)]
    pub categories: &'static [u32],
    #[serde(skip)]
    pub words: &'static [&'static str],
}

pub const TOPICS: &[Topic] = &[
    Topic {
        id: "cooking",
        label: "Cooking",
        categories: &[],
        words: &["cooking", "recipe", "recipes", "baking", "cook with me", "кулинар*", "рецепт*", "готовим", "готовка", "приготов*", "выпечк*"],
    },
    Topic { id: "mukbang", label: "Mukbang", categories: &[], words: &["mukbang*", "eating show", "мукбанг*", "итинг*"] },
    Topic {
        id: "reaction",
        label: "Reaction",
        categories: &[],
        words: &["reaction*", "reacting", "reacts", "react to", "реакци*", "реагиру*"],
    },
    Topic { id: "asmr", label: "ASMR", categories: &[], words: &["asmr", "асмр"] },
    Topic { id: "pranks", label: "Pranks", categories: &[], words: &["prank*", "пранк*", "розыгрыш*"] },
    Topic {
        id: "sports",
        label: "Sports",
        categories: &[17],
        words: &["football", "soccer", "nba", "nfl", "nhl", "ufc", "футбол*", "хоккей*", "баскетбол*", "теннис*"],
    },
    Topic {
        id: "gaming",
        label: "Gaming",
        categories: &[20],
        words: &["gameplay", "let's play", "walkthrough", "speedrun*", "летсплей*", "прохождени*", "геймплей*"],
    },
    Topic {
        id: "music",
        label: "Music",
        categories: &[10],
        words: &["official video", "official audio", "music video", "lyric video", "клип"],
    },
    Topic {
        id: "news",
        label: "News & politics",
        categories: &[25],
        words: &["news", "politic*", "election*", "новост*", "политик*", "выборы"],
    },
    Topic {
        id: "true-crime",
        label: "True crime",
        categories: &[],
        words: &["true crime", "murder*", "serial killer*", "тру крайм", "убийств*", "маньяк*"],
    },
    Topic { id: "gossip", label: "Celebrity gossip", categories: &[], words: &["gossip", "celebrity", "celebrities", "сплетн*"] },
    Topic { id: "vlogs", label: "Vlogs", categories: &[], words: &["vlog*", "влог*", "day in my life", "day in the life"] },
    Topic { id: "unboxing", label: "Unboxing", categories: &[], words: &["unboxing", "распаковк*", "анбоксинг*"] },
    Topic { id: "challenges", label: "Challenges", categories: &[], words: &["challenge*", "челлендж*"] },
    Topic { id: "crypto", label: "Crypto", categories: &[], words: &["crypto*", "bitcoin*", "nft*", "криптовалют*", "биткоин*"] },
    Topic {
        id: "astrology",
        label: "Astrology & tarot",
        categories: &[],
        words: &["horoscope*", "astrology", "tarot", "гороскоп*", "астролог*", "таро"],
    },
];

pub fn find(id: &str) -> Option<&'static Topic> {
    TOPICS.iter().find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::text;

    #[test]
    fn ids_unique_and_words_valid() {
        for (i, t) in TOPICS.iter().enumerate() {
            assert!(TOPICS[..i].iter().all(|o| o.id != t.id), "duplicate topic {}", t.id);
            for w in t.words {
                assert_eq!(text::normalize(w).as_deref(), Some(*w), "word {w:?} in {} is not canonical", t.id);
            }
        }
    }
}
