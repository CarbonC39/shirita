# M6 Plan 2 — 后台异步滚动总结 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 `summarize::run`（自检阈值 → 选水位线 → 构造总结请求 → 聚合调用 provider → 落库滚动摘要，幂等可重入），并在 web `chat` SSE 流结束（`Done`）后以 `tokio::spawn` fire-and-forget 触发，用进程级 per-session 互斥防并发重复；内置默认总结指令，settings `summarize.instruction` 可覆盖。

**Architecture:** 总结绝不在 SSE 主流上同步执行（避免回复流完后前端卡死 5–20s）。`Done` 立即返回，web 层在 `map` 闭包匹配到 `Done` 时 spawn 一个 detached 任务跑 `summarize::run`。`run` 自检"未折叠历史 + 上一摘要"的 token 是否越过 `window*threshold`，是则把"上一摘要 + 待折叠原文"喂给总结指令，聚合 provider 输出为单段文本，写入 `summaries`（cutoff 推进）——滚动重写，永远一个有效摘要。

**Tech Stack:** Rust、async-trait、futures、tokio::spawn、`OnceLock<Mutex<HashSet>>`。依赖 Plan 1 的 `summaries` 表 / `Summary` / `budget::over_threshold` / `ChatRequest.summary`。

**Upstream spec:** `docs/superpowers/specs/2026-06-15-m6-context-engineering-design.md`（§2 滚动总结管道、§5 配置）。

---

## File Structure

- `shirita-core/src/summarize.rs` — **create**：`DEFAULT_INSTRUCTION`、`fold_range`（纯函数）、settings 读取 helper、`run`（async）。
- `shirita-core/src/lib.rs` — **modify**：`pub mod summarize;` + `pub use summarize::run as run_summary;`（避免与其它 `run` 混淆，用别名）。
- `shirita-web/src/routes/chat.rs` — **modify**：进程级互斥 + `spawn_summary` + 在 `send`/`regenerate` 的 `Done` 触发。

> 不改 `AppState`：用进程级 `static` 互斥（`OnceLock<Mutex<HashSet<String>>>`）替代"AppState 持 Arc<Mutex<HashSet>>"，
> 语义等价（进程级 per-session 互斥），避免改 13 个 `AppState` 构造点。

---

## Task 1: `summarize` 模块骨架 —— 默认指令 + `fold_range` + settings helper

**Files:**
- Create: `shirita-core/src/summarize.rs`
- Modify: `shirita-core/src/lib.rs`

- [ ] **Step 1: 写文件 + `fold_range` 失败测试**

创建 `shirita-core/src/summarize.rs`：

```rust
//! 滚动总结管道：自检阈值 → 选水位线 → 构造请求 → 聚合 provider → 落库摘要。
//! 后台 fire-and-forget 调用（web 层 spawn），幂等可重入（见 M6 spec §2）。

use std::sync::Arc;

use futures::StreamExt;

use crate::model::{ChatMessage, ChatRequest, ModelProvider};
use crate::models::message::Role;
use crate::models::summary::Summary;
use crate::storage::Storage;
use crate::tokenizer::TokenCounter;

/// 内置默认总结指令（settings `summarize.instruction` 可整体覆盖）。
pub const DEFAULT_INSTRUCTION: &str = "Summarize the prior conversation faithfully and concisely. \
Preserve facts, decisions, character state, world details and any unresolved threads. \
Write plain prose, third person, no preamble and no meta commentary.";

/// 选待折叠区间 `[start, end)`（path 下标）：start = 上一水位线之后，end = 保留最近
/// `keep_recent` 条之前。无可折叠时返回 None。
pub fn fold_range(path_len: usize, prev_cutoff_idx: Option<usize>, keep_recent: usize) -> Option<(usize, usize)> {
    let start = prev_cutoff_idx.map(|i| i + 1).unwrap_or(0);
    let end = path_len.saturating_sub(keep_recent);
    if start < end {
        Some((start, end))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_range_first_fold_keeps_recent() {
        assert_eq!(fold_range(20, None, 10), Some((0, 10)));
    }

    #[test]
    fn fold_range_advances_from_prev_cutoff() {
        assert_eq!(fold_range(20, Some(4), 10), Some((5, 10)));
    }

    #[test]
    fn fold_range_none_when_nothing_new_to_fold() {
        assert_eq!(fold_range(12, Some(4), 10), None); // start 5 >= end 2
        assert_eq!(fold_range(8, None, 10), None); // end saturates to 0
    }
}
```

