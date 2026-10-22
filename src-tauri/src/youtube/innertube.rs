//! The one door to Innertube, the API youtube.com's own pages use. Anonymous:
//! no cookies, no OAuth token, no Data API quota. Comments, search, related
//! videos and channel headers all go through `post`.

use serde_json::{json, Value};

use crate::error::{Error, Result};

pub const CLIENT_VERSION: &str = "2.20251020.01.00";

/// `endpoint` is `next`, `search` or `browse`; `what` names it in errors.
pub async fn post(http: &reqwest::Client, endpoint: &str, mut body: Value, what: &str) -> Result<Value> {
    body["context"] = json!({ "client": { "clientName": "WEB", "clientVersion": CLIENT_VERSION, "hl": "en" } });
    let res = http
        .post(format!("https://www.youtube.com/youtubei/v1/{endpoint}?prettyPrint=false"))
        .header("Content-Type", "application/json")
        .header("X-YouTube-Client-Name", "1")
        .header("X-YouTube-Client-Version", CLIENT_VERSION)
        .body(body.to_string())
        .send()
        .await?;
    if !res.status().is_success() {
        return Err(Error::msg(format!("YouTube answered {} for {what}", res.status())));
    }
    let bytes = res.bytes().await?;
    serde_json::from_slice(&bytes).map_err(|e| Error::msg(format!("Bad {what} answer: {e}")))
}

/// The first `continuationCommand.token` anywhere under `v`.
pub fn token_in(v: &Value) -> Option<String> {
    let mut cmds = Vec::new();
    find_all(v, "continuationCommand", &mut cmds);
    cmds.iter().find_map(|c| c["token"].as_str()).map(str::to_owned)
}

pub fn find_all<'a>(v: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(map) => {
            for (k, child) in map {
                if k == key {
                    out.push(child);
                }
                find_all(child, key, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|c| find_all(c, key, out)),
        _ => {}
    }
}
