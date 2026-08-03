//! One-time authorization for `ask`-policy MCP Tools.
//!
//! A request is bound to the authenticated run (run_id + call_id + session),
//! plus frozen server/Tool/arguments. It can be resolved exactly once; stale or
//! mismatched decisions are rejected. Stop (dropping the waiter) resolves the
//! wait without storing a policy.

use std::collections::HashMap;

use serde_json::Value;
use tokio::sync::{Mutex, oneshot};

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

impl AuthorizationBroker {
    pub fn new() -> Self {
        Self::default()
    }

    fn key(run_id: &str, call_id: &str) -> String {
        format!("{run_id}:{call_id}")
    }

    /// Register a pending request and wait for its one-time decision. A bounded
    /// preview is derived from the frozen arguments. Expires if no decision
    /// arrives within the timeout (or the waiter is cancelled, e.g. Stop).
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
        let (tx, rx) = oneshot::channel();
        let preview = bounded_preview(&arguments);
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
        self.pending.lock().await.insert(Self::key(run_id, call_id), pending);
        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms.max(1)), rx).await {
            Ok(Ok(decision)) => decision,
            Ok(Err(_)) => {
                // The sender was dropped (caller cancelled/stopped).
                self.pending.lock().await.remove(&Self::key(run_id, call_id));
                AuthorizationDecision::Expired
            }
            Err(_) => AuthorizationDecision::Expired,
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
        let mut pending = self.pending.lock().await;
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
            .await
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

/// A bounded, redacted preview of the arguments (compact JSON, capped at 512
/// bytes) — never raw secrets or full argument dumps.
fn bounded_preview(arguments: &Value) -> String {
    let compact = serde_json::to_string(arguments).unwrap_or_else(|_| "{}".into());
    if compact.len() > 512 {
        format!("{}…", &compact[..512])
    } else {
        compact
    }
}