- [ ] **Step 2: 接 lib + 跑 fold_range 测试**

`shirita-core/src/lib.rs` 加 `pub mod summarize;`，并加 `pub use summarize::fold_range;`
（`run` 的 re-export 在 Task 2 之后再加，避免引用未定义项）。

Run: `cargo test -p shirita-core --lib summarize::`
Expected: PASS（3 tests）。

> settings 读取 helper（`setting_usize/f64/string`）与 `run` 一起在 Task 2 加入——避免本步出现"未被调用"的
> dead_code 警告（项目要求零警告）。

- [ ] **Step 3: 提交**

```bash
git add shirita-core/src/summarize.rs shirita-core/src/lib.rs
git commit -m "feat(core): summarize module skeleton — default instruction + fold_range"
```

---

## Task 2: `summarize::run`（自检 + 折叠 + 聚合 + 落库）

**Files:**
- Modify: `shirita-core/src/summarize.rs`、`shirita-core/src/lib.rs`

- [ ] **Step 1: 写 settings helper + `run`**

在 `shirita-core/src/summarize.rs` 的 `fold_range` 之后、`#[cfg(test)]` 之前，先加 settings 读取 helper：

```rust
async fn setting_usize(s: &dyn Storage, key: &str, default: usize) -> usize {
    s.get_setting(key).await.ok().flatten()
        .and_then(|v| v.as_u64()).map(|n| n as usize).unwrap_or(default)
}
async fn setting_f64(s: &dyn Storage, key: &str, default: f64) -> f64 {
    s.get_setting(key).await.ok().flatten()
        .and_then(|v| v.as_f64()).unwrap_or(default)
}
async fn setting_string(s: &dyn Storage, key: &str, default: &str) -> String {
    s.get_setting(key).await.ok().flatten()
        .and_then(|v| v.as_str().map(|x| x.to_string())).unwrap_or_else(|| default.to_string())
}
```

紧接着加 `run`：

