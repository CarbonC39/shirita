//! Streamable HTTP transport for MCP.
//!
//! JSON or SSE responses, `Mcp-Session-Id` handling, declared request timeouts,
//! redacted diagnostics. No Shirita auth, provider keys, cookies, or unrelated
//! headers are ever forwarded; only the configured static headers are sent.

use futures::StreamExt;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::mcp::parse_result;
use crate::{Error, Result};

pub struct HttpSession {
    client: reqwest::Client,
    url: String,
    headers: Vec<(String, String)>,
    session_id: Mutex<Option<String>>,
}

impl HttpSession {
    pub async fn new(
        url: &str,
        headers: &[(String, String)],
        timeout_ms: u64,
    ) -> Result<HttpSession> {
        // Configured headers are validated against a routing/hop-by-hop
        // denylist; Shirita's own auth is never forwarded automatically.
        for (name, value) in headers {
            validate_header_name(name)?;
            if value.len() > crate::mcp::MCP_MAX_CONFIG_ITEM_BYTES {
                return Err(Error::Mcp("mcp http header value too large".into()));
            }
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms.max(1)))
            // Redirects stay on the original origin so custom secret headers can
            // never be forwarded to a cross-host target.
            .redirect(same_origin_redirect_policy(url))
            .build()
            .map_err(|e| Error::Mcp(format!("mcp http client build failed: {e}")))?;
        validate_url(url).map_err(Error::Mcp)?;
        Ok(HttpSession {
            client,
            url: url.to_string(),
            headers: headers.to_vec(),
            session_id: Mutex::new(None),
        })
    }

    /// Send a JSON-RPC request and return its `result`.
    pub async fn request(&mut self, id: u64, message: &Value) -> Result<Value> {
        let response = self.post(message).await?;
        if response.get("id").and_then(Value::as_u64) == Some(id) {
            return parse_result(&response);
        }
        // A server may respond with an error envelope referencing our id.
        if response.get("error").is_some() && response.get("id").and_then(Value::as_u64) == Some(id)
        {
            return parse_result(&response);
        }
        Err(Error::Mcp(format!("mcp http response id mismatch (wanted {id})")))
    }

    /// Send a JSON-RPC notification. Unlike requests, a notification's response
    /// may be a success with an empty body (the MCP spec does not require a JSON
    /// response for notifications), so it is drained but not parsed.
    pub async fn notify(&self, message: &Value) -> Result<()> {
        let mut builder = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");
        for (key, value) in &self.headers {
            builder = builder.header(key, value);
        }
        if let Some(sid) = self.session_id.lock().await.as_ref() {
            builder = builder.header("Mcp-Session-Id", sid);
        }
        let response = builder
            .body(message.to_string())
            .send()
            .await
            .map_err(|e| Error::Mcp(format!("mcp http notification failed: {e}")))?;
        if !response.status().is_success() {
            return Err(Error::Mcp(format!(
                "mcp http notification returned {}",
                response.status()
            )));
        }
        let _ = read_bounded_body(response, crate::mcp::MCP_MAX_HTTP_BODY_BYTES).await?;
        Ok(())
    }

    async fn post(&self, message: &Value) -> Result<Value> {
        let mut builder = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");
        for (key, value) in &self.headers {
            builder = builder.header(key, value);
        }
        if let Some(sid) = self.session_id.lock().await.as_ref() {
            builder = builder.header("Mcp-Session-Id", sid);
        }
        let response = builder
            .body(message.to_string())
            .send()
            .await
            .map_err(|e| Error::Mcp(format!("mcp http request failed: {e}")))?;

        // Persist a session id if the server issued one.
        if let Some(sid) = response.headers().get("mcp-session-id") {
            if let Ok(s) = sid.to_str() {
                *self.session_id.lock().await = Some(s.to_string());
            }
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = read_bounded_body(response, crate::mcp::MCP_MAX_HTTP_BODY_BYTES).await?;
        if content_type.contains("text/event-stream") {
            let text = String::from_utf8_lossy(&bytes);
            parse_sse(&text)
        } else {
            serde_json::from_slice(&bytes)
                .map_err(|e| Error::Mcp(format!("mcp http response was not JSON: {e}")))
        }
    }
}

