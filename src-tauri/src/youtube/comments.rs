//! Comments through Innertube, the API youtube.com's own pages use. Anonymous:
//! no cookies, no OAuth token, no Data API quota.
//!
//! The answer is a tree of renderers that YouTube reshuffles every few months,
//! so parsing searches for a handful of stable keys instead of walking fixed
//! paths: `commentThreadRenderer` / `commentViewModel` give the order and the
//! entity key, `commentEntityPayload` (in `frameworkUpdates`) holds the text,
//! and `continuationCommand.token` is how every next page is asked for.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{json, Value};

use super::innertube::{self, find_all, token_in};
use crate::error::Result;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    /// "1,234" — only on the first page.
    pub count: Option<String>,
    /// "Top" / "Newest" with the token that reloads the list in that order.
    pub sort: Vec<SortOption>,
    pub comments: Vec<Comment>,
    pub next: Option<String>,
    /// The video has comments turned off.
    pub disabled: bool,
}

#[derive(Serialize, Debug)]
pub struct SortOption {
    pub title: String,
    pub token: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub avatar: Option<String>,
    pub text: String,
    /// "3 days ago (edited)"
    pub published: String,
    pub likes: Option<String>,
    pub reply_count: Option<String>,
    /// Token for the first page of replies.
    pub replies: Option<String>,
    pub pinned: bool,
    pub verified: bool,
    /// Written by the video's channel.
    pub creator: bool,
}

/// First page of a video's comments, in YouTube's default ("Top") order.
pub async fn first(http: &reqwest::Client, video_id: &str) -> Result<Page> {
    let watch = innertube::post(http, "next", json!({ "videoId": video_id }), "comments").await?;
    match section_token(&watch) {
        Some(token) => more(http, &token).await,
        None => Ok(Page { disabled: true, ..Page::default() }),
    }
}

/// Any continuation: the next page, a sort order, or a thread's replies.
pub async fn more(http: &reqwest::Client, token: &str) -> Result<Page> {
    Ok(parse(&innertube::post(http, "next", json!({ "continuation": token }), "comments").await?))
}

fn section_token(watch: &Value) -> Option<String> {
    let mut sections = Vec::new();
    find_all(watch, "itemSectionRenderer", &mut sections);
    let section = sections.into_iter().find(|s| s["sectionIdentifier"] == "comment-item-section")?;
    token_in(section)
}

fn parse(v: &Value) -> Page {
    let mut entities = Vec::new();
    find_all(&v["frameworkUpdates"], "commentEntityPayload", &mut entities);
    let entities: HashMap<&str, &Value> = entities.into_iter().filter_map(|e| Some((e["key"].as_str()?, e))).collect();

    let mut page = Page::default();
    for endpoint in v["onResponseReceivedEndpoints"].as_array().into_iter().flatten() {
        let items = endpoint
            .get("reloadContinuationItemsCommand")
            .or_else(|| endpoint.get("appendContinuationItemsAction"))
            .and_then(|c| c["continuationItems"].as_array());
        for item in items.into_iter().flatten() {
            if let Some(h) = item.get("commentsHeaderRenderer") {
                page.count = h["countText"]["runs"][0]["text"].as_str().map(str::to_owned);
                page.sort = sort_options(h);
            } else if let Some(t) = item.get("commentThreadRenderer") {
                let vm = &t["commentViewModel"]["commentViewModel"];
                let pinned = t["renderingPriority"] == "RENDERING_PRIORITY_PINNED_COMMENT";
                let replies = t.get("replies").and_then(token_in);
                page.comments.extend(comment(vm, &entities, pinned, replies));
            } else if let Some(vm) = item.get("commentViewModel") {
                page.comments.extend(comment(vm, &entities, false, None));
            } else if let Some(c) = item.get("continuationItemRenderer") {
                page.next = token_in(c);
            }
        }
    }
    page
}

fn comment(vm: &Value, entities: &HashMap<&str, &Value>, pinned: bool, replies: Option<String>) -> Option<Comment> {
    let e = entities.get(vm["commentKey"].as_str()?)?;
    let p = &e["properties"];
    let a = &e["author"];
    let t = &e["toolbar"];
    let nonempty = |v: &Value| v.as_str().map(str::trim).filter(|s| !s.is_empty() && *s != "0").map(str::to_owned);
    Some(Comment {
        id: p["commentId"].as_str()?.to_owned(),
        author: a["displayName"].as_str().unwrap_or_default().to_owned(),
        avatar: nonempty(&a["avatarThumbnailUrl"]),
        text: p["content"]["content"].as_str().unwrap_or_default().to_owned(),
        published: p["publishedTime"].as_str().unwrap_or_default().to_owned(),
        likes: nonempty(&t["likeCountNotliked"]),
        reply_count: nonempty(&t["replyCount"]),
        replies,
        pinned,
        verified: a["isVerified"].as_bool().unwrap_or(false),
        creator: a["isCreator"].as_bool().unwrap_or(false),
    })
}

