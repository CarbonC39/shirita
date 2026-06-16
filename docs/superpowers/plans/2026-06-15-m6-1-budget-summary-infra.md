# M6 Plan 1 — 预算 + 摘要基础设施 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 加上下文长度治理的"读侧地基"——`budget` 纯函数模块（阈值判断 + 历史裁剪）、`summaries` 侧带表（迁移 0012）与 `Storage` 方法、`ChatRequest.summary` 一等字段，并让组装按当前分支适用摘要填 `summary`、`context` 只取水位线之后；fork 复制并重映射摘要；`send_message`/`regenerate` 接入裁剪与溢出错误。

**Architecture:** 摘要不进消息树，而是 `summaries` 表按 `cutoff_message_id` 锚定（分支隔离）。组装时 conversation 层选"cutoff 落在 active path 上且最靠后"的摘要，填进新增的 `ChatRequest.summary` 字段（放进请求体哪里由各 provider 决定，见 Plan 3），并把 cutoff 之前的历史移出 `context`。裁剪是 best-effort 兜底：组装后若超窗口，从最旧的中段历史消息开始丢，溢出则优雅报错。

**Tech Stack:** Rust、sqlx 运行时 query API（迁移 `sqlx::migrate!("./migrations")` 编译期嵌入）、`serde_json`、async-stream。

**Upstream spec:** `docs/superpowers/specs/2026-06-15-m6-context-engineering-design.md`（§1 预算/裁剪、§3 数据模型/组装/fork）。

---

## File Structure

- `shirita-core/src/budget.rs` — **create**：`over_threshold`、`trim_history`（纯函数 + 单测）。
- `shirita-core/src/lib.rs` — **modify**：`pub mod budget;` + 选择性 re-export。
- `shirita-core/src/model/mod.rs` — **modify**：`ChatRequest` 加 `summary: Option<String>`。
- `shirita-core/src/models/summary.rs` — **create**：`Summary` 模型。
- `shirita-core/src/models/mod.rs` — **modify**：`pub mod summary;`。
- `shirita-core/migrations/0012_summaries.sql` — **create**：`summaries` 表。
- `shirita-core/src/storage/mod.rs` — **modify**：`create_summary` / `list_summaries` trait 方法。
- `shirita-core/src/storage/sqlite.rs` — **modify**：上述方法实现 + `row_to_summary` + 单测。
- `shirita-core/src/conversation.rs` — **modify**：`applicable_summary` 选取 + `assemble_request` 接 `summary` 参数 + `context` 截断 + 裁剪/溢出接入；inline 集成测试。
- `shirita-web/src/routes/messages.rs` — **modify**：`fork_session` 复制并重映射 `summaries`。
- `shirita-web/tests/summaries_test.rs` — **create**：fork 复制摘要的 web 测试。

---

## Task 1: `budget` 模块（纯函数）

**Files:**
- Create: `shirita-core/src/budget.rs`
- Modify: `shirita-core/src/lib.rs`

- [ ] **Step 1: 写失败测试（建文件）**

创建 `shirita-core/src/budget.rs`：

