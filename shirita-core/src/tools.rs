//! Trusted Tool registry and the built-in harness/capability Tools.

use async_trait::async_trait;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};

use crate::agent::{
    MAX_FINISH_RESPONSE_BYTES, MAX_MATH_EXPRESSION_BYTES, MAX_MATH_PARSE_DEPTH,
    MAX_RANDOM_INTEGER_SPAN, MAX_RANDOM_ITEMS, MAX_STATUS_MESSAGE_BYTES, MAX_TOOL_ARGUMENT_BYTES,
    MAX_TOOL_RESULT_BYTES,
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
    Finish { response: String },
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

fn rejected(call: &ToolCall, code: &str) -> ToolExecution {
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
        let Some(response) = call
            .arguments
            .get("response")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.len() <= MAX_FINISH_RESPONSE_BYTES)
        else {
            return rejected(call, "invalid_arguments");
        };
        ok(
            call,
            json!({"accepted": true}),
            ToolControl::Finish {
                response: response.into(),
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
                description: "Submit the final user-visible response".into(),
                input_schema: schema(json!({"response":{"type":"string"}}), &["response"]),
                output_schema: None,
                source: source.clone(),
                required: true,
            },
            Arc::new(FinishTool),
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