```rust
/// 后台执行一次滚动总结尝试（幂等可重入）：未超阈值或无可折叠则静默返回；
/// 否则把"上一摘要 + 待折叠原文"喂给总结指令，聚合 provider 输出，写入 `summaries`。
pub async fn run(
    storage: Arc<dyn Storage>,
    provider: Arc<dyn ModelProvider>,
    counter: Arc<dyn TokenCounter>,
    model: String,
    session_id: String,
) {
    let Ok(Some(session)) = storage.get_session(&session_id).await else { return };
    let Ok(all) = storage.list_messages(&session_id).await else { return };
    let path = crate::tree::active_path(&all, session.active_leaf_id.as_deref());
    if path.is_empty() {
        return;
    }

    // 上一摘要：cutoff 落在 active path 上、最靠后的那条。
    let summaries = storage.list_summaries(&session_id).await.unwrap_or_default();
    let prev = summaries
        .iter()
        .filter_map(|s| path.iter().position(|m| m.id == s.cutoff_message_id).map(|i| (s, i)))
        .max_by_key(|(_, i)| *i);
    let prev_idx = prev.as_ref().map(|(_, i)| *i);
    let prev_content = prev.as_ref().map(|(s, _)| s.content.clone());

    let window = setting_usize(storage.as_ref(), "context.window", 200_000).await;
    let threshold = setting_f64(storage.as_ref(), "context.threshold", 0.8).await;
    let keep_recent = setting_usize(storage.as_ref(), "context.keep_recent", 10).await;

    // 自检：未折叠历史（cutoff 之后可见）+ 上一摘要 的 token 是否越过触发线。
    let start_visible = prev_idx.map(|i| i + 1).unwrap_or(0);
    let mut hist_tokens = prev_content.as_deref().map(|c| counter.count(c)).unwrap_or(0);
    for m in &path[start_visible..] {
        if !m.is_hidden {
            hist_tokens += counter.count(&m.raw_content);
        }
    }
    if !crate::budget::over_threshold(hist_tokens, window, threshold) {
        return;
    }

    // 选折叠区间。
    let Some((s, e)) = fold_range(path.len(), prev_idx, keep_recent) else { return };
    let new_cutoff = path[e - 1].id.clone();

    // 构造折叠正文：上一摘要 + 待折叠原文（跳过 hidden）。
    let mut body = String::new();
    if let Some(pc) = &prev_content {
        body.push_str("[Previous summary]\n");
        body.push_str(pc);
        body.push_str("\n\n");
    }
    for m in &path[s..e] {
        if m.is_hidden {
            continue;
        }
        body.push_str(m.role.as_str());
        body.push_str(": ");
        body.push_str(&m.raw_content);
        body.push('\n');
    }

    let instruction = setting_string(storage.as_ref(), "summarize.instruction", DEFAULT_INSTRUCTION).await;
    let req = ChatRequest {
        model,
        messages: vec![
            ChatMessage { role: Role::System, content: instruction },
            ChatMessage { role: Role::User, content: body },
        ],
        summary: None,
    };

    // 聚合调用（非流式语义：收集全部 delta）。
    let Ok(mut stream) = provider.stream_chat(req).await else { return };
    let mut full = String::new();
    while let Some(item) = stream.next().await {
        match item {
            Ok(d) => full.push_str(&d),
            Err(_) => return,
        }
    }
    let full = full.trim();
    if full.is_empty() {
        return;
    }

    let summary = Summary::new(&session_id, &new_cutoff, full);
    if let Err(e) = storage.create_summary(&summary).await {
        tracing::warn!(error = %e, "summary persist failed");
    }
}
```

- [ ] **Step 2: 写集成测试（含一个本地 provider）**

在 `shirita-core/src/summarize.rs` 的 `#[cfg(test)] mod tests` 顶部加 import + 一个固定回复 provider，
并加两个测试：

```rust
    use std::sync::Arc;
    use futures::stream::BoxStream;
    use serde_json::json;
    use crate::models::message::Message;
    use crate::models::session::Session;
    use crate::storage::sqlite::SqliteStorage;
    use crate::tokenizer::tiktoken::TiktokenCounter;

    async fn temp_storage() -> SqliteStorage {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sum.db");
        std::mem::forget(dir);
        let s = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
        s.run_migrations().await.unwrap();
        s
    }

    struct FixedProvider(String);
    #[async_trait::async_trait]
    impl ModelProvider for FixedProvider {
        async fn stream_chat(&self, _req: ChatRequest) -> crate::Result<BoxStream<'static, crate::Result<String>>> {
            let r = self.0.clone();
            Ok(Box::pin(futures::stream::iter(vec![Ok(r)])))
        }
    }

    async fn long_session(storage: &SqliteStorage, turns: usize) -> (Session, String) {
        let session = Session::new("s");
        storage.create_session(&session).await.unwrap();
        let mut parent: Option<String> = None;
        let mut leaf = String::new();
        for i in 0..turns {
            let role = if i % 2 == 0 { Role::User } else { Role::Assistant };
            let m = Message::new(&session.id, parent.clone(), role, &format!("turn-{i}-{}", "x".repeat(40)));
            storage.create_message(&m).await.unwrap();
            parent = Some(m.id.clone());
            leaf = m.id.clone();
        }
        storage.set_session_active_leaf(&session.id, Some(&leaf)).await.unwrap();
        (session, leaf)
    }

    #[tokio::test]
    async fn run_folds_history_when_over_threshold() {
        let storage = Arc::new(temp_storage().await);
        let (session, leaf) = long_session(&storage, 14).await;
        storage.set_setting("context.window", &json!(50)).await.unwrap(); // 小窗口 → 超阈值

        let provider: Arc<dyn ModelProvider> = Arc::new(FixedProvider("SUMMARY-TEXT".into()));
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        run(storage.clone(), provider, counter, "m".into(), session.id.clone()).await;

        let sums = storage.list_summaries(&session.id).await.unwrap();
        assert_eq!(sums.len(), 1);
        assert_eq!(sums[0].content, "SUMMARY-TEXT");
        // len=14, keep_recent=10(默认) → end=4 → cutoff = path[3]
        let all = storage.list_messages(&session.id).await.unwrap();
        let path = crate::tree::active_path(&all, Some(&leaf));
        assert_eq!(sums[0].cutoff_message_id, path[3].id);
    }

    #[tokio::test]
    async fn run_noop_when_under_threshold() {
        let storage = Arc::new(temp_storage().await);
        let (session, _leaf) = long_session(&storage, 4).await; // 短历史
        // 默认 window 200k → 远未超阈值
        let provider: Arc<dyn ModelProvider> = Arc::new(FixedProvider("X".into()));
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        run(storage.clone(), provider, counter, "m".into(), session.id.clone()).await;
        assert!(storage.list_summaries(&session.id).await.unwrap().is_empty());
    }
```