```rust
//! 上下文预算与 best-effort 历史裁剪：纯函数、无 I/O。
//! 既定决策：保守安全边际、不做严格逐 token 裁剪，溢出优雅暴露（见 M6 spec §1）。

use crate::model::ChatMessage;
use crate::tokenizer::TokenCounter;

/// 用量是否越过触发线（window * threshold）。
pub fn over_threshold(prompt_tokens: usize, window: usize, threshold: f64) -> bool {
    (prompt_tokens as f64) > (window as f64) * threshold
}

/// best-effort 裁剪：保留首条（system）与末条（当前 user 轮），从最旧的中段历史
/// 逐条丢弃直到总用量 <= window 或只剩首末两条。返回（保留的消息，丢弃条数）。
pub fn trim_history(
    messages: &[ChatMessage],
    window: usize,
    counter: &dyn TokenCounter,
) -> (Vec<ChatMessage>, usize) {
    let tok = |m: &ChatMessage| counter.count(&m.content);
    let mut running: usize = messages.iter().map(tok).sum();
    if running <= window || messages.len() <= 2 {
        return (messages.to_vec(), 0);
    }
    let mut keep = vec![true; messages.len()];
    let last = messages.len() - 1;
    let mut dropped = 0usize;
    // 中段 = 索引 1..last，最旧的先丢。
    for i in 1..last {
        if running <= window {
            break;
        }
        keep[i] = false;
        running -= tok(&messages[i]);
        dropped += 1;
    }
    let out = messages
        .iter()
        .zip(keep)
        .filter_map(|(m, k)| if k { Some(m.clone()) } else { None })
        .collect();
    (out, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::message::Role;

    struct CharCounter;
    impl TokenCounter for CharCounter {
        fn count(&self, t: &str) -> usize {
            t.chars().count()
        }
    }

    fn msg(role: Role, content: &str) -> ChatMessage {
        ChatMessage { role, content: content.into() }
    }

    #[test]
    fn over_threshold_compares_against_window_times_ratio() {
        assert!(over_threshold(81, 100, 0.8));
        assert!(!over_threshold(80, 100, 0.8));
    }

    #[test]
    fn trim_keeps_first_and_last_drops_oldest_middle() {
        // sys(2) + h1(10) + h2(10) + h3(10) + last(2) = 34; window 20
        let msgs = vec![
            msg(Role::System, "ss"),
            msg(Role::User, "aaaaaaaaaa"),
            msg(Role::Assistant, "bbbbbbbbbb"),
            msg(Role::User, "cccccccccc"),
            msg(Role::User, "zz"),
        ];
        let (out, dropped) = trim_history(&msgs, 20, &CharCounter);
        assert_eq!(dropped, 2); // h1,h2 丢弃
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].content, "ss"); // 首条 system 保留
        assert_eq!(out[1].content, "cccccccccc"); // 最近 history 保留
        assert_eq!(out[2].content, "zz"); // 末条保留
    }

    #[test]
    fn trim_noop_when_within_window() {
        let msgs = vec![msg(Role::System, "ss"), msg(Role::User, "hi")];
        let (out, dropped) = trim_history(&msgs, 100, &CharCounter);
        assert_eq!(dropped, 0);
        assert_eq!(out.len(), 2);
    }
}
```

- [ ] **Step 2: 接 lib + 跑测试**

`shirita-core/src/lib.rs` 在 `pub mod assembly;` 附近加 `pub mod budget;`，并加 re-export：

```rust
pub use budget::{over_threshold, trim_history};
```

Run: `cargo test -p shirita-core --lib budget::`
Expected: PASS（3 tests）。

- [ ] **Step 3: 提交**

```bash
git add shirita-core/src/budget.rs shirita-core/src/lib.rs
git commit -m "feat(core): budget module — over_threshold + trim_history"
```

---

## Task 2: `ChatRequest` 加 `summary` 字段

**Files:**
- Modify: `shirita-core/src/model/mod.rs`、`shirita-core/src/conversation.rs`

- [ ] **Step 1: 改结构 + 唯一构造点**

`shirita-core/src/model/mod.rs`，把 `ChatRequest` 改为：

```rust
/// 一次聊天补全请求。
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    /// 当前分支的滚动摘要（若有）。放进请求体哪里由各 provider 决定（见 M6 spec §4）。
    pub summary: Option<String>,
}
```

`shirita-core/src/conversation.rs` 里 `assemble_request` 末尾的构造点（现为
`Ok((ChatRequest { model, messages: chat_messages }, regex_rules))`）暂时补 `summary: None`，
保持编译（Task 4 再真正填）：

```rust
    Ok((ChatRequest { model, messages: chat_messages, summary: None }, regex_rules))
```

- [ ] **Step 2: 编译验证（含测试）**

Run: `cargo test -p shirita-core --lib`
Expected: PASS（既有测试不受影响；`ChatRequest` 无 `PartialEq`，测试只比较 `messages[i].content`）。

- [ ] **Step 3: 提交**

```bash
git add shirita-core/src/model/mod.rs shirita-core/src/conversation.rs
git commit -m "feat(core): ChatRequest carries optional rolling summary"
```

---

## Task 3: 迁移 0012 + `Summary` 模型 + `Storage` 方法

**Files:**
- Create: `shirita-core/migrations/0012_summaries.sql`、`shirita-core/src/models/summary.rs`
- Modify: `shirita-core/src/models/mod.rs`、`shirita-core/src/storage/mod.rs`、`shirita-core/src/storage/sqlite.rs`、`shirita-core/src/lib.rs`

