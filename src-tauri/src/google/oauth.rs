//! OAuth 2.0 for installed apps: the system browser, a loopback redirect to
//! 127.0.0.1 and PKCE. The embedded webview is never used: Google blocks it.

use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::vault::Client;
use crate::error::{Error, Result};

pub const SCOPE: &str = "https://www.googleapis.com/auth/youtube.readonly";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";

/// How long a sign-in waits for the browser before giving up.
pub const WAIT: Duration = Duration::from_secs(300);

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64url without padding (RFC 4648 §5), as PKCE wants.
fn b64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

fn random(len: usize) -> Result<String> {
    let mut buf = vec![0u8; len];
    getrandom::fill(&mut buf).map_err(|e| Error::msg(format!("No system randomness: {e}")))?;
    Ok(b64url(&buf))
}

/// Percent-encodes everything but the RFC 3986 unreserved set.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => match bytes.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok()) {
                Some(b) => {
                    out.push(b);
                    i += 2;
                }
                None => out.push(b'%'),
            },
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| format!("{k}={}", encode(v))).collect::<Vec<_>>().join("&")
}

/// One sign-in attempt: the PKCE pair and the anti-CSRF state.
pub struct Attempt {
    verifier: String,
    state: String,
    redirect: String,
}

impl Attempt {
    pub fn new(port: u16) -> Result<Self> {
        Ok(Attempt { verifier: random(32)?, state: random(16)?, redirect: format!("http://127.0.0.1:{port}") })
    }

    pub fn url(&self, client: &Client) -> String {
        let challenge = b64url(&Sha256::digest(self.verifier.as_bytes()));
        format!(
            "{AUTH_URL}?{}",
            form(&[
                ("client_id", &client.id),
                ("redirect_uri", &self.redirect),
                ("response_type", "code"),
                ("scope", SCOPE),
                ("code_challenge", &challenge),
                ("code_challenge_method", "S256"),
                ("state", &self.state),
                // A refresh token every time, even when the user said yes before.
                ("access_type", "offline"),
                ("prompt", "consent"),
            ])
        )
    }
}

/// What came back on the redirect.
#[derive(Debug, PartialEq)]
enum Reply {
    Code(String),
    Denied(String),
    /// Not the redirect at all: a favicon request, a stray tab.
    Other,
}

fn parse_request(head: &str, state: &str) -> Reply {
    let Some(target) = head.lines().next().and_then(|l| l.strip_prefix("GET ")).and_then(|l| l.split(' ').next())
    else {
        return Reply::Other;
    };
    let Some(query) = target.strip_prefix("/?") else { return Reply::Other };
    let mut code = None;
    let mut error = None;
    let mut state_ok = false;
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        match k {
            "code" => code = Some(decode(v)),
            "error" => error = Some(decode(v)),
            "state" => state_ok = decode(v) == state,
            _ => {}
        }
    }
    if !state_ok {
        return Reply::Other;
    }
    match (code, error) {
        (Some(c), None) if !c.is_empty() => Reply::Code(c),
        (_, Some(e)) => Reply::Denied(e),
        _ => Reply::Other,
    }
}

const PAGE: &str = "<!doctype html><meta charset=utf-8><title>MyTube</title>\
<body style=\"font:16px system-ui;background:#0b0b12;color:#ececf4;display:grid;place-items:center;height:90vh\">";

/// Waits for the browser to come back to the loopback port with a code.
pub async fn wait_for_code(listener: TcpListener, attempt: &Attempt) -> Result<String> {
    loop {
        let (mut sock, _) = listener.accept().await.map_err(|e| Error::msg(format!("Sign-in listener: {e}")))?;
        let mut buf = vec![0u8; 8192];
        let n = match tokio::time::timeout(Duration::from_secs(5), sock.read(&mut buf)).await {
            Ok(Ok(n)) => n,
            _ => continue,
        };
        let head = String::from_utf8_lossy(&buf[..n]);
        let reply = parse_request(&head, &attempt.state);
        let body = match &reply {
            Reply::Code(_) => "<p>Signed in. You can close this tab and go back to MyTube.</p>",
            Reply::Denied(_) => "<p>Sign-in cancelled. You can close this tab.</p>",
            Reply::Other => "",
        };
        let status = if body.is_empty() { "404 Not Found" } else { "200 OK" };
        let page = format!("{PAGE}{body}");
        let resp = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
            page.len()
        );
        let _ = sock.write_all(resp.as_bytes()).await;
        let _ = sock.shutdown().await;
        match reply {
            Reply::Code(c) => return Ok(c),
            Reply::Denied(e) if e == "access_denied" => return Err(Error::msg("Sign-in cancelled in the browser.")),
            Reply::Denied(e) => return Err(Error::msg(format!("Google refused the sign-in: {e}"))),
            Reply::Other => {}
        }
    }
}

