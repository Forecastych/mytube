//! Whole-word, case-insensitive matching of titles, in any script.
//!
//! A pattern is one or more words. `top 10` matches "Top-10 laptops" but not
//! "stop 100". A trailing `*` matches word endings, which Russian needs:
//! `влог*` → влог, влоги, влогер.

/// Longest pattern we keep, in chars.
pub const MAX_LEN: usize = 60;

/// Lowercased words of `s`. `ё` folds to `е`: titles use both.
pub fn words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.chars().flat_map(char::to_lowercase).map(|c| if c == 'ё' { 'е' } else { c }).collect())
        .collect()
}

/// The canonical stored form of a user-typed word or phrase, or `None` if it
/// has no letters or digits. Keeps punctuation and `*`, so the UI shows what
/// the user typed ("let's play", not "let s play").
pub fn normalize(s: &str) -> Option<String> {
    Pattern::new(s)?;
    let out = s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    (out.chars().count() <= MAX_LEN).then_some(out)
}

#[derive(Clone, Debug)]
pub struct Pattern {
    words: Vec<String>,
    /// The last word matches as a prefix.
    prefix: bool,
}

impl Pattern {
    pub fn new(s: &str) -> Option<Self> {
        let s = s.trim();
        let words = words(s);
        if words.is_empty() {
            return None;
        }
        Some(Self { words, prefix: s.ends_with('*') })
    }

    /// `title` is the output of [`words`]: split once, test every pattern.
    pub fn is_match(&self, title: &[String]) -> bool {
        let (last, head) = self.words.split_last().expect("non-empty");
        title.windows(self.words.len()).any(|w| {
            let (w_last, w_head) = w.split_last().expect("non-empty");
            w_head == head
                && if self.prefix { w_last.starts_with(last.as_str()) } else { w_last == last }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(pattern: &str, title: &str) -> bool {
        Pattern::new(pattern).unwrap().is_match(&words(title))
    }

    #[test]
    fn whole_words_only() {
        assert!(hit("vlog", "My VLOG #12"));
        assert!(!hit("vlog", "vlogger life"));
        assert!(!hit("art", "Smart home"));
        assert!(hit("asmr", "ASMR: tapping"));
    }

    #[test]
    fn phrases_and_punctuation() {
        assert!(hit("top 10", "TOP-10 laptops of 2026"));
        assert!(!hit("top 10", "stop 100 things"));
        assert!(hit("how to", "How to: fix a sink"));
        // A curly apostrophe splits words the same way as a straight one.
        assert!(hit("let's play", "Let’s Play Doom — part 1"));
        assert!(!hit("let's play", "Lets play"));
    }

    #[test]
    fn prefixes_and_cyrillic() {
        assert!(hit("влог*", "Мой ВЛОГ из Питера"));
        assert!(hit("влог*", "Влоги каждый день"));
        assert!(!hit("влог", "Влоги каждый день"));
        assert!(hit("рецепт*", "Лучший рецепт борща"));
        assert!(hit("ёлка", "Новогодняя Елка"));
    }

    #[test]
    fn normalizes_input() {
        assert_eq!(normalize("  Top   10 ").as_deref(), Some("top 10"));
        assert_eq!(normalize("Влог*").as_deref(), Some("влог*"));
        assert_eq!(normalize("!!!"), None);
        assert_eq!(normalize("*"), None);
        assert_eq!(normalize(&"a".repeat(MAX_LEN + 1)), None);
    }
}