- [ ] **Step 1: 写迁移**

创建 `shirita-core/migrations/0012_summaries.sql`：

```sql
-- 滚动摘要侧带表：按 cutoff_message_id 锚定水位线，不进消息树（M6 spec §3）。
CREATE TABLE summaries (
    id                TEXT PRIMARY KEY,
    session_id        TEXT NOT NULL,
    cutoff_message_id TEXT NOT NULL,
    content           TEXT NOT NULL,
    created_at        TEXT NOT NULL
);
CREATE INDEX idx_summaries_session ON summaries(session_id);
```

- [ ] **Step 2: 写 `Summary` 模型**

创建 `shirita-core/src/models/summary.rs`：

```rust
//! 滚动摘要：覆盖"对话开头 → cutoff_message_id（含）"的历史压缩文本。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub id: String,
    pub session_id: String,
    pub cutoff_message_id: String,
    pub content: String,
    pub created_at: String,
}

impl Summary {
    pub fn new(session_id: &str, cutoff_message_id: &str, content: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            cutoff_message_id: cutoff_message_id.to_string(),
            content: content.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}
```

> `uuid` / `chrono` 已是 core 依赖（见 `Message::new` 同样用法）。

在 `shirita-core/src/models/mod.rs` 加 `pub mod summary;`。在 `shirita-core/src/lib.rs` 加
`pub use models::summary::Summary;`。

- [ ] **Step 3: 加 `Storage` trait 方法**

`shirita-core/src/storage/mod.rs`，在 `// --- settings ---` 之前加一段：

```rust
    // --- summaries (M6 rolling context summaries) ---
    async fn create_summary(&self, summary: &Summary) -> Result<()>;
    async fn list_summaries(&self, session_id: &str) -> Result<Vec<Summary>>;
```

并确保该文件顶部 `use` 引入了 `Summary`（在已有 `use crate::models::...` 处加
`use crate::models::summary::Summary;`）。

- [ ] **Step 4: 写失败测试（在 sqlite.rs 的 `#[cfg(test)] mod tests`）**

在 `shirita-core/src/storage/sqlite.rs` 测试模块加：

```rust
    #[tokio::test]
    async fn summaries_roundtrip() {
        let s = temp_storage().await;
        let sess = crate::models::session::Session::new("s");
        s.create_session(&sess).await.unwrap();
        let sum = crate::models::summary::Summary::new(&sess.id, "msg-7", "earlier summary");
        s.create_summary(&sum).await.unwrap();
        let got = s.list_summaries(&sess.id).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].cutoff_message_id, "msg-7");
        assert_eq!(got[0].content, "earlier summary");
        // 其他会话不串
        assert!(s.list_summaries("other").await.unwrap().is_empty());
    }
```

> `temp_storage()` 已存在于该测试模块（运行迁移后返回 `SqliteStorage`）。

- [ ] **Step 5: 跑测试看它失败**

Run: `cargo test -p shirita-core --lib summaries_roundtrip`
Expected: FAIL（trait 方法未实现，编译错误）。

- [ ] **Step 6: 实现方法（在 `impl Storage for SqliteStorage`）**

在 `shirita-core/src/storage/sqlite.rs` 顶部 `use` 处加 `use crate::models::summary::Summary;`，
并在 `impl Storage for SqliteStorage` 内（settings 方法附近）加：

```rust
    async fn create_summary(&self, summary: &Summary) -> Result<()> {
        sqlx::query(
            "INSERT INTO summaries (id, session_id, cutoff_message_id, content, created_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&summary.id)
        .bind(&summary.session_id)
        .bind(&summary.cutoff_message_id)
        .bind(&summary.content)
        .bind(&summary.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_summaries(&self, session_id: &str) -> Result<Vec<Summary>> {
        let rows = sqlx::query(
            "SELECT id, session_id, cutoff_message_id, content, created_at \
             FROM summaries WHERE session_id = ? ORDER BY created_at ASC, id ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                use sqlx::Row;
                Ok(Summary {
                    id: r.try_get("id")?,
                    session_id: r.try_get("session_id")?,
                    cutoff_message_id: r.try_get("cutoff_message_id")?,
                    content: r.try_get("content")?,
                    created_at: r.try_get("created_at")?,
                })
            })
            .collect()
    }
```

