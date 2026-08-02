//! Typed configuration and runtime records for the RP Agent harness.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const HARD_MAX_ROUNDS: u32 = 8;
pub const HARD_MAX_TOOL_CALLS: u32 = 32;
pub const HARD_MAX_TOOL_TIMEOUT_MS: u64 = 30_000;
pub const MAX_TOOL_ARGUMENT_BYTES: usize = 64 * 1024;
pub const MAX_TOOL_RESULT_BYTES: usize = 64 * 1024;
pub const MAX_FINISH_RESPONSE_BYTES: usize = 1024 * 1024;
/// XML fallback buffers one private model round so incomplete protocol markup
/// cannot be exposed as conversation content.
pub const MAX_XML_ROUND_BYTES: usize = 256 * 1024;
pub const MAX_AGENT_PROMPT_BYTES: usize = 64 * 1024;
pub const MAX_STATUS_MESSAGE_BYTES: usize = 4 * 1024;
pub const MAX_RANDOM_ITEMS: usize = 1_000;
pub const MAX_RANDOM_INTEGER_SPAN: i128 = 1_000_000_000;
pub const MAX_MATH_EXPRESSION_BYTES: usize = 4 * 1024;
pub const MAX_MATH_PARSE_DEPTH: usize = 64;
/// Single response workspace ceiling (the one user-visible response a run builds).
pub const MAX_RESPONSE_WORKSPACE_BYTES: usize = 256 * 1024;
/// Maximum operations in one atomic `shirita.response.patch` batch.
pub const MAX_RESPONSE_PATCH_OPS: usize = 16;
/// Total bytes across all `search` strings in one patch batch.
pub const MAX_RESPONSE_PATCH_SEARCH_BYTES: usize = 8 * 1024;

pub const DEFAULT_SYSTEM_PROMPT: &str = r#"You are operating inside Shirita's text-generation harness.
Use the registered tools when they help you produce the response requested by the user and the conversation prompt. Tool schemas are authoritative.
Ordinary model text and reasoning are private working output. They are not returned to the user.
The current user-visible response is supplied every round as a response workspace with a revision number. Ordinary output does not change it.
Use shirita.response.replace to create or replace the response, and shirita.response.patch to revise it atomically. Tool results become visible only in the next round.
Use shirita.run.update_status only for transient status: visibility=user may be shown while you work; internal status remains private.
Only shirita.run.finish returns text to the user and stores it. Call finish() to commit the current response workspace, or finish with a response field for a one-shot response. Calls after finish are ignored."#;

pub const DEFAULT_UNFINISHED_PROMPT: &str = "Continue working. Tool results from this round are visible now. When the response is ready, call shirita.run.finish() to commit the response workspace, or shirita.run.finish with the final text as a one-shot response.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolTransport {
    #[default]
    Auto,
    Native,
    Xml,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NativeToolCapability {
    #[default]
    Auto,
    Supported,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentSettings {
    pub enabled: bool,
    pub transport: ToolTransport,
    pub enabled_tools: Vec<String>,
    pub max_rounds: u32,
    pub max_tool_calls: u32,
    pub tool_timeout_ms: u64,
    pub show_activity: bool,
    pub show_user_status: bool,
    pub max_identical_call_rounds: u32,
    pub system_prompt: String,
    pub unfinished_prompt: String,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            transport: ToolTransport::Auto,
            enabled_tools: vec![
                "shirita.random.number".into(),
                "shirita.random.choose".into(),
                "shirita.math.evaluate".into(),
            ],
            max_rounds: 4,
            max_tool_calls: 8,
            tool_timeout_ms: 5_000,
            show_activity: true,
            show_user_status: true,
            max_identical_call_rounds: 3,
            system_prompt: DEFAULT_SYSTEM_PROMPT.into(),
            unfinished_prompt: DEFAULT_UNFINISHED_PROMPT.into(),
        }
    }
}