> `tempfile` / `async-trait` 已是 core dev/deps（`conversation.rs` 测试同样使用）。

- [ ] **Step 3: 接 re-export + 跑测试**

`shirita-core/src/lib.rs` 加 `pub use summarize::run as run_summary;`。

Run: `cargo test -p shirita-core --lib summarize::`
Expected: PASS（fold_range 3 + run 2 = 5）。再 `cargo test -p shirita-core` 全绿。

- [ ] **Step 4: 提交**

```bash
git add shirita-core/src/summarize.rs shirita-core/src/lib.rs
git commit -m "feat(core): summarize::run — rolling summary with self-checked threshold"
```

---

## Task 3: web 触发（SSE Done 后 fire-and-forget）+ per-session 互斥

**Files:**
- Modify: `shirita-web/src/routes/chat.rs`

- [ ] **Step 1: 写互斥 helper + `try_claim` 单测**

在 `shirita-web/src/routes/chat.rs` 顶部加 import 与互斥工具：

```rust
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use shirita_core::summarize;

/// 进程级"正在总结的 session"集合，防 fire-and-forget 并发重复（语义等价 spec §2 的 per-session 互斥）。
fn summarizing() -> &'static Mutex<HashSet<String>> {
    static S: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}
fn try_claim(session_id: &str) -> bool {
    let mut g = summarizing().lock().unwrap();
    if g.contains(session_id) {
        false
    } else {
        g.insert(session_id.to_string());
        true
    }
}
fn release(session_id: &str) {
    summarizing().lock().unwrap().remove(session_id);
}

/// 若该 session 未在总结，spawn 一个后台总结任务（不阻塞 SSE）。
fn spawn_summary(state: &AppState, session_id: String) {
    if !try_claim(&session_id) {
        return;
    }
    let storage = state.storage.clone();
    let provider = state.provider.clone();
    let counter = state.token_counter.clone();
    let model = state.model.clone();
    tokio::spawn(async move {
        summarize::run(storage, provider, counter, model, session_id.clone()).await;
        release(&session_id);
    });
}
```

在该文件底部加 `#[cfg(test)] mod tests`：

```rust
#[cfg(test)]
mod tests {
    use super::{release, try_claim};

    #[test]
    fn try_claim_is_exclusive_until_release() {
        let key = "claim-test-unique-key";
        assert!(try_claim(key));
        assert!(!try_claim(key)); // 已占用
        release(key);
        assert!(try_claim(key)); // 释放后可再占
        release(key);
    }
}
```

- [ ] **Step 2: 跑单测**

Run: `cargo test -p shirita-web --lib try_claim_is_exclusive_until_release`
Expected: PASS。