> 参照同文件 `list_messages` / `row_to_message` 的 `sqlx::query(...).fetch_all` + `try_get` 风格。
> `delete_session` 已 `DELETE FROM messages WHERE session_id=?` 等；为对称，在 `delete_session`
> 实现里追加一行 `sqlx::query("DELETE FROM summaries WHERE session_id = ?").bind(id).execute(&self.pool).await?;`。

- [ ] **Step 7: 跑测试看它通过**

Run: `cargo test -p shirita-core --lib summaries_roundtrip`
Expected: PASS。再跑 `cargo test -p shirita-core` 确认全绿。

- [ ] **Step 8: 提交**

```bash
git add shirita-core/migrations/0012_summaries.sql shirita-core/src/models/summary.rs \
        shirita-core/src/models/mod.rs shirita-core/src/storage/mod.rs \
        shirita-core/src/storage/sqlite.rs shirita-core/src/lib.rs
git commit -m "feat(core): summaries table (0012) + Summary model + storage methods"
```

---

## Task 4: 组装读摘要（选取 + 填 `summary` + `context` 截断）

**Files:**
- Modify: `shirita-core/src/conversation.rs`

- [ ] **Step 1: 写失败测试**

在 `shirita-core/src/conversation.rs` 的 `#[cfg(test)] mod tests` 加：

```rust
    #[tokio::test]
    async fn assembly_uses_applicable_summary_and_truncates_history() {
        let storage = Arc::new(temp_storage().await);
        let session = Session::new("s");
        storage.create_session(&session).await.unwrap();

        // 线性历史：u1 -> a1 -> u2 -> a2（a2 为活动叶子）
        let u1 = Message::new(&session.id, None, Role::User, "u1");
        storage.create_message(&u1).await.unwrap();
        let a1 = Message::new(&session.id, Some(u1.id.clone()), Role::Assistant, "a1");
        storage.create_message(&a1).await.unwrap();
        let u2 = Message::new(&session.id, Some(a1.id.clone()), Role::User, "u2");
        storage.create_message(&u2).await.unwrap();
        let a2 = Message::new(&session.id, Some(u2.id.clone()), Role::Assistant, "a2");
        storage.create_message(&a2).await.unwrap();
        storage.set_session_active_leaf(&session.id, Some(&a2.id)).await.unwrap();

        // 摘要覆盖到 a1（cutoff = a1）：u1/a1 不应进 context，summary 应被携带。
        let sum = crate::models::summary::Summary::new(&session.id, &a1.id, "[sum] u1 a1 happened");
        storage.create_summary(&sum).await.unwrap();

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "u3".into());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let req = seen.lock().unwrap().clone().unwrap();
        assert_eq!(req.summary.as_deref(), Some("[sum] u1 a1 happened"));
        let joined: String = req.messages.iter().map(|m| m.content.clone()).collect::<Vec<_>>().join("|");
        assert!(!joined.contains("u1"), "cutoff 之前的历史不应进 context: {joined}");
        assert!(!joined.contains("a1"), "cutoff 之前的历史不应进 context: {joined}");
        assert!(joined.contains("u2"), "cutoff 之后的历史应保留");
        assert!(joined.contains("u3"), "本次 user 应保留");
    }
```

- [ ] **Step 2: 跑测试看它失败**

Run: `cargo test -p shirita-core --lib assembly_uses_applicable_summary`
Expected: FAIL（`req.summary` 为 None；u1/a1 仍在 context）。

- [ ] **Step 3: 加 `applicable_summary` helper**

在 `shirita-core/src/conversation.rs`（`session_schema` helper 附近）加：

```rust
use crate::models::summary::Summary;

/// 选当前分支适用摘要：cutoff 必须落在 active path 上，多条取 path 中最靠后的那条。
/// 返回（摘要内容, path 中 cutoff 的下标）。
async fn applicable_summary(
    storage: &dyn Storage,
    session_id: &str,
    path: &[Message],
) -> Option<(String, usize)> {
    let summaries: Vec<Summary> = storage.list_summaries(session_id).await.ok()?;
    let pos = |mid: &str| path.iter().position(|m| m.id == mid);
    summaries
        .into_iter()
        .filter_map(|s| pos(&s.cutoff_message_id).map(|i| (s.content, i)))
        .max_by_key(|(_, i)| *i)
}
```

