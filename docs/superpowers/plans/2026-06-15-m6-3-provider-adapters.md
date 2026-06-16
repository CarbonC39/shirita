# M6 Plan 3 — 多 Provider 适配 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让各 `ModelProvider` 自行决定 `ChatRequest.summary` 放进请求体的哪里——`OpenAiProvider`（含 Ollama 兼容端点）把摘要拼进 system；新增 `AnthropicProvider`（`/v1/messages`、顶层 `system`、摘要作 `<history_summary>` 插到首条 user 开头、SSE `content_block_delta` 解析）；`main.rs` 按 `PROVIDER` env 选择。

**Architecture:** 把"消息体构造"抽成可单测纯函数：`openai_messages(&ChatRequest) -> Vec<Value>`、`anthropic_body(&ChatRequest) -> Value`、`parse_anthropic_delta(&str) -> Result<Option<String>>`；`stream_chat` 同构现有 `OpenAiProvider`（reqwest + 手动逐行 SSE）。Anthropic 的 `messages` 必须 user/assistant 交替，故摘要 prepend 到第一条 user 而非另起一条 user。

**Tech Stack:** Rust、reqwest（流式）、async-trait、futures、async-stream、serde_json。依赖 Plan 1 的 `ChatRequest.summary`。

**Upstream spec:** `docs/superpowers/specs/2026-06-15-m6-context-engineering-design.md`（§4 多 Provider 适配）。

---

## File Structure

- `shirita-core/src/model/openai.rs` — **modify**：抽 `openai_messages` 纯函数（含 `summary` 拼 system）+ 单测。
- `shirita-core/src/model/anthropic.rs` — **create**：`AnthropicProvider` + `anthropic_body` + `parse_anthropic_delta` + 单测。
- `shirita-core/src/model/mod.rs` — **modify**：`pub mod anthropic;` + `pub use anthropic::AnthropicProvider;`。
- `shirita-core/src/lib.rs` — **modify**：re-export `AnthropicProvider`。
- `shirita-web/src/main.rs` — **modify**：按 `PROVIDER` env 选择 provider。
- `docs/providers.md` — **create**：Ollama / Anthropic 配置说明。

---

## Task 1: `OpenAiProvider` 处理 `summary`（拼 system，纯函数 + 单测）

**Files:**
- Modify: `shirita-core/src/model/openai.rs`

- [ ] **Step 1: 抽 `openai_messages` 纯函数 + 失败测试**

在 `shirita-core/src/model/openai.rs` 顶部加 `use serde_json::json;`，并在 `impl` 之外加纯函数：

```rust
/// 构造 OpenAI `messages` 数组：把 `req.summary`（若有）拼到首条 system 尾部；无 system 则前插一条 system。
pub fn openai_messages(req: &ChatRequest) -> Vec<serde_json::Value> {
    let mut msgs: Vec<serde_json::Value> = req
        .messages
        .iter()
        .map(|m| json!({ "role": m.role.as_str(), "content": m.content }))
        .collect();
    if let Some(sum) = &req.summary {
        let block = format!("\n\n[Summary of earlier conversation]\n{sum}");
        if let Some(sys) = msgs.iter_mut().find(|m| m["role"] == "system") {
            let cur = sys["content"].as_str().unwrap_or("").to_string();
            sys["content"] = json!(format!("{cur}{block}"));
        } else {
            msgs.insert(0, json!({ "role": "system", "content": block.trim_start() }));
        }
    }
    msgs
}
```

