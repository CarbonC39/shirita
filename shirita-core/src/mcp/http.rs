//! Streamable HTTP transport for MCP.
//!
//! JSON or SSE responses, `Mcp-Session-Id` handling, declared request timeouts,
//! redacted diagnostics. No Shirita auth, provider keys, cookies, or unrelated
//! headers are ever forwarded; only the configured static headers are sent.

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
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms.max(1)))
            // Bounded redirects; never follow to an arbitrary host.
            .redirect(reqwest::redirect::Policy::limited(3))
            .build()
            .map_err(|e| Error::Mcp(format!("mcp http client build failed: {e}")))?;
        // Embedded credentials (user:pass@host) are rejected.
        if parse_url_has_credentials(url) {
            return Err(Error::Mcp("mcp http URL must not embed credentials".into()));
        }
        // Plain HTTP is only accepted for loopback hosts; HTTPS is normal.
        if let Err(message) = plain_http_loopback_only(url) {
            return Err(Error::Mcp(message));
        }
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

    /// Send a JSON-RPC notification; the response body is drained and ignored.
    pub async fn notify(&self, message: &Value) -> Result<()> {
        let _ = self.post(message).await?;
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
        let bytes = response
            .bytes()
            .await
            .map_err(|e| Error::Mcp(format!("mcp http response read failed: {e}")))?;
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

/// Plain `http://` is accepted only for loopback hosts; HTTPS is normal.
fn plain_http_loopback_only(url: &str) -> std::result::Result<(), String> {
    let Some(rest) = url.strip_prefix("http://") else {
        return Ok(());
    };
    let host = rest.split(['/', ':', '?']).next().unwrap_or("");
    let loopback = host == "localhost"
        || host.starts_with("127.")
        || host == "::1"
        || host == "[::1]"
        || host == "[::1]";
    if !loopback {
        return Err("plain http is only allowed for loopback hosts".into());
    }
    Ok(())
}