- [ ] **Step 4: `assemble_request` 接 `summary` 参数**

把 `assemble_request` 签名再加一个尾参 `summary: Option<String>`：

```rust
async fn assemble_request(
    storage: &dyn Storage,
    session: &Session,
    model: String,
    context: &[ChatMessage],
    state: &serde_json::Value,
    summary: Option<String>,
) -> crate::Result<(ChatRequest, Vec<Definition>)> {
```

并把末尾构造点改为：

```rust
    Ok((ChatRequest { model, messages: chat_messages, summary }, regex_rules))
```

- [ ] **Step 5: `send_message` 选摘要 + 截断 context + 传参**

在 `send_message` 里，`let path = crate::tree::active_path(...)` 之后、构造 `context` 之前，加：

```rust
        let summary = applicable_summary(storage.as_ref(), &session_id, &path).await;
        let visible_start = summary.as_ref().map(|(_, i)| i + 1).unwrap_or(0);
        let summary_text = summary.map(|(c, _)| c);
```

把构造 `context` 的那段（现在 `path.iter().filter(...)`）改为从 `visible_start` 起：

```rust
        let mut context: Vec<ChatMessage> = path[visible_start..]
            .iter()
            .filter(|m| !m.is_hidden)
            .map(|m| ChatMessage { role: m.role, content: m.raw_content.clone() })
            .collect();
        context.push(ChatMessage { role: Role::User, content: user_text.clone() });
```

把 `assemble_request(...)` 调用补上 `summary_text.clone()`：

```rust
        let (req, regex_rules) = match assemble_request(storage.as_ref(), &session, model, &context, &branch_state, summary_text.clone()).await {
```

> 注意 `branch_state` 仍按 M5 用整条 `path` 的叶子快照计算，不受 `visible_start` 影响——保持原有顺序（先算 schema/branch_state，再算 summary/visible_start）。

- [ ] **Step 6: `regenerate` 同样处理**

在 `regenerate` 里，`let path = crate::tree::active_path(&all, target.parent_id.as_deref());` 之后加同样三行
（`applicable_summary` / `visible_start` / `summary_text`），把 `context` 构造改为 `path[visible_start..]`，
并把 `assemble_request(...)` 调用补 `summary_text.clone()`。

- [ ] **Step 7: 跑测试看它通过**

Run: `cargo test -p shirita-core --lib`
Expected: PASS（新测试 + 既有 M4/M5 测试全绿；无摘要时 `visible_start=0`，行为不变）。

- [ ] **Step 8: 提交**

```bash
git add shirita-core/src/conversation.rs
git commit -m "feat(core): assembly carries applicable branch summary, truncates folded history"
```

---

## Task 5: 裁剪接入 `send_message`/`regenerate` + 溢出错误

**Files:**
- Modify: `shirita-core/src/conversation.rs`

- [ ] **Step 1: 写失败测试**

在 `#[cfg(test)] mod tests` 加（用极小 window 触发裁剪，断言最旧历史被丢）：

```rust
    #[tokio::test]
    async fn oversized_history_is_trimmed_before_send() {
        let storage = Arc::new(temp_storage().await);
        let session = Session::new("s");
        storage.create_session(&session).await.unwrap();
        // 造一串长 user/assistant 历史（无摘要）
        let mut parent: Option<String> = None;
        let mut leaf = String::new();
        for i in 0..6 {
            let content = format!("turn-{i}-{}", "x".repeat(50));
            let role = if i % 2 == 0 { Role::User } else { Role::Assistant };
            let m = Message::new(&session.id, parent.clone(), role, &content);
            storage.create_message(&m).await.unwrap();
            parent = Some(m.id.clone());
            leaf = m.id.clone();
        }
        storage.set_session_active_leaf(&session.id, Some(&leaf)).await.unwrap();

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        // window 设得很小 → 触发裁剪
        storage.set_setting("context.window", &serde_json::json!(20)).await.unwrap();

        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "newest".into());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let req = seen.lock().unwrap().clone().unwrap();
        let joined: String = req.messages.iter().map(|m| m.content.clone()).collect::<Vec<_>>().join("|");
        assert!(joined.contains("newest"), "末条本次 user 必须保留");
        assert!(!joined.contains("turn-0"), "最旧历史应被裁掉");
    }
```