在文件底部加测试模块：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ChatMessage;
    use crate::models::message::Role;

    fn req(messages: Vec<ChatMessage>, summary: Option<&str>) -> ChatRequest {
        ChatRequest { model: "m".into(), messages, summary: summary.map(|s| s.into()) }
    }

    #[test]
    fn summary_appended_to_existing_system() {
        let r = req(vec![
            ChatMessage { role: Role::System, content: "SYS".into() },
            ChatMessage { role: Role::User, content: "hi".into() },
        ], Some("earlier stuff"));
        let msgs = openai_messages(&r);
        assert_eq!(msgs[0]["role"], "system");
        let sys = msgs[0]["content"].as_str().unwrap();
        assert!(sys.starts_with("SYS"));
        assert!(sys.contains("earlier stuff"));
        assert_eq!(msgs[1]["content"], "hi");
    }

    #[test]
    fn summary_prepends_system_when_none() {
        let r = req(vec![ChatMessage { role: Role::User, content: "hi".into() }], Some("S"));
        let msgs = openai_messages(&r);
        assert_eq!(msgs[0]["role"], "system");
        assert!(msgs[0]["content"].as_str().unwrap().contains("S"));
    }

    #[test]
    fn no_summary_passes_through() {
        let r = req(vec![ChatMessage { role: Role::User, content: "hi".into() }], None);
        let msgs = openai_messages(&r);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["content"], "hi");
    }
}
```

- [ ] **Step 2: 跑测试看它失败**

Run: `cargo test -p shirita-core --lib openai::`
Expected: FAIL（`openai_messages` 未定义 / 测试编译错误）→ 加函数后应 PASS。先确认函数已加。

- [ ] **Step 3: `stream_chat` 改用 `openai_messages`**

把 `stream_chat` 里 `body` 的 `"messages"` 字段从内联 map 改为调用纯函数：

```rust
        let body = serde_json::json!({
            "model": req.model,
            "stream": true,
            "messages": openai_messages(&req),
        });
```

- [ ] **Step 4: 跑测试看它通过**

Run: `cargo test -p shirita-core --lib openai::`
Expected: PASS（3 tests）。

- [ ] **Step 5: 提交**

```bash
git add shirita-core/src/model/openai.rs
git commit -m "feat(core): OpenAiProvider folds ChatRequest.summary into the system message"
```

---

## Task 2: `AnthropicProvider`

**Files:**
- Create: `shirita-core/src/model/anthropic.rs`
- Modify: `shirita-core/src/model/mod.rs`、`shirita-core/src/lib.rs`

- [ ] **Step 1: 写文件（纯函数 + provider）+ 失败测试**

创建 `shirita-core/src/model/anthropic.rs`：

```rust
//! Anthropic Messages API 流式适配器（POST /v1/messages, stream=true）。

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use serde_json::json;

use crate::models::message::Role;
use crate::{Error, Result};

use super::{ChatRequest, ModelProvider};

pub struct AnthropicProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl AnthropicProvider {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self { client: reqwest::Client::new(), base_url: base_url.into(), api_key: api_key.into() }
    }
}

/// 构造 Anthropic 请求体：System 段合并进顶层 `system`；`summary` 包成 `<history_summary>`
/// **prepend 到第一条 user 消息开头**（保持 user/assistant 交替，避免连续同 role）；无 user 则作首条 user。
pub fn anthropic_body(req: &ChatRequest) -> serde_json::Value {
    let mut system = String::new();
    let mut messages: Vec<serde_json::Value> = Vec::new();
    for m in &req.messages {
        match m.role {
            Role::System => {
                if !system.is_empty() {
                    system.push_str("\n\n");
                }
                system.push_str(&m.content);
            }
            Role::User => messages.push(json!({ "role": "user", "content": m.content })),
            Role::Assistant => messages.push(json!({ "role": "assistant", "content": m.content })),
        }
    }
    if let Some(sum) = &req.summary {
        let wrapped = format!("<history_summary>\n{sum}\n</history_summary>");
        if let Some(first_user) = messages.iter_mut().find(|m| m["role"] == "user") {
            let cur = first_user["content"].as_str().unwrap_or("").to_string();
            first_user["content"] = json!(format!("{wrapped}\n\n{cur}"));
        } else {
            messages.insert(0, json!({ "role": "user", "content": wrapped }));
        }
    }
    json!({
        "model": req.model,
        "stream": true,
        "max_tokens": 4096,
        "system": system,
        "messages": messages,
    })
}

/// 解析 Anthropic SSE `data:` 之后的 JSON：仅 `content_block_delta` 取 `delta.text`，其余返回 None。
pub fn parse_anthropic_delta(json_after_data: &str) -> Result<Option<String>> {
    let v: serde_json::Value = serde_json::from_str(json_after_data)?;
    if v["type"] == "content_block_delta" {
        Ok(v["delta"]["text"].as_str().map(|s| s.to_string()))
    } else {
        Ok(None)
    }
}

