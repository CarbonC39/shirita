//! One-time authorization for `ask`-policy MCP Tools.
//!
//! A request is bound to the authenticated run (run_id + call_id + session),
//! plus frozen server/Tool/arguments. It can be resolved exactly once; stale or
//! mismatched decisions are rejected. Timeout, Stop, or dropping the waiter
//! removes the pending entry (RAII guard), so no zombie requests accumulate.

use std::collections::HashMap;

use serde_json::{Map, Value};
use std::sync::Mutex;
use tokio::sync::oneshot;

/// The outcome of an authorization wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationDecision {
    Approved,
    Denied,
    /// No decision arrived in time / the waiter was cancelled (Stop).
    Expired,
}

/// A serializable summary of a pending request for the prompt UI.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PendingAuthorizationInfo {
    pub run_id: String,
    pub call_id: String,
    pub server_id: String,
    /// The original MCP tool name (before encoding).
    pub tool_name: String,
    /// Bounded, redacted preview.
    pub preview: String,
}

/// A pending, frozen authorization request awaiting a one-time decision.
pub struct PendingAuthorization {
    pub run_id: String,
    pub call_id: String,
    pub session_id: String,
    pub server_id: String,
    /// The original MCP tool name (before encoding).
    pub tool_name: String,
    /// The frozen arguments that will be executed verbatim on approval.
    pub arguments: Value,
    /// A bounded, redacted preview suitable for the prompt UI.
    pub preview: String,
    reply: Option<oneshot::Sender<AuthorizationDecision>>,
}

/// Shared per-app broker for `ask` requests; the web layer resolves decisions
/// through `POST /api/agent-runs/{run}/calls/{call}/approve|deny`.
#[derive(Default)]
pub struct AuthorizationBroker {
    pending: Mutex<HashMap<String, PendingAuthorization>>,
}

/// RAII cleanup: removes the pending entry when the waiter future is dropped
/// (outer Tool timeout, Stop, or any other cancellation), so no request is left
/// behind waiting for a decision nobody will deliver. Idempotent: `resolve()`
/// already removed the entry on approve/deny.
struct PendingGuard<'a> {
    broker: &'a AuthorizationBroker,
    key: String,
}

impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        // The pending mutex is a std Mutex never held across an await, so a
        // deterministic sync lock here cannot deadlock and never fails: the
        // entry is always removed on timeout, Stop, or future drop.
        if let Ok(mut pending) = self.broker.pending.lock() {
            pending.remove(&self.key);
        }
    }
}

impl AuthorizationBroker {
    pub fn new() -> Self {
        Self::default()
    }

    fn key(run_id: &str, call_id: &str) -> String {
        format!("{run_id}:{call_id}")
    }

    /// Register a pending request and wait for its one-time decision. A bounded,
    /// redacted preview is derived from the frozen arguments. Expires if no
    /// decision arrives within the timeout; Stop or dropping the waiter (outer
    /// timeout/cancellation) also resolves the wait and clears the entry.
    pub async fn request_and_wait(
        &self,
        run_id: &str,
        session_id: &str,
        server_id: &str,
        tool_name: &str,
        call_id: &str,
        arguments: Value,
        timeout_ms: u64,
    ) -> AuthorizationDecision {
        let key = Self::key(run_id, call_id);
        let _guard = PendingGuard {
            broker: self,
            key: key.clone(),
        };
        let (tx, rx) = oneshot::channel();
        let preview = bounded_redacted_preview(&arguments);
        let pending = PendingAuthorization {
            run_id: run_id.to_string(),
            call_id: call_id.to_string(),
            session_id: session_id.to_string(),
            server_id: server_id.to_string(),
            tool_name: tool_name.to_string(),
            arguments,
            preview,
            reply: Some(tx),
        };
        let _ = self.pending.lock().unwrap().insert(key, pending);
        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms.max(1)), rx).await {
            Ok(Ok(decision)) => decision,
            // Sender dropped (caller cancelled) or internal timeout: the guard
            // clears the pending entry either way.
            Ok(Err(_)) | Err(_) => AuthorizationDecision::Expired,
        }
    }

    /// Resolve a pending request. Returns `None` if it is stale or mismatched
    /// (already decided, unknown, or the run/call does not match).
    pub async fn resolve(
        &self,
        run_id: &str,
        call_id: &str,
        decision: AuthorizationDecision,
    ) -> Option<PendingAuthorization> {
        let key = Self::key(run_id, call_id);
        let mut pending = self.pending.lock().unwrap();
        let mut entry = pending.remove(&key)?;
        if let Some(reply) = entry.reply.take() {
            let _ = reply.send(decision);
        }
        Some(entry)
    }

    /// Pending requests for a session (for the chat authorization prompt).
    pub async fn pending_for_session(&self, session_id: &str) -> Vec<PendingAuthorizationInfo> {
        self.pending
            .lock()
            .unwrap()
            .values()
            .filter(|p| p.session_id == session_id)
            .map(|p| PendingAuthorizationInfo {
                run_id: p.run_id.clone(),
                call_id: p.call_id.clone(),
                server_id: p.server_id.clone(),
                tool_name: p.tool_name.clone(),
                preview: p.preview.clone(),
            })
            .collect()
    }
}

const PREVIEW_MAX_CHARS: usize = 512;

/// A bounded, redacted preview: sensitive object values are masked and the
/// compact JSON is truncated on a character boundary (never a raw byte slice,
/// which could panic mid-UTF-8).
pub(crate) fn bounded_redacted_preview(arguments: &Value) -> String {
    let redacted = redact(arguments);
    let compact = serde_json::to_string(&redacted).unwrap_or_else(|_| "{}".into());
    let truncated: String = compact.chars().take(PREVIEW_MAX_CHARS).collect();
    if truncated.chars().count() < compact.chars().count() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

/// Recursively mask values under sensitive keys so secrets never reach the UI.
fn redact(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::with_capacity(map.len());
            for (key, v) in map {
                if is_sensitive_key(key) {
                    out.insert(key.clone(), Value::String("[redacted]".into()));
                } else {
                    out.insert(key.clone(), redact(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact).collect()),
        other => other.clone(),
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "password", "token", "secret", "authorization", "auth", "cookie", "credentials",
        "credential", "bearer", "api_key", "apikey", "access_key", "accesskey", "private_key",
        "privatekey", "session", "passwd", "pw",
    ]
    .iter()
    .any(|needle| key == *needle || key.ends_with(needle))
}
