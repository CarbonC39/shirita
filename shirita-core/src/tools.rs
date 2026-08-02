//! Trusted Tool registry and the built-in harness/capability Tools.

use async_trait::async_trait;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};

use crate::agent::{
    MAX_MATH_EXPRESSION_BYTES, MAX_MATH_PARSE_DEPTH, MAX_RANDOM_INTEGER_SPAN, MAX_RANDOM_ITEMS,
    MAX_RESPONSE_PATCH_OPS, MAX_RESPONSE_PATCH_REPLACE_BYTES, MAX_RESPONSE_PATCH_SEARCH_BYTES,
    MAX_RESPONSE_WORKSPACE_BYTES, MAX_STATUS_MESSAGE_BYTES, MAX_TOOL_ARGUMENT_BYTES,
    MAX_TOOL_RESULT_BYTES, ResponsePatchOperation,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    Builtin,
    Plugin,
    Mcp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Option<Value>,
    pub source: ToolSource,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallTransport {
    Native,
    Xml,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    pub transport: ToolCallTransport,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub name: String,
    pub status: ToolResultStatus,
    pub output: Value,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolResultStatus {
    Ok,
    Rejected,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolControl {
    None,
    Status { message: String, user_visible: bool },
    ReplaceResponse { text: String },
    PatchResponse { revision: u64, operations: Vec<ResponsePatchOperation> },
    Finish { response: Option<String> },
}

/// Whether a normalized Tool name is a response-control capability (whose
/// call/result pair is compacted into a bounded receipt after execution).
pub fn is_response_control(name: &str) -> bool {
    name == "shirita.response.replace" || name == "shirita.response.patch"
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolExecution {
    pub result: ToolResult,
    pub control: ToolControl,
}

#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn execute(&self, call: &ToolCall) -> ToolExecution;
}

pub struct ToolRegistry {
    specs: HashMap<String, ToolSpec>,
    handlers: HashMap<String, Arc<dyn ToolHandler>>,
}

impl ToolRegistry {
    pub fn builder() -> ToolRegistryBuilder {
        ToolRegistryBuilder::default()
    }
    pub fn spec(&self, name: &str) -> Option<&ToolSpec> {
        self.specs.get(name)
    }
    pub fn specs(&self) -> Vec<ToolSpec> {
        let mut out: Vec<_> = self.specs.values().cloned().collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }
    pub fn capability_names(&self) -> Vec<String> {
        self.specs()
            .into_iter()
            .filter(|s| !s.required)
            .map(|s| s.name)
            .collect()
    }

    pub async fn execute(&self, call: &ToolCall, enabled: &[String]) -> ToolExecution {
        if serde_json::to_vec(&call.arguments)
            .map(|x| x.len())
            .unwrap_or(usize::MAX)
            > MAX_TOOL_ARGUMENT_BYTES
        {
            return rejected(call, "arguments_too_large");
        }
        let Some(spec) = self.specs.get(&call.name) else {
            return rejected(call, "unknown_tool");
        };
        if !spec.required && !enabled.contains(&call.name) {
            return rejected(call, "disabled_tool");
        }
        let Some(handler) = self.handlers.get(&call.name) else {
            return rejected(call, "unknown_tool");
        };
        let mut out = handler.execute(call).await;
        if serde_json::to_vec(&out.result.output)
            .map(|x| x.len())
            .unwrap_or(usize::MAX)
            > MAX_TOOL_RESULT_BYTES
        {
            out = rejected(call, "result_too_large");
        }
        out
    }
}

#[derive(Default)]
pub struct ToolRegistryBuilder {
    specs: HashMap<String, ToolSpec>,
    handlers: HashMap<String, Arc<dyn ToolHandler>>,
}

impl ToolRegistryBuilder {
    pub fn register(
        mut self,
        spec: ToolSpec,
        handler: Arc<dyn ToolHandler>,
    ) -> Result<Self, String> {
        if !valid_name(&spec.name) {
            return Err(format!("invalid tool name: {}", spec.name));
        }
        if self.specs.contains_key(&spec.name) {
            return Err(format!("duplicate tool: {}", spec.name));
        }
        self.handlers.insert(spec.name.clone(), handler);
        self.specs.insert(spec.name.clone(), spec);
        Ok(self)
    }
    pub fn build(self) -> ToolRegistry {
        ToolRegistry {
            specs: self.specs,
            handlers: self.handlers,
        }
    }
}

fn valid_name(name: &str) -> bool {
    name.contains('.')
        && name.split('.').all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
}

pub(crate) fn rejected(call: &ToolCall, code: &str) -> ToolExecution {
    ToolExecution {
        result: ToolResult {
            call_id: call.id.clone(),
            name: call.name.clone(),
            status: ToolResultStatus::Rejected,
            output: json!({}),
            error_code: Some(code.into()),
        },
        control: ToolControl::None,
    }
}
fn ok(call: &ToolCall, output: Value, control: ToolControl) -> ToolExecution {
    ToolExecution {
        result: ToolResult {
            call_id: call.id.clone(),
            name: call.name.clone(),
            status: ToolResultStatus::Ok,
            output,
            error_code: None,
        },
        control,
    }
}

struct StatusTool;
#[async_trait]
impl ToolHandler for StatusTool {
    async fn execute(&self, call: &ToolCall) -> ToolExecution {
        let Some(message) = call
            .arguments
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.len() <= MAX_STATUS_MESSAGE_BYTES)
        else {
            return rejected(call, "invalid_arguments");
        };
        let visibility = call
            .arguments
            .get("visibility")
            .and_then(Value::as_str)
            .unwrap_or("internal");
        if visibility != "internal" && visibility != "user" {
            return rejected(call, "invalid_arguments");
        }
        ok(
            call,
            json!({"accepted": true}),
            ToolControl::Status {
                message: message.into(),
                user_visible: visibility == "user",
            },
        )
    }
}
struct FinishTool;
#[async_trait]
impl ToolHandler for FinishTool {
    async fn execute(&self, call: &ToolCall) -> ToolExecution {
        // `finish()` (commit the workspace) or `finish({"response": "..."})`
        // (replace the workspace then commit). The empty-workspace rejection is
        // a run-state check performed by the loop, not here.
        let response = match call.arguments.get("response") {
            None | Some(Value::Null) => None,
            Some(value) => {
                let Some(s) = value
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty() && s.len() <= MAX_RESPONSE_WORKSPACE_BYTES)
                else {
                    return rejected(call, "invalid_arguments");
                };
                Some(s.to_string())
            }
        };
        ok(call, json!({}), ToolControl::Finish { response })
    }
}

struct ReplaceResponseTool;
#[async_trait]
impl ToolHandler for ReplaceResponseTool {
    async fn execute(&self, call: &ToolCall) -> ToolExecution {
        let Some(text) = call.arguments.get("text").and_then(Value::as_str) else {
            return rejected(call, "invalid_arguments");
        };
        if text.len() > MAX_RESPONSE_WORKSPACE_BYTES {
            return rejected(call, "response_too_large");
        }
        ok(
            call,
            json!({}),
            ToolControl::ReplaceResponse {
                text: text.to_string(),
            },
        )
    }
}

struct PatchResponseTool;
#[async_trait]
impl ToolHandler for PatchResponseTool {
    async fn execute(&self, call: &ToolCall) -> ToolExecution {
        let Some(revision) = call.arguments.get("revision").and_then(Value::as_u64) else {
            return rejected(call, "invalid_arguments");
        };
        let Some(ops) = call.arguments.get("operations").and_then(Value::as_array) else {
            return rejected(call, "invalid_arguments");
        };
        if ops.is_empty() {
            return rejected(call, "empty_patch");
        }
        if ops.len() > MAX_RESPONSE_PATCH_OPS {
            return rejected(call, "too_many_operations");
        }
        let mut operations = Vec::with_capacity(ops.len());
        let mut search_bytes = 0usize;
        let mut replace_bytes = 0usize;
        for op in ops {
            let (Some(search), Some(replace)) = (
                op.get("search").and_then(Value::as_str),
                op.get("replace").and_then(Value::as_str),
            ) else {
                return rejected(call, "invalid_arguments");
            };
            if search.is_empty() {
                return rejected(call, "invalid_arguments");
            }
            search_bytes = search_bytes.saturating_add(search.len());
            replace_bytes = replace_bytes.saturating_add(replace.len());
            operations.push(ResponsePatchOperation {
                search: search.to_string(),
                replace: replace.to_string(),
            });
        }
        if search_bytes > MAX_RESPONSE_PATCH_SEARCH_BYTES
            || replace_bytes > MAX_RESPONSE_PATCH_REPLACE_BYTES
        {
            return rejected(call, "response_too_large");
        }
        ok(
            call,
            json!({}),
            ToolControl::PatchResponse {
                revision,
                operations,
            },
        )
    }
}
struct RandomNumberTool;
#[async_trait]
impl ToolHandler for RandomNumberTool {
    async fn execute(&self, call: &ToolCall) -> ToolExecution {
        let (Some(min), Some(max)) = (
            call.arguments.get("min").and_then(Value::as_i64),
            call.arguments.get("max").and_then(Value::as_i64),
        ) else {
            return rejected(call, "invalid_arguments");
        };
        if min > max || (max as i128 - min as i128) > MAX_RANDOM_INTEGER_SPAN {
            return rejected(call, "invalid_range");
        }
        let value = rand::thread_rng().gen_range(min..=max);
        ok(call, json!({"value": value}), ToolControl::None)
    }
}
struct RandomChooseTool;
#[async_trait]
impl ToolHandler for RandomChooseTool {
    async fn execute(&self, call: &ToolCall) -> ToolExecution {
        let Some(items) = call
            .arguments
            .get("items")
            .and_then(Value::as_array)
            .filter(|x| !x.is_empty() && x.len() <= MAX_RANDOM_ITEMS)
        else {
            return rejected(call, "invalid_arguments");
        };
        let index = rand::thread_rng().gen_range(0..items.len());
        ok(
            call,
            json!({"index": index, "value": items[index]}),
            ToolControl::None,
        )
    }
}
struct MathTool;
#[async_trait]
impl ToolHandler for MathTool {
    async fn execute(&self, call: &ToolCall) -> ToolExecution {
        let Some(expr) = call
            .arguments
            .get("expression")
            .and_then(Value::as_str)
            .filter(|s| s.len() <= MAX_MATH_EXPRESSION_BYTES)
        else {
            return rejected(call, "invalid_arguments");
        };
        match eval_math(expr) {
            Some(v) if v.is_finite() => ok(call, json!({"value": v}), ToolControl::None),
            _ => rejected(call, "invalid_expression"),
        }
    }
}

fn schema(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object", "properties":properties, "required":required, "additionalProperties":false})
}
pub fn builtin_tool_registry() -> ToolRegistry {
    let source = ToolSource::Builtin;
    ToolRegistry::builder()
        .register(
            ToolSpec {
                name: "shirita.run.update_status".into(),
                description: "Update transient run status".into(),
                input_schema: schema(
                    json!({"message":{"type":"string"},"visibility":{"enum":["internal","user"]}}),
                    &["message"],
                ),
                output_schema: None,
                source: source.clone(),
                required: true,
            },
            Arc::new(StatusTool),
        )
        .unwrap()
        .register(
            ToolSpec {
                name: "shirita.run.finish".into(),
                description: "Commit the current response workspace, or supply a final response".into(),
                input_schema: schema(json!({"response":{"type":"string"}}), &[]),
                output_schema: None,
                source: source.clone(),
                required: true,
            },
            Arc::new(FinishTool),
        )
        .unwrap()
        .register(
            ToolSpec {
                name: "shirita.response.replace".into(),
                description: "Replace the response workspace with a complete new draft".into(),
                input_schema: schema(json!({"text":{"type":"string"}}), &["text"]),
                output_schema: None,
                source: source.clone(),
                required: true,
            },
            Arc::new(ReplaceResponseTool),
        )
        .unwrap()
        .register(
            ToolSpec {
                name: "shirita.response.patch".into(),
                description: "Atomically apply ordered literal search/replace edits to the response workspace".into(),
                input_schema: schema(
                    json!({
                        "revision":{"type":"integer"},
                        "operations":{"type":"array","minItems":1,"maxItems":MAX_RESPONSE_PATCH_OPS,"items":{
                            "type":"object",
                            "properties":{"search":{"type":"string"},"replace":{"type":"string"}},
                            "required":["search","replace"]
                        }}
                    }),
                    &["revision", "operations"],
                ),
                output_schema: None,
                source: source.clone(),
                required: true,
            },
            Arc::new(PatchResponseTool),
        )
        .unwrap()
        .register(
            ToolSpec {
                name: "shirita.random.number".into(),
                description: "Choose a random integer in an inclusive range".into(),
                input_schema: schema(
                    json!({"min":{"type":"integer"},"max":{"type":"integer"}}),
                    &["min", "max"],
                ),
                output_schema: None,
                source: source.clone(),
                required: false,
            },
            Arc::new(RandomNumberTool),
        )
        .unwrap()
        .register(
            ToolSpec {
                name: "shirita.random.choose".into(),
                description: "Choose one item from a non-empty list".into(),
                input_schema: schema(
                    json!({"items":{"type":"array","minItems":1,"maxItems":MAX_RANDOM_ITEMS}}),
                    &["items"],
                ),
                output_schema: None,
                source: source.clone(),
                required: false,
            },
            Arc::new(RandomChooseTool),
        )
        .unwrap()
        .register(
            ToolSpec {
                name: "shirita.math.evaluate".into(),
                description: "Evaluate bounded arithmetic using +, -, *, /, %, ^ and parentheses"
                    .into(),
                input_schema: schema(json!({"expression":{"type":"string"}}), &["expression"]),
                output_schema: None,
                source,
                required: false,
            },
            Arc::new(MathTool),
        )
        .unwrap()
        .build()
}

fn eval_math(input: &str) -> Option<f64> {
    struct P<'a> {
        b: &'a [u8],
        i: usize,
        depth: usize,
    }
    impl<'a> P<'a> {
        fn ws(&mut self) {
            while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
        }
        fn eat(&mut self, c: u8) -> bool {
            self.ws();
            if self.b.get(self.i) == Some(&c) {
                self.i += 1;
                true
            } else {
                false
            }
        }
        fn expr(&mut self) -> Option<f64> {
            let mut v = self.term()?;
            loop {
                if self.eat(b'+') {
                    v += self.term()?
                } else if self.eat(b'-') {
                    v -= self.term()?
                } else {
                    return Some(v);
                }
            }
        }
        fn term(&mut self) -> Option<f64> {
            let mut v = self.power()?;
            loop {
                if self.eat(b'*') {
                    v *= self.power()?
                } else if self.eat(b'/') {
                    let x = self.power()?;
                    if x == 0.0 {
                        return None;
                    }
                    v /= x
                } else if self.eat(b'%') {
                    let x = self.power()?;
                    if x == 0.0 {
                        return None;
                    }
                    v %= x
                } else {
                    return Some(v);
                }
            }
        }
        fn power(&mut self) -> Option<f64> {
            let v = self.unary()?;
            if self.eat(b'^') {
                Some(v.powf(self.power()?))
            } else {
                Some(v)
            }
        }
        fn unary(&mut self) -> Option<f64> {
            if self.eat(b'+') {
                self.unary()
            } else if self.eat(b'-') {
                Some(-self.unary()?)
            } else {
                self.atom()
            }
        }
        fn atom(&mut self) -> Option<f64> {
            self.ws();
            if self.eat(b'(') {
                self.depth += 1;
                if self.depth > MAX_MATH_PARSE_DEPTH {
                    return None;
                }
                let v = self.expr()?;
                if !self.eat(b')') {
                    return None;
                }
                self.depth -= 1;
                return Some(v);
            }
            let s = self.i;
            while self.i < self.b.len()
                && (self.b[self.i].is_ascii_digit() || self.b[self.i] == b'.')
            {
                self.i += 1
            }
            if s == self.i {
                return None;
            }
            std::str::from_utf8(&self.b[s..self.i]).ok()?.parse().ok()
        }
    }
    if input.len() > MAX_MATH_EXPRESSION_BYTES {
        return None;
    }
    let mut p = P {
        b: input.as_bytes(),
        i: 0,
        depth: 0,
    };
    let v = p.expr()?;
    p.ws();
    (p.i == p.b.len() && v.is_finite()).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn call(name: &str, args: Value) -> ToolCall {
        ToolCall {
            id: "c".into(),
            name: name.into(),
            arguments: args,
            transport: ToolCallTransport::Native,
        }
    }
    #[tokio::test]
    async fn finish_and_status_are_control_tools() {
        let r = builtin_tool_registry();
        let f = r
            .execute(&call("shirita.run.finish", json!({"response":"hi"})), &[])
            .await;
        assert!(matches!(f.control, ToolControl::Finish { .. }));
        let s = r
            .execute(
                &call(
                    "shirita.run.update_status",
                    json!({"message":"x","visibility":"user"}),
                ),
                &[],
            )
            .await;
        assert!(matches!(
            s.control,
            ToolControl::Status {
                user_visible: true,
                ..
            }
        ));
    }
    #[tokio::test]
    async fn registry_rejects_unknown_and_disabled() {
        let r = builtin_tool_registry();
        assert_eq!(
            r.execute(&call("no.tool", json!({})), &[])
                .await
                .result
                .error_code
                .as_deref(),
            Some("unknown_tool")
        );
        assert_eq!(
            r.execute(
                &call("shirita.math.evaluate", json!({"expression":"1+2"})),
                &[]
            )
            .await
            .result
            .error_code
            .as_deref(),
            Some("disabled_tool")
        );
    }
    #[tokio::test]
    async fn response_tools_validate_schemas_and_bounds() {
        let r = builtin_tool_registry();
        let bad_patch = r
            .execute(
                &call("shirita.response.patch", json!({"revision": 0, "operations": []})),
                &[],
            )
            .await;
        assert_eq!(bad_patch.result.error_code.as_deref(), Some("empty_patch"));
        let too_many = r
            .execute(
                &call(
                    "shirita.response.patch",
                    json!({"revision": 0, "operations": (0..crate::agent::MAX_RESPONSE_PATCH_OPS + 1)
                        .map(|i| json!({"search": format!("s{i}"), "replace": "x"}))
                        .collect::<Vec<_>>()}),
                ),
                &[],
            )
            .await;
        assert_eq!(too_many.result.error_code.as_deref(), Some("too_many_operations"));
        // The serialized-argument ceiling (64 KiB) is tighter than the response
        // workspace ceiling, so an over-sized draft is rejected by the registry's
        // layered argument check before it could become workspace text.
        let oversized = r
            .execute(
                &call(
                    "shirita.response.replace",
                    json!({"text": "x".repeat(crate::agent::MAX_RESPONSE_WORKSPACE_BYTES + 1)}),
                ),
                &[],
            )
            .await;
        assert_eq!(oversized.result.error_code.as_deref(), Some("arguments_too_large"));
        let ok = r
            .execute(&call("shirita.response.replace", json!({"text": "hi"})), &[])
            .await;
        assert!(matches!(ok.control, ToolControl::ReplaceResponse { .. }));
    }

    #[tokio::test]
    async fn math_is_bounded_not_eval() {
        let r = builtin_tool_registry();
        let e = vec!["shirita.math.evaluate".into()];
        assert_eq!(
            r.execute(
                &call("shirita.math.evaluate", json!({"expression":"2*(3+4)"})),
                &e
            )
            .await
            .result
            .output["value"],
            14.0
        );
        assert!(r
            .execute(
                &call("shirita.math.evaluate", json!({"expression":"system(1)"})),
                &e
            )
            .await
            .result
            .error_code
            .is_some());
    }
}