#[async_trait]
impl ModelProvider for AnthropicProvider {
    async fn stream_chat(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<String>>> {
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let body = anthropic_body(&req);

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Config(format!("request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Config(format!("provider {status}: {text}")));
        }

        let mut bytes = resp.bytes_stream();
        let stream = async_stream::stream! {
            let mut buf = String::new();
            while let Some(chunk) = bytes.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => { yield Err(Error::Config(format!("stream error: {e}"))); return; }
                };
                buf.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim_end_matches('\r').to_string();
                    buf.drain(..=pos);
                    let data = match line.strip_prefix("data:") {
                        Some(d) => d.trim(),
                        None => continue, // 忽略 event: 行与空行
                    };
                    match parse_anthropic_delta(data) {
                        Ok(Some(text)) => yield Ok(text),
                        Ok(None) => {}
                        Err(e) => { yield Err(e); return; }
                    }
                }
            }
        };
        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ChatMessage;

    fn req(messages: Vec<ChatMessage>, summary: Option<&str>) -> ChatRequest {
        ChatRequest { model: "claude".into(), messages, summary: summary.map(|s| s.into()) }
    }

    #[test]
    fn system_segments_lifted_to_top_level() {
        let r = req(vec![
            ChatMessage { role: Role::System, content: "SYS".into() },
            ChatMessage { role: Role::User, content: "hi".into() },
        ], None);
        let b = anthropic_body(&r);
        assert_eq!(b["system"], "SYS");
        assert_eq!(b["messages"].as_array().unwrap().len(), 1);
        assert_eq!(b["messages"][0]["role"], "user");
        assert_eq!(b["messages"][0]["content"], "hi");
    }

