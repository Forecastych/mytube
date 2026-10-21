//! Channel RSS: `feeds/videos.xml?channel_id=…`. Free, no auth, last 15 uploads.
//! YouTube sends `max-age=900` and no ETag, so we cache by TTL instead.

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::Event;
use quick_xml::Reader;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::error::{Error, Result};

pub struct Entry {
    pub id: String,
    pub title: String,
    pub published: i64,
    pub views: Option<i64>,
    /// RSS links Shorts as `/shorts/<id>` instead of `/watch?v=<id>`.
    pub is_short: bool,
}

pub struct Feed {
    pub title: String,
    pub entries: Vec<Entry>,
}

pub async fn fetch(http: &reqwest::Client, channel_id: &str) -> Result<Feed> {
    let resp = http
        // Ids come from our own DB and are validated `UC…` strings: safe to inline.
        .get(format!("https://www.youtube.com/feeds/videos.xml?channel_id={channel_id}"))
        .send()
        .await?
        .error_for_status()?;
    parse(&resp.text().await?)
}

#[derive(Clone, Copy)]
enum Field {
    FeedTitle,
    Id,
    Title,
    Published,
}

pub fn parse(xml: &str) -> Result<Feed> {
    let mut reader = Reader::from_str(xml);
    let mut feed = Feed { title: String::new(), entries: Vec::with_capacity(15) };

    let mut depth = 0u32;
    let mut entry: Option<Entry> = None;
    let mut field: Option<Field> = None;
    let mut text = String::new();

    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                depth += 1;
                let name = e.name();
                field = match (name.as_ref(), entry.is_some()) {
                    ("entry", _) => {
                        entry = Some(Entry {
                            id: String::new(),
                            title: String::new(),
                            published: 0,
                            views: None,
                            is_short: false,
                        });
                        None
                    }
                    ("title", false) if depth == 2 => Some(Field::FeedTitle),
                    ("yt:videoId", true) => Some(Field::Id),
                    ("title", true) => Some(Field::Title),
                    ("published", true) => Some(Field::Published),
                    _ => None,
                };
                text.clear();
            }
            Event::Empty(e) => {
                let Some(en) = entry.as_mut() else { continue };
                match e.name().as_ref() {
                    "link" => {
                        if let Some(href) = e.try_get_attribute("href").ok().flatten() {
                            en.is_short = href.value.contains("/shorts/");
                        }
                    }
                    "media:statistics" => {
                        if let Some(v) = e.try_get_attribute("views").ok().flatten() {
                            en.views = v.value.parse().ok();
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) if field.is_some() => text.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if field.is_some() => {
                if let Ok(Some(ch)) = r.resolve_char_ref() {
                    text.push(ch);
                } else if let Some(s) = resolve_predefined_entity(&r) {
                    text.push_str(s);
                }
            }
            Event::End(e) => {
                depth = depth.saturating_sub(1);
                if let Some(f) = field.take() {
                    let value = std::mem::take(&mut text);
                    match (f, entry.as_mut()) {
                        (Field::FeedTitle, _) => feed.title = value,
                        (Field::Id, Some(en)) => en.id = value,
                        (Field::Title, Some(en)) => en.title = value,
                        (Field::Published, Some(en)) => {
                            en.published = OffsetDateTime::parse(value.trim(), &Rfc3339)
                                .map(|d| d.unix_timestamp())
                                .unwrap_or(0)
                        }
                        _ => {}
                    }
                }
                if e.name().as_ref() == "entry" {
                    if let Some(en) = entry.take().filter(|en| !en.id.is_empty()) {
                        feed.entries.push(en);
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    if feed.title.is_empty() {
        return Err(Error::msg("Not a YouTube channel feed"));
    }
    Ok(feed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns:yt="http://www.youtube.com/xml/schemas/2015" xmlns:media="http://search.yahoo.com/mrss/" xmlns="http://www.w3.org/2005/Atom">
 <title>Tom &amp; Jerry&#39;s</title>
 <author><name>Tom</name></author>
 <entry>
  <yt:videoId>short1</yt:videoId>
  <title>A short</title>
  <link rel="alternate" href="https://www.youtube.com/shorts/short1"/>
  <published>2026-10-15T13:00:26+00:00</published>
  <media:group><media:title>A short</media:title>
   <media:community><media:statistics views="1164165"/></media:community></media:group>
 </entry>
 <entry>
  <yt:videoId>vid2</yt:videoId>
  <title>Fiber &lt;Optics&gt;</title>
  <link rel="alternate" href="https://www.youtube.com/watch?v=vid2"/>
  <published>2026-10-12T17:11:45+00:00</published>
  <media:group><media:community><media:statistics views="42"/></media:community></media:group>
 </entry>
</feed>"#;

    #[test]
    fn parses_entries_entities_and_shorts() {
        let f = parse(SAMPLE).unwrap();
        assert_eq!(f.title, "Tom & Jerry's");
        assert_eq!(f.entries.len(), 2);
        assert_eq!(f.entries[0].id, "short1");
        assert!(f.entries[0].is_short);
        assert_eq!(f.entries[0].views, Some(1164165));
        assert_eq!(f.entries[1].title, "Fiber <Optics>");
        assert!(!f.entries[1].is_short);
        assert_eq!(f.entries[1].published, 1_791_825_105);
    }
}