- [ ] **Step 3: 在 `send` 的 `Done` 触发 spawn**

把 `send` 里构造 `sse` 的那段改为捕获 state/session 并在 `Done` 时 spawn：

```rust
    let state_for_summary = state.clone();
    let sid_for_summary = reg_id.clone();
    let sse = events.map(move |ev| {
        if matches!(ev, SendEvent::Done { .. }) {
            spawn_summary(&state_for_summary, sid_for_summary.clone());
        }
        let payload = match ev {
            SendEvent::Delta(text) => json!({ "type": "delta", "text": text }),
            SendEvent::Done { message_id } => json!({ "type": "done", "message_id": message_id }),
            SendEvent::Error(message) => json!({ "type": "error", "message": message }),
        };
        Ok(Event::default().data(payload.to_string()))
    });
```

> `state.clone()` 在 `send_message(...)` 调用之后仍可用——`send_message` 接收的是 `state.storage.clone()` 等克隆，
> `state` 本身未被 move。`reg_id` 已是 `session_id.clone()`，再 clone 一份给闭包。

- [ ] **Step 4: 在 `regenerate_message` 的 `Done` 触发 spawn**

`regenerate_message` 做同样改动：在其 `let sse = events.map(...)` 前加
`let state_for_summary = state.clone();  let sid_for_summary = reg_id.clone();`，
把 `map(|ev| {...})` 改成 `map(move |ev| { if matches!(ev, SendEvent::Done {..}) { spawn_summary(&state_for_summary, sid_for_summary.clone()); } ... })`
（与 Step 3 同形）。

- [ ] **Step 5: 编译 + 全量回归**

Run: `cargo test --workspace`
Expected: PASS、零警告。

> 端到端"发长对话 → 后台自动产出摘要"的真实触发，放到三个 plan 完成后的整体手动验证里做
> （`summarize::run` 的折叠逻辑已由 Task 2 集成测试覆盖；本任务确保 wiring 编译且互斥正确）。

- [ ] **Step 6: 提交**

```bash
git add shirita-web/src/routes/chat.rs
git commit -m "feat(web): spawn background summarization after SSE Done with per-session mutex"
```

---

## Self-Review Checklist

- **Spec 覆盖**：§2 滚动语义（`fold_range` + `run` 用"上一摘要 + 待折叠原文" T1/T2）✓、后台 fire-and-forget 不阻塞 SSE（T3 `Done` 后 spawn）✓、per-session 互斥（T3 `try_claim`，用进程级 static 替代 AppState 字段）✓、自检阈值（`run` 用 `budget::over_threshold` T2）✓、保留最近 K 条（`fold_range` keep_recent，默认 10 T1）✓、失败不阻断（`run` 各处 `let-else`/`Err=>return` + warn T2）✓、SSE 事件不变（T3 只在闭包里 spawn，不新增事件）✓；§5 配置键 `context.window/threshold/keep_recent` + `summarize.instruction`（T1 helper + T2 读取）✓、内置默认指令（`DEFAULT_INSTRUCTION` T1）✓。
- **Placeholder 扫描**：无 TBD；每步含完整代码与命令。
- **类型一致**：`fold_range(usize, Option<usize>, usize) -> Option<(usize,usize)>`、`run(Arc<dyn Storage>, Arc<dyn ModelProvider>, Arc<dyn TokenCounter>, String, String)`、`setting_usize/f64/string(&dyn Storage, &str, default)`、`try_claim(&str)->bool`/`release(&str)`/`spawn_summary(&AppState, String)` 各处一致；`ChatRequest { model, messages, summary }` 沿用 Plan 1 定义（`summary: None`）；`Summary::new(&str,&str,&str)`、`create_summary`/`list_summaries`、`budget::over_threshold` 沿用 Plan 1。
- **依赖前置**：本 plan 依赖 Plan 1 的 `summaries` 表 / `Summary` / `ChatRequest.summary` / `budget::over_threshold` 已落地，须在 Plan 1 之后执行。