impl AgentSettings {
    /// Runtime defense-in-depth for settings written through legacy/generic
    /// paths. Persistence validation is useful UX, but never a safety boundary.
    pub fn sanitized(mut self, registered_tools: &[String]) -> Self {
        self.max_rounds = self.max_rounds.clamp(1, HARD_MAX_ROUNDS);
        self.max_tool_calls = self.max_tool_calls.clamp(1, HARD_MAX_TOOL_CALLS);
        self.tool_timeout_ms = self.tool_timeout_ms.clamp(1, HARD_MAX_TOOL_TIMEOUT_MS);
        self.max_identical_call_rounds = self.max_identical_call_rounds.clamp(1, self.max_rounds);
        self.enabled_tools.retain(|name| registered_tools.contains(name));
        self.enabled_tools.sort();
        self.enabled_tools.dedup();
        if self.system_prompt.trim().is_empty() || self.system_prompt.len() > MAX_AGENT_PROMPT_BYTES {
            self.system_prompt = DEFAULT_SYSTEM_PROMPT.into();
        }
        if self.unfinished_prompt.trim().is_empty() || self.unfinished_prompt.len() > MAX_AGENT_PROMPT_BYTES {
            self.unfinished_prompt = DEFAULT_UNFINISHED_PROMPT.into();
        }
        self
    }

    pub fn validate(&self, registered_tools: &[String]) -> Result<(), String> {
        if self.max_rounds == 0 || self.max_rounds > HARD_MAX_ROUNDS {
            return Err(format!(
                "max_rounds must be between 1 and {HARD_MAX_ROUNDS}"
            ));
        }
        if self.max_tool_calls == 0 || self.max_tool_calls > HARD_MAX_TOOL_CALLS {
            return Err(format!(
                "max_tool_calls must be between 1 and {HARD_MAX_TOOL_CALLS}"
            ));
        }
        if self.tool_timeout_ms == 0 || self.tool_timeout_ms > HARD_MAX_TOOL_TIMEOUT_MS {
            return Err(format!(
                "tool_timeout_ms must be between 1 and {HARD_MAX_TOOL_TIMEOUT_MS}"
            ));
        }
        if self.max_identical_call_rounds == 0 || self.max_identical_call_rounds > self.max_rounds {
            return Err("max_identical_call_rounds must be between 1 and max_rounds".into());
        }
        if self.system_prompt.trim().is_empty() || self.unfinished_prompt.trim().is_empty() {
            return Err("agent prompts must not be empty".into());
        }
        if self.system_prompt.len() > MAX_AGENT_PROMPT_BYTES
            || self.unfinished_prompt.len() > MAX_AGENT_PROMPT_BYTES
        {
            return Err(format!(
                "agent prompts must not exceed {MAX_AGENT_PROMPT_BYTES} bytes"
            ));
        }
        for name in &self.enabled_tools {
            if !registered_tools.contains(name) {
                return Err(format!("unknown tool: {name}"));
            }
        }
        Ok(())
    }

    pub fn from_settings_map(map: &serde_json::Map<String, Value>) -> Self {
        let defaults = Self::default();
        let mut out = defaults.clone();
        if let Some(v) = map.get("agent.enabled").and_then(Value::as_bool) {
            out.enabled = v;
        }
        if let Some(v) = map.get("agent.transport").and_then(Value::as_str) {
            if let Ok(x) = serde_json::from_value(Value::String(v.into())) {
                out.transport = x;
            }
        }
        if let Some(v) = map.get("agent.enabled_tools") {
            if let Ok(x) = serde_json::from_value(v.clone()) {
                out.enabled_tools = x;
            }
        }
        macro_rules! num {
            ($key:literal, $field:ident, $ty:ty) => {
                if let Some(v) = map.get($key).and_then(Value::as_u64) {
                    out.$field = v as $ty;
                }
            };
        }
        num!("agent.max_rounds", max_rounds, u32);
        num!("agent.max_tool_calls", max_tool_calls, u32);
        num!("agent.tool_timeout_ms", tool_timeout_ms, u64);
        num!(
            "agent.max_identical_call_rounds",
            max_identical_call_rounds,
            u32
        );
        if let Some(v) = map.get("agent.show_activity").and_then(Value::as_bool) {
            out.show_activity = v;
        }
        if let Some(v) = map.get("agent.show_user_status").and_then(Value::as_bool) {
            out.show_user_status = v;
        }
        if let Some(v) = map.get("agent.system_prompt").and_then(Value::as_str) {
            out.system_prompt = v.into();
        }
        if let Some(v) = map.get("agent.unfinished_prompt").and_then(Value::as_str) {
            out.unfinished_prompt = v.into();
        }
        out
    }