fn sort_options(header: &Value) -> Vec<SortOption> {
    let mut menus = Vec::new();
    find_all(header, "sortFilterSubMenuRenderer", &mut menus);
    menus
        .first()
        .and_then(|m| m["subMenuItems"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|i| {
            Some(SortOption {
                title: i["title"].as_str()?.to_owned(),
                token: i["serviceEndpoint"]["continuationCommand"]["token"].as_str()?.to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(key: &str, id: &str, text: &str, likes: &str) -> Value {
        json!({ "payload": { "commentEntityPayload": {
            "key": key,
            "properties": { "commentId": id, "content": { "content": text }, "publishedTime": "2 days ago", "replyLevel": 0 },
            "author": { "displayName": "@someone", "avatarThumbnailUrl": "https://yt3.ggpht.com/a=s88", "isVerified": false, "isCreator": id == "c2" },
            "toolbar": { "likeCountNotliked": likes, "replyCount": "" }
        }}})
    }

    #[test]
    fn parses_first_page() {
        let v = json!({
            "onResponseReceivedEndpoints": [
                { "reloadContinuationItemsCommand": { "continuationItems": [
                    { "commentsHeaderRenderer": {
                        "countText": { "runs": [{ "text": "1,234" }, { "text": " Comments" }] },
                        "sortMenu": { "sortFilterSubMenuRenderer": { "subMenuItems": [
                            { "title": "Top", "serviceEndpoint": { "continuationCommand": { "token": "TOP" } } },
                            { "title": "Newest", "serviceEndpoint": { "continuationCommand": { "token": "NEW" } } }
                        ]}}
                    }}
                ]}},
                { "reloadContinuationItemsCommand": { "continuationItems": [
                    { "commentThreadRenderer": {
                        "renderingPriority": "RENDERING_PRIORITY_PINNED_COMMENT",
                        "commentViewModel": { "commentViewModel": { "commentKey": "k1" } },
                        "replies": { "commentRepliesRenderer": { "contents": [
                            { "continuationItemRenderer": { "continuationEndpoint": { "continuationCommand": { "token": "R1" } } } }
                        ]}}
                    }},
                    { "commentThreadRenderer": { "commentViewModel": { "commentViewModel": { "commentKey": "k2" } } } },
                    { "commentThreadRenderer": { "commentViewModel": { "commentViewModel": { "commentKey": "missing" } } } },
                    { "continuationItemRenderer": { "continuationEndpoint": { "continuationCommand": { "token": "NEXT" } } } }
                ]}}
            ],
            "frameworkUpdates": { "entityBatchUpdate": { "mutations": [
                entity("k2", "c2", "second", " "),
                entity("k1", "c1", "first at 1:23", "4.8K")
            ]}}
        });
        let p = parse(&v);
        assert_eq!(p.count.as_deref(), Some("1,234"));
        assert_eq!(p.sort.iter().map(|s| s.token.as_str()).collect::<Vec<_>>(), ["TOP", "NEW"]);
        assert_eq!(p.comments.len(), 2, "unknown entity keys are skipped");
        assert_eq!(p.comments[0].id, "c1");
        assert!(p.comments[0].pinned);
        assert_eq!(p.comments[0].replies.as_deref(), Some("R1"));
        assert_eq!(p.comments[0].likes.as_deref(), Some("4.8K"));
        assert_eq!(p.comments[1].likes, None, "blank like counts become None");
        assert!(p.comments[1].creator);
        assert_eq!(p.next.as_deref(), Some("NEXT"));
    }

    #[test]
    fn parses_reply_page() {
        let v = json!({
            "onResponseReceivedEndpoints": [{ "appendContinuationItemsAction": { "continuationItems": [
                { "commentViewModel": { "commentKey": "k1" } },
                { "continuationItemRenderer": { "button": { "buttonRenderer": { "command": { "continuationCommand": { "token": "MORE" } } } } } }
            ]}}],
            "frameworkUpdates": { "entityBatchUpdate": { "mutations": [entity("k1", "r1", "a reply", "3")] } }
        });
        let p = parse(&v);
        assert_eq!(p.comments.len(), 1);
        assert_eq!(p.next.as_deref(), Some("MORE"));
    }

    #[test]
    fn finds_comment_section() {
        let v = json!({ "contents": [
            { "itemSectionRenderer": { "sectionIdentifier": "related", "contents": [{ "continuationCommand": { "token": "NO" } }] } },
            { "itemSectionRenderer": { "sectionIdentifier": "comment-item-section", "contents": [
                { "continuationItemRenderer": { "continuationEndpoint": { "continuationCommand": { "token": "YES" } } } }
            ]}}
        ]});
        assert_eq!(section_token(&v).as_deref(), Some("YES"));
        assert_eq!(section_token(&json!({})), None);
    }

    /// Hits YouTube: `cargo test comments::tests::live -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live() {
        let http = crate::net::client();
        let p = first(&http, "jNQXAC9IVRw").await.unwrap();
        println!("{:?} {} comments, next: {}", p.count, p.comments.len(), p.next.is_some());
        assert!(!p.comments.is_empty());
        let token = p.comments.iter().find_map(|c| c.replies.clone()).expect("a thread with replies");
        let r = more(&http, &token).await.unwrap();
        println!("replies: {}", r.comments.len());
        assert!(!r.comments.is_empty());
    }
}