    #[test]
    fn summary_prepended_to_first_user_keeps_alternation() {
        let r = req(vec![
            ChatMessage { role: Role::System, content: "SYS".into() },
            ChatMessage { role: Role::User, content: "hi".into() },
            ChatMessage { role: Role::Assistant, content: "yo".into() },
        ], Some("earlier"));
        let b = anthropic_body(&r);
        let msgs = b["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2); // 没有额外多出一条 user
        assert_eq!(msgs[0]["role"], "user");
        let first = msgs[0]["content"].as_str().unwrap();
        assert!(first.contains("<history_summary>"));
        assert!(first.contains("earlier"));
        assert!(first.trim_end().ends_with("hi"));
        assert_eq!(msgs[1]["role"], "assistant");
    }

    #[test]
    fn parse_delta_extracts_text_block() {
        let line = r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"He"}}"#;
        assert_eq!(parse_anthropic_delta(line).unwrap(), Some("He".to_string()));
    }

    #[test]
    fn parse_delta_ignores_non_text_events() {
        assert_eq!(parse_anthropic_delta(r#"{"type":"message_start"}"#).unwrap(), None);
    }

    #[test]
    fn parse_delta_invalid_json_errors() {
        assert!(parse_anthropic_delta("not json").is_err());
    }
}
```

- [ ] **Step 2: 注册模块 + 跑测试**

`shirita-core/src/model/mod.rs` 在 `pub mod openai;` 旁加 `pub mod anthropic;`，并加
`pub use anthropic::AnthropicProvider;`。`shirita-core/src/lib.rs` 的
`pub use model::{...}` 行加上 `AnthropicProvider`。

Run: `cargo test -p shirita-core --lib anthropic::`
Expected: PASS（5 tests）。

- [ ] **Step 3: 提交**

```bash
git add shirita-core/src/model/anthropic.rs shirita-core/src/model/mod.rs shirita-core/src/lib.rs
git commit -m "feat(core): AnthropicProvider — messages API + summary as <history_summary> user turn"
```

---

## Task 3: provider 选择（`PROVIDER` env）+ Ollama 文档

**Files:**
- Modify: `shirita-web/src/main.rs`
- Create: `docs/providers.md`

- [ ] **Step 1: 按 `PROVIDER` env 选择**

`shirita-web/src/main.rs` 顶部 `use shirita_core::{...}` 加上 `AnthropicProvider`。把现有的
provider 选择块（`let provider: Arc<dyn ModelProvider> = if config.openai_api_key.is_empty() {...} else {...};`）
替换为：

```rust
    let provider: Arc<dyn ModelProvider> = match std::env::var("PROVIDER").unwrap_or_default().as_str() {
        "anthropic" => {
            let base = std::env::var("ANTHROPIC_BASE_URL").unwrap_or_else(|_| "https://api.anthropic.com".into());
            tracing::info!("using AnthropicProvider at {base}");
            Arc::new(AnthropicProvider::new(base, config.openai_api_key.clone()))
        }
        "ollama" => {
            let base = std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434/v1".into());
            tracing::info!("using Ollama (OpenAI-compatible) at {base}");
            Arc::new(OpenAiProvider::new(base, "ollama"))
        }
        _ => {
            if config.openai_api_key.is_empty() {
                tracing::info!("OPENAI_API_KEY empty: using offline EchoProvider");
                Arc::new(EchoProvider)
            } else {
                tracing::info!("using OpenAiProvider at {}", config.openai_base_url);
                Arc::new(OpenAiProvider::new(config.openai_base_url.clone(), config.openai_api_key.clone()))
            }
        }
    };
```

> `OPENAI_API_KEY`（即 `config.openai_api_key`）作为通用 key 传给 Anthropic；Ollama 不需要真实 key，传占位
> `"ollama"`。模型名仍由现有 `OPENAI_MODEL`（`config.openai_model`）提供（Anthropic 处填 `claude-*` 模型名）。

- [ ] **Step 2: 写 Ollama / Anthropic 文档**

创建 `docs/providers.md`：

```markdown
# Provider 配置

后端按环境变量 `PROVIDER` 选择模型适配器（默认 OpenAI 兼容；无 key 则离线 Echo）。

| PROVIDER | 端点 | 必要 env |
|---|---|---|
| (空) / 其它 | `OPENAI_BASE_URL`（默认 OpenAI） | `OPENAI_API_KEY`、`OPENAI_MODEL` |
| `ollama` | `OLLAMA_BASE_URL`（默认 `http://localhost:11434/v1`） | `OPENAI_MODEL`（如 `llama3`） |
| `anthropic` | `ANTHROPIC_BASE_URL`（默认 `https://api.anthropic.com`） | `OPENAI_API_KEY`（即 Anthropic key）、`OPENAI_MODEL`（如 `claude-sonnet-4-6`） |

- **Ollama**：复用 OpenAI 兼容端点 `/v1/chat/completions`，无需新代码——`PROVIDER=ollama` 即可。
- **Anthropic**：走 `/v1/messages`，滚动摘要作为 `<history_summary>` user 消息插到可见历史最前（见 M6 spec §4）。
```

- [ ] **Step 3: 编译 + 全量回归**

Run: `cargo test --workspace`
Expected: PASS、零警告（`cargo build --workspace` clean）。

- [ ] **Step 4: 提交**

```bash
git add shirita-web/src/main.rs docs/providers.md
git commit -m "feat(web): select provider via PROVIDER env (openai/ollama/anthropic) + docs"
```

---

## Self-Review Checklist

- **Spec 覆盖**：§4 各 provider 自行放置 `summary`（`openai_messages` 拼 system T1；`anthropic_body` 作 `<history_summary>` user T2）✓、Anthropic `/v1/messages` + 顶层 system + `content_block_delta` 解析（T2）✓、Ollama 复用兼容端点（T3 `PROVIDER=ollama`）✓、provider 选择（T3 `PROVIDER` env）✓。
- **Placeholder 扫描**：无 TBD；纯函数与 provider 均给完整代码；`stream_chat` 同构现有 `OpenAiProvider`，真实网络路径不写脆弱单测（与现有 `OpenAiProvider` 一致），核心由 `openai_messages`/`anthropic_body`/`parse_anthropic_delta` 单测覆盖。
- **类型一致**：`openai_messages(&ChatRequest) -> Vec<Value>`、`anthropic_body(&ChatRequest) -> Value`、`parse_anthropic_delta(&str) -> Result<Option<String>>`、`AnthropicProvider::new(impl Into<String>, impl Into<String>)` 与 `OpenAiProvider::new` 同形；`ChatRequest { model, messages, summary }` 沿用 Plan 1；`Role::as_str()` 既有。
- **依赖前置**：依赖 Plan 1 的 `ChatRequest.summary`。与 Plan 2 相互独立（可在 Plan 1 之后、与 Plan 2 并行或之后执行）。
- **交替约束**：Anthropic `messages` 必须 user/assistant 交替——摘要 prepend 到首条 user（不另起 user），单测 `summary_prepended_to_first_user_keeps_alternation` 守这条。