    pub fn to_settings_map(&self) -> serde_json::Map<String, Value> {
        let v = serde_json::to_value(self).expect("AgentSettings serializes");
        v.as_object()
            .expect("settings is object")
            .iter()
            .map(|(k, v)| (format!("agent.{k}"), v.clone()))
            .collect()
    }
}

pub fn session_agent_override(override_config: &Value) -> Option<AgentSettings> {
    override_config
        .get("agent")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

pub fn effective_agent_settings(global: &AgentSettings, override_config: &Value) -> AgentSettings {
    session_agent_override(override_config).unwrap_or_else(|| global.clone())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    Send,
    Regenerate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Completed,
    Stopped,
    Failed,
}

/// The run-owned response workspace. Only `shirita.response.*` controls and a
/// successful `finish` may change it; ordinary model output never does.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseWorkspace {
    pub text: String,
    pub revision: u64,
}

/// One literal search/replace step in an atomic response patch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsePatchOperation {
    pub search: String,
    pub replace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationRun {
    pub id: String,
    pub session_id: String,
    pub parent_message_id: Option<String>,
    pub kind: RunKind,
    pub round: u32,
    pub tool_calls: u32,
    pub status: RunStatus,
    /// The mutable response workspace, owned by this run.
    #[serde(default)]
    pub workspace: ResponseWorkspace,
}

impl GenerationRun {
    pub fn new(id: &str, session_id: &str, parent_message_id: Option<&str>, kind: RunKind) -> Self {
        Self {
            id: id.into(),
            session_id: session_id.into(),
            parent_message_id: parent_message_id.map(str::to_string),
            kind,
            round: 0,
            tool_calls: 0,
            status: RunStatus::Running,
            workspace: ResponseWorkspace::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_compatible_and_bounded() {
        let d = AgentSettings::default();
        assert!(!d.enabled);
        assert!(d.validate(&d.enabled_tools).is_ok());
    }

    #[test]
    fn override_inherits_or_replaces_as_a_block() {
        let mut custom = AgentSettings::default();
        custom.enabled = true;
        custom.max_rounds = 2;
        let global = AgentSettings::default();
        assert_eq!(
            effective_agent_settings(&global, &serde_json::json!({})),
            global
        );
        assert_eq!(
            effective_agent_settings(&global, &serde_json::json!({"agent": custom})).max_rounds,
            2
        );
    }

    #[test]
    fn rejects_unknown_tools_and_limits() {
        let mut d = AgentSettings::default();
        d.enabled_tools.push("missing.tool".into());
        assert!(d
            .validate(&AgentSettings::default().enabled_tools)
            .unwrap_err()
            .contains("unknown tool"));
        d = AgentSettings::default();
        d.max_rounds = HARD_MAX_ROUNDS + 1;
        assert!(d.validate(&d.enabled_tools).is_err());
    }

    #[test]
    fn runtime_sanitization_enforces_hard_ceilings() {
        let mut d = AgentSettings::default();
        d.max_rounds = 999;
        d.max_tool_calls = 999;
        d.tool_timeout_ms = u64::MAX;
        d.enabled_tools.push("unknown.tool".into());
        let d = d.sanitized(&AgentSettings::default().enabled_tools);
        assert_eq!(d.max_rounds, HARD_MAX_ROUNDS);
        assert_eq!(d.max_tool_calls, HARD_MAX_TOOL_CALLS);
        assert_eq!(d.tool_timeout_ms, HARD_MAX_TOOL_TIMEOUT_MS);
        assert!(!d.enabled_tools.contains(&"unknown.tool".into()));
    }
}