- [ ] **Step 2: 跑测试看它失败**

Run: `cargo test -p shirita-core --lib oversized_history_is_trimmed_before_send`
Expected: FAIL（未接裁剪，turn-0 仍在）。

- [ ] **Step 3: 加预算读取 helper + 接入裁剪**

在 `shirita-core/src/conversation.rs` 顶部 `use` 加 `use crate::budget::trim_history;`，并加 helper：

```rust
/// 读上下文窗口（settings `context.window`，默认 200000）。
async fn context_window(storage: &dyn Storage) -> usize {
    storage
        .get_setting("context.window")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(200_000)
}
```

在 `send_message` 里，`assemble_request(...)` 返回 `(req, regex_rules)` 之后、`provider.stream_chat(req)` 之前，插入裁剪：

```rust
        let window = context_window(storage.as_ref()).await;
        let (trimmed, dropped) = trim_history(&req.messages, window, counter.as_ref());
        if dropped > 0 {
            tracing::warn!(dropped, "context over window: trimmed oldest history");
        }
        let req = ChatRequest { model: req.model, messages: trimmed, summary: req.summary };
```

> `counter` 已在 `send_message` 参数中（`Arc<dyn TokenCounter>`）。`ChatRequest`/`ChatMessage` 已在作用域。

在 `regenerate` 里同样插入这段（`regenerate` 的 token counter 参数名为 `_counter`——把它改名为 `counter` 并去掉下划线，使其可用）。

- [ ] **Step 4: 溢出优雅暴露**

`provider.stream_chat(req).await` 已返回 `Err` 时被转成 `SendEvent::Error`（既有逻辑）。无需额外改动：
真实 provider 在裁剪后仍超长会返回 `Err(Error::Config("provider 4xx: ..."))`，沿现有路径变成
`SendEvent::Error`。EchoProvider 不会报错（离线兜底）。本步仅确认既有错误路径覆盖溢出，无代码改动。

- [ ] **Step 5: 跑测试看它通过**

Run: `cargo test -p shirita-core --lib`
Expected: PASS（新测试 + 全量）。

- [ ] **Step 6: 提交**

```bash
git add shirita-core/src/conversation.rs
git commit -m "feat(core): best-effort history trim against context window before send"
```

---

## Task 6: fork 复制并重映射摘要

**Files:**
- Modify: `shirita-web/src/routes/messages.rs`
- Test: `shirita-web/tests/summaries_test.rs`

- [ ] **Step 1: 确认现状映射点**

`shirita-web/src/routes/messages.rs` 的 `fork_session` 已有：源会话 id = `session_id`（Path）、新会话 = `dup`、
旧→新消息 id 映射 = `idmap: HashMap<String, String>`（覆盖 active path `slice`）、请求体 = `ForkBody { message_id }`。
本任务在深拷消息循环之后、`get_session(&dup.id)` 返回之前，用 `idmap` 复制 `summaries`。

- [ ] **Step 2: 写失败测试**

创建 `shirita-web/tests/summaries_test.rs`（harness 同 `local_overrides_test.rs`：复制 `test_state`/`send`/`json`，
含 `generations` 字段）。加：

