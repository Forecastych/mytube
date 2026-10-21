//! `stream://<video id>.<format id>` → googlevideo, with Range passed through.
//! On Windows the webview sees it as `http://stream.localhost/...`.

use tauri::http::{header, Method, Request, Response, StatusCode};
use tauri::{AppHandle, Manager, Runtime};

use super::Source;
use crate::commands::AppState;
use crate::error::Result;

/// Open-ended ranges (`bytes=N-`, what a bare `<video src>` asks for) are cut
/// to this size: we hold each response in memory, and googlevideo throttles
/// long single requests anyway.
const CHUNK: u64 = 4 * 1024 * 1024;

pub async fn fetch_range(http: &reqwest::Client, src: &Source, start: u64, end: u64) -> Result<reqwest::Response> {
    let mut last = None;
    // googlevideo drops a request now and then; one retry hides it.
    for _ in 0..2 {
        let mut req = http.get(&src.url).header(header::RANGE, format!("bytes={start}-{end}"));
        for (k, v) in &src.headers {
            req = req.header(k, v);
        }
        match req.send().await {
            Ok(r) if r.status().is_server_error() => last = Some(r.error_for_status().unwrap_err()),
            Ok(r) => return Ok(r.error_for_status()?),
            Err(e) => last = Some(e),
        }
    }
    Err(last.expect("loop ran").into())
}

fn parse_range(h: Option<&str>) -> (u64, Option<u64>) {
    let Some(spec) = h.and_then(|h| h.strip_prefix("bytes=")) else { return (0, None) };
    let (a, b) = spec.split_once('-').unwrap_or((spec, ""));
    (a.trim().parse().unwrap_or(0), b.trim().parse().ok())
}

fn cors(b: tauri::http::response::Builder) -> tauri::http::response::Builder {
    b.header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "Range")
        .header(header::ACCESS_CONTROL_EXPOSE_HEADERS, "Content-Range, Content-Length")
}

fn status(code: StatusCode, msg: &str) -> Response<Vec<u8>> {
    cors(Response::builder()).status(code).body(msg.as_bytes().to_vec()).expect("static response")
}

pub async fn handle<R: Runtime>(app: AppHandle<R>, req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    if req.method() == Method::OPTIONS {
        return status(StatusCode::NO_CONTENT, "");
    }
    let key = req.uri().path().trim_start_matches('/');
    let state = app.state::<AppState>();
    let Some(src) = state.player.source(key) else {
        return status(StatusCode::NOT_FOUND, "unknown stream");
    };

    let (start, end) = parse_range(req.headers().get(header::RANGE).and_then(|v| v.to_str().ok()));
    let end = end.unwrap_or(start + CHUNK - 1).min(start + CHUNK * 4 - 1);

    let resp = match fetch_range(&state.http, &src, start, end).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("stream {key}: {e}");
            // 403 means the signed URL expired or is bound to another IP.
            let code = match &e {
                crate::error::Error::Http(h) => h.status().unwrap_or(StatusCode::BAD_GATEWAY),
                _ => StatusCode::BAD_GATEWAY,
            };
            return status(code, "upstream error");
        }
    };

    let mut b = cors(Response::builder()).status(resp.status()).header(header::ACCEPT_RANGES, "bytes");
    for h in [header::CONTENT_TYPE, header::CONTENT_RANGE] {
        if let Some(v) = resp.headers().get(&h) {
            b = b.header(h, v.clone());
        }
    }
    match resp.bytes().await {
        Ok(body) => b.body(body.to_vec()).expect("valid response"),
        Err(e) => {
            eprintln!("stream {key}: {e}");
            status(StatusCode::BAD_GATEWAY, "upstream dropped")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_range;

    #[test]
    fn ranges() {
        assert_eq!(parse_range(None), (0, None));
        assert_eq!(parse_range(Some("bytes=100-")), (100, None));
        assert_eq!(parse_range(Some("bytes=5-9")), (5, Some(9)));
    }
}
