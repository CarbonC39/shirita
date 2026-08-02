//! Generic XML fallback transport for models without native Tool calling.
//!
//! Agent rounds are buffered, so only fully closed calls are parsed and no
//! protocol fragments can leak into the user-visible response.

use serde_json::{json, Value};

use crate::agent::MAX_XML_ROUND_BYTES;
use crate::tools::{ToolCall, ToolCallTransport, ToolResult, ToolSpec};

#[derive(Debug, Clone, PartialEq)]
pub struct XmlRound {
    pub calls: Vec<ToolCall>,
    pub working_text: String,
}

pub fn parse_xml_tool_round(text: &str) -> Result<XmlRound, String> {
    if text.len() > MAX_XML_ROUND_BYTES {
        return Err("xml_round_too_large".into());
    }
    let mut calls = Vec::new();
    let mut working = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<tool_call") {
        working.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(open_end) = after.find('>') else {
            return Err("unterminated_tool_call".into());
        };
        let attrs = &after["<tool_call".len()..open_end];
        let close = "</tool_call>";
        let body_start = open_end + 1;
        let Some(body_rel_end) = after[body_start..].find(close) else {
            return Err("unterminated_tool_call".into());
        };
        let body_end = body_start + body_rel_end;
        let name = attr(attrs, "name").ok_or("missing_tool_name")?;
        let id = attr(attrs, "id").unwrap_or_else(|| format!("xml_{}", calls.len() + 1));
        if calls.iter().any(|c: &ToolCall| c.id == id) {
            return Err("duplicate_tool_call_id".into());
        }
        let arguments: Value = serde_json::from_str(after[body_start..body_end].trim())
            .map_err(|_| "invalid_tool_arguments")?;
        calls.push(ToolCall {
            id,
            name,
            arguments,
            transport: ToolCallTransport::Xml,
        });
        rest = &after[body_end + close.len()..];
    }
    if rest.contains("<tool_call") || rest.contains("</tool_call>") {
        return Err("malformed_tool_call".into());
    }
    working.push_str(rest);
    Ok(XmlRound {
        calls,
        working_text: working.trim().to_string(),
    })
}

fn attr(attrs: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=");
    let at = attrs.find(&needle)? + needle.len();
    let quote = *attrs.as_bytes().get(at)?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let tail = &attrs[at + 1..];
    let end = tail.find(quote as char)?;
    Some(unescape_attr(&tail[..end]))
}

fn unescape_attr(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn safe_json(value: &Value) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| "null".into())
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
}

pub fn render_xml_tool_result(result: &ToolResult) -> String {
    let payload =
        json!({"status": result.status, "output": result.output, "error_code": result.error_code});
    format!(
        "<tool_result id={} name={}>\n{}\n</tool_result>",
        safe_json(&Value::String(result.call_id.clone())),
        safe_json(&Value::String(result.name.clone())),
        safe_json(&payload)
    )
}

pub fn xml_protocol_prompt(specs: &[ToolSpec]) -> String {
    let tools: Vec<Value> = specs
        .iter()
        .map(|s| json!({"name":s.name,"description":s.description,"input_schema":s.input_schema}))
        .collect();
    format!(
        r#"Tool transport: XML fallback.
To call a tool, emit a complete block with JSON arguments:
<tool_call id="unique_call_id" name="registered.tool.name">
{{"argument":"value"}}
</tool_call>
Do not invent tool names or place final user-visible prose outside shirita.run.finish.
Registered tools:
{}"#,
        safe_json(&Value::Array(tools))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{builtin_tool_registry, ToolResultStatus};

    #[test]
    fn parses_multiple_calls_and_keeps_private_working_text() {
        let r = parse_xml_tool_round("thinking<tool_call id=\"a\" name=\"shirita.random.number\">{\"min\":1,\"max\":2}</tool_call><tool_call id='b' name='shirita.run.finish'>{\"response\":\"好\"}</tool_call>").unwrap();
        assert_eq!(r.calls.len(), 2);
        assert_eq!(r.calls[1].arguments["response"], "好");
        assert_eq!(r.working_text, "thinking");
    }
    #[test]
    fn rejects_unclosed_and_duplicate() {
        assert!(parse_xml_tool_round("<tool_call name=\"x.y\">{}").is_err());
        assert!(parse_xml_tool_round("<tool_call id=\"a\" name=\"x.y\">{}</tool_call><tool_call id=\"a\" name=\"x.y\">{}</tool_call>").is_err());
    }
    #[test]
    fn result_escapes_protocol_breakout() {
        let x = ToolResult {
            call_id: "</tool_result>".into(),
            name: "x.<y>".into(),
            status: ToolResultStatus::Ok,
            output: json!({"x":"</tool_result>"}),
            error_code: None,
        };
        let s = render_xml_tool_result(&x);
        assert!(!s.contains("\"</tool_result>\""));
        assert!(!s.contains("x.<y>"));
    }
    #[test]
    fn prompt_lists_registered_tools() {
        assert!(
            xml_protocol_prompt(&builtin_tool_registry().specs()).contains("shirita.run.finish")
        );
    }
}