```rust
#[tokio::test]
async fn fork_copies_and_remaps_summaries() {
    let state = test_state().await;
    // 建会话 + 一条消息 + 一条以该消息为 cutoff 的摘要
    let sid = {
        let (_, out) = send(&state, "POST", "/api/sessions", Some(r#"{"name":"S"}"#)).await;
        json(&out)["id"].as_str().unwrap().to_string()
    };
    // 发一条消息产生 user+assistant（EchoProvider）
    send(&state, "POST", &format!("/api/sessions/{sid}/messages"), Some(r#"{"text":"hi"}"#)).await;
    let msgs = state.storage.list_messages(&sid).await.unwrap();
    let leaf = msgs.last().unwrap().id.clone();
    let sum = shirita_core::models::summary::Summary::new(&sid, &leaf, "[sum] earlier");
    state.storage.create_summary(&sum).await.unwrap();

    // fork
    let (st, out) = send(&state, "POST", &format!("/api/sessions/{sid}/fork"),
        Some(&format!(r#"{{"message_id":"{leaf}"}}"#))).await;
    assert_eq!(st, StatusCode::OK);
    let new_sid = json(&out)["id"].as_str().unwrap().to_string();

    // 新会话应有一条摘要，cutoff 指向新会话里对应的（最后一条）消息
    let copied = state.storage.list_summaries(&new_sid).await.unwrap();
    assert_eq!(copied.len(), 1);
    assert_eq!(copied[0].content, "[sum] earlier");
    let new_msgs = state.storage.list_messages(&new_sid).await.unwrap();
    assert!(new_msgs.iter().any(|m| m.id == copied[0].cutoff_message_id),
        "cutoff 必须重映射到新会话内的消息 id");
}
```

> fork 的请求体字段名以 `messages.rs` 的 `fork_session` 反序列化结构为准（若为 `message_id` 之外的名字，按实际改）。

- [ ] **Step 3: 跑测试看它失败**

Run: `cargo test -p shirita-web --test summaries_test fork_copies_and_remaps_summaries`
Expected: FAIL（新会话 summaries 为空）。

- [ ] **Step 4: 复制 + 重映射**

在 `fork_session` 的 `let _ = state.storage.set_session_active_leaf(&dup.id, new_leaf.as_deref()).await;` 之后、
`get_session(&dup.id)` 之前，加：

```rust
    // 复制源会话的滚动摘要，把 cutoff 重映射到新会话的消息 id（与消息深拷的 idmap 一致）。
    if let Ok(summaries) = state.storage.list_summaries(&session_id).await {
        for s in summaries {
            if let Some(new_cutoff) = idmap.get(&s.cutoff_message_id) {
                let copy = shirita_core::models::summary::Summary::new(&dup.id, new_cutoff, &s.content);
                let _ = state.storage.create_summary(&copy).await;
            }
        }
    }
```

> `idmap` 只覆盖 active path `slice`（root→`message_id`）。cutoff 在该路径内的摘要会被带过去；fork 点之后的不带（合理）。

- [ ] **Step 5: 跑测试看它通过**

Run: `cargo test -p shirita-web --test summaries_test`
Expected: PASS。

- [ ] **Step 6: 全量回归 + 提交**

Run: `cargo test --workspace`
Expected: PASS、零警告。

```bash
git add shirita-web/src/routes/messages.rs shirita-web/tests/summaries_test.rs
git commit -m "feat(web): fork copies and remaps rolling summaries to the new session"
```

---

## Self-Review Checklist

- **Spec 覆盖**：§1 预算（`over_threshold` T1）✓、best-effort 历史裁剪（`trim_history` T1 + 接入 T5）✓、溢出优雅暴露（T5 走既有 Error 路径）✓；§3 `summaries` 表 0012（T3）✓、组装取适用摘要 + cutoff 截断（T4）✓、`ChatRequest.summary` 一等字段（T2）✓、fork 复制重映射（T6）✓。§4 provider 放置 / §2 总结管道在 Plan 2、3。
- **Placeholder 扫描**：无 TBD；每步含具体代码/命令。fork 任务对变量名做了"按实际对齐"说明（因 `fork_session` 内部命名需读后确认），但给了完整插入代码与定位指引。
- **类型一致**：`ChatRequest { model, messages, summary: Option<String> }` 在 T2 定义、T4/T5 构造一致；`trim_history(&[ChatMessage], usize, &dyn TokenCounter) -> (Vec<ChatMessage>, usize)`、`over_threshold(usize, usize, f64) -> bool`、`applicable_summary(&dyn Storage, &str, &[Message]) -> Option<(String, usize)>`、`Summary::new(&str,&str,&str)`、`create_summary(&Summary)`、`list_summaries(&str) -> Vec<Summary>` 各处签名统一；`assemble_request` 尾参增加 `summary: Option<String>`，两处调用点（send/regenerate）均更新。
- **实现者需现场确认点**：`fork_session` 的请求体字段名与旧→新消息 id 映射变量（T6 给了对齐指引）；`regenerate` 的 `_counter` 改名为 `counter`（T5）。