/// Parse a JSON-RPC response from an SSE body: `data: <json>` lines, first
/// non-ping data event wins.
fn parse_sse(text: &str) -> Result<Value> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(payload) = line.strip_prefix("data:") {
            let payload = payload.trim();
            if payload == "[DONE]" {
                continue;
            }
            let value: Value = serde_json::from_str(payload)
                .map_err(|e| Error::Mcp(format!("mcp SSE data was not JSON: {e}")))?;
            if value.get("jsonrpc").is_some() {
                return Ok(value);
            }
        }
    }
    Err(Error::Mcp("mcp HTTP SSE stream contained no JSON-RPC message".into()))
}

/// Reject routing/hop-by-hop header names that must never come from user
/// configuration (Shirita's own auth/session handling is applied separately).
pub(crate) fn validate_header_name(name: &str) -> Result<()> {
    let lower = name.to_ascii_lowercase();
    const DENYLIST: &[&str] = &[
        "host",
        "content-length",
        "connection",
        "transfer-encoding",
        "cookie",
        "mcp-session-id",
        "proxy-authorization",
        "proxy-authenticate",
        "upgrade",
        "te",
        "trailer",
        "keep-alive",
        "expect",
    ];
    if DENYLIST.contains(&lower.as_str()) {
        return Err(Error::Mcp(format!(
            "mcp http header '{name}' is reserved or hop-by-hop"
        )));
    }
    if lower.len() > 256 {
        return Err(Error::Mcp("mcp http header name too long".into()));
    }
    Ok(())
}

/// Max same-origin redirects before the request fails (a same-origin redirect
/// loop cannot run to the request timeout).
const MAX_REDIRECTS: usize = 3;

/// Whether a redirect to `url` should be followed: only same-origin, and only
/// within the count bound. Cross-origin redirects stop so configured secret
/// headers are never forwarded to a different host.
pub(crate) fn should_follow_redirect(
    original_origin: &(String, String, Option<u16>),
    url: &str,
    previous_count: usize,
) -> bool {
    previous_count < MAX_REDIRECTS && origin_of(url) == *original_origin
}

/// Redirects stay on the original origin (so configured secret headers are
/// never forwarded to a cross-host target) and are bounded in count so a
/// same-origin redirect loop fails at the declared limit rather than running to
/// the request timeout.
pub(crate) fn same_origin_redirect_policy(original: &str) -> reqwest::redirect::Policy {
    let original_origin = origin_of(original);
    reqwest::redirect::Policy::custom(move |attempt| {
        if should_follow_redirect(&original_origin, attempt.url().as_str(), attempt.previous().len()) {
            attempt.follow()
        } else {
            attempt.stop()
        }
    })
}

fn origin_of(url: &str) -> (String, String, Option<u16>) {
    let parsed = reqwest::Url::parse(url).unwrap_or_else(|_| reqwest::Url::parse("http://localhost").unwrap());
    (
        parsed.scheme().to_string(),
        parsed.host_str().unwrap_or("").to_string(),
        parsed.port(),
    )
}

/// Read a response body up to a byte cap so a malicious server cannot make us
/// buffer an unbounded payload.
async fn read_bounded_body(response: reqwest::Response, cap: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Error::Mcp(format!("mcp http body read failed: {e}")))?;
        if out.len().saturating_add(chunk.len()) > cap {
            return Err(Error::Mcp("mcp http response body exceeded the declared limit".into()));
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

fn parse_url_has_credentials(url: &str) -> bool {
    // Credentials before the last `@` in the authority portion
    // (`scheme://user:pass@host`) are rejected.
    let without_scheme = match url.split_once("://") {
        Some((_, rest)) => rest,
        None => url,
    };
    let authority = without_scheme.split('/').next().unwrap_or(without_scheme);
    authority.contains('@')
}

/// Validate a Streamable HTTP endpoint URL: no embedded credentials, and plain
/// `http://` only for loopback hosts (HTTPS is normal).
pub(crate) fn validate_url(url: &str) -> std::result::Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("mcp http URL must use http or https".into());
    }
    if parse_url_has_credentials(url) {
        return Err("mcp http URL must not embed credentials".into());
    }
    if let Some(rest) = url.strip_prefix("http://") {
        let host = rest.split(['/', ':', '?']).next().unwrap_or("");
        let loopback =
            host == "localhost" || host.starts_with("127.") || host == "::1" || host == "[::1]";
        if !loopback {
            return Err("plain http is only allowed for loopback hosts".into());
        }
    }
    Ok(())
}