#[derive(Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub expires_in: u64,
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub scope: String,
}

#[derive(Deserialize)]
struct TokenError {
    error: String,
    #[serde(default)]
    error_description: String,
}

/// The refresh token is gone: expired, revoked, or the client changed.
pub const SESSION_ENDED: &str = "Google ended the session. Sign in again.";

async fn token_request(http: &reqwest::Client, body: String) -> Result<Tokens> {
    let resp = http
        .post(TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await?;
    let ok = resp.status().is_success();
    let bytes = resp.bytes().await?;
    if ok {
        return serde_json::from_slice(&bytes).map_err(|e| Error::msg(format!("Bad token reply: {e}")));
    }
    let err: TokenError = serde_json::from_slice(&bytes)
        .unwrap_or(TokenError { error: "unknown".into(), error_description: String::from_utf8_lossy(&bytes).into() });
    Err(Error::msg(match err.error.as_str() {
        "invalid_client" | "unauthorized_client" => "Google doesn't accept this Client ID or secret. Check them in Google Cloud → Credentials.".into(),
        "invalid_grant" => SESSION_ENDED.into(),
        "redirect_uri_mismatch" => "This client isn't a Desktop app. Create an OAuth client of type Desktop app.".into(),
        _ if err.error_description.is_empty() => format!("Google: {}", err.error),
        _ => format!("Google: {}", err.error_description),
    }))
}

pub async fn exchange(http: &reqwest::Client, client: &Client, attempt: &Attempt, code: &str) -> Result<Tokens> {
    let body = form(&[
        ("code", code),
        ("client_id", &client.id),
        ("client_secret", &client.secret),
        ("redirect_uri", &attempt.redirect),
        ("grant_type", "authorization_code"),
        ("code_verifier", &attempt.verifier),
    ]);
    token_request(http, body).await
}

pub async fn refresh(http: &reqwest::Client, client: &Client, refresh_token: &str) -> Result<Tokens> {
    let body = form(&[
        ("client_id", &client.id),
        ("client_secret", &client.secret),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ]);
    token_request(http, body).await
}

/// Best effort: signing out locally works even if Google is unreachable.
pub async fn revoke(http: &reqwest::Client, token: &str) {
    let _ = http
        .post(REVOKE_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form(&[("token", token)]))
        .send()
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b64url_matches_rfc4648() {
        assert_eq!(b64url(b""), "");
        assert_eq!(b64url(b"f"), "Zg");
        assert_eq!(b64url(b"fo"), "Zm8");
        assert_eq!(b64url(b"foo"), "Zm9v");
        assert_eq!(b64url(b"foob"), "Zm9vYg");
        assert_eq!(b64url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn pkce_challenge_matches_rfc7636_example() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(b64url(&Sha256::digest(verifier.as_bytes())), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn verifier_is_valid_length() {
        let v = random(32).unwrap();
        assert_eq!(v.len(), 43);
        assert!(v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
    }

    #[test]
    fn encode_decode_round_trip() {
        let s = "4/0Ab cd+é&=/";
        assert_eq!(decode(&encode(s)), s);
        assert_eq!(encode("a b/c"), "a%20b%2Fc");
        assert_eq!(decode("100%"), "100%");
        assert_eq!(decode("%zz"), "%zz");
    }

    #[test]
    fn redirect_parsing() {
        let st = "abc";
        let get = |q: &str| format!("GET {q} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
        assert_eq!(parse_request(&get("/?state=abc&code=4%2F0Ax&scope=x"), st), Reply::Code("4/0Ax".into()));
        assert_eq!(parse_request(&get("/?error=access_denied&state=abc"), st), Reply::Denied("access_denied".into()));
        assert_eq!(parse_request(&get("/?state=evil&code=x"), st), Reply::Other);
        assert_eq!(parse_request(&get("/?code=x"), st), Reply::Other);
        assert_eq!(parse_request(&get("/favicon.ico"), st), Reply::Other);
        assert_eq!(parse_request("garbage", st), Reply::Other);
    }

    #[test]
    fn auth_url_has_pkce_and_scope() {
        let a = Attempt::new(5555).unwrap();
        let url = a.url(&Client { id: "x.apps.googleusercontent.com".into(), secret: "s".into() });
        assert!(url.starts_with(AUTH_URL));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A5555"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains(&format!("scope={}", encode(SCOPE))));
        assert!(!url.contains("secret"));
    }
}
