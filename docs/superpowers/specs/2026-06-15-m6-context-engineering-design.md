# M6 — 上下文工程：自动总结 + 轻量预算（设计 spec）

> Shirita 路线图里程碑 M6（`2026-06-12-shirita-roadmap-design.md`）。建立在 M2（`assembly` 组装 + `build_chat_messages`）、
> M4（消息树 `parent_id`/`active_leaf_id` + `tree::active_path`）、M5（每条消息 `snapshot_state` 折叠）之上。本里程碑加上
> **上下文长度治理的写侧**：轻量 token 预算 + best-effort 裁剪兜底，以及（重点）**滚动重写式自动总结管道**——
> 长对话到阈值时把较早历史压缩成单一摘要、按分支隔离、不污染消息树。同时纳入多 Provider 适配（Anthropic / Ollama）。
>
> **纯后端里程碑**：本次不动前端（token 用量条、摘要指示等 UI 切片推后）。

## 目标 / 完成标志

- 组装时，若当前分支存在适用摘要，把摘要文本并入 `system` 段，水位线之前的原文不进 context；其后（cutoff 之后）原文照常带入。
- `send_message` 本轮回复落库、`Done` **立即返回**后，若用量超阈值，**后台异步（fire-and-forget）**生成滚动摘要并落库（不阻塞流）；之后某轮组装即生效。
- 摘要是**滚动重写**：每次总结 = `LLM(上一摘要 + 新折叠的原文)`，产出覆盖范围更大的**单一**摘要，规模有界，"套娃"自然消解。
- 摘要**按分支隔离**：锚定在水位线消息（`cutoff_message_id`）上；不同 active path 各自取适用摘要；fork 复制并重映射。
- 组装后若仍超硬上限，best-effort 按优先级裁剪；裁无可裁仍超时**优雅暴露错误**，不静默截断、不崩。
- 预算参数（窗口、阈值、保留窗口、总结指令）全局可配（settings），均有保守内置默认。
- 新增 `AnthropicProvider`；Ollama 经其 OpenAI 兼容端点复用现有 provider；按配置选择 provider。

## 非目标（本里程碑不做）

- 前端 UI（用量条、摘要卡片/指示）——推后。
- 完整的异步任务编排（注册表 / 取消 / 重试队列）——本次用最简 fire-and-forget + per-session 互斥；滚动语义幂等，失败下轮自然重试。
- 严格逐 token 裁剪——沿用既定决策：保守安全边际 + best-effort，溢出优雅暴露。
- 每模板/每会话独立预算与总结 prompt——本次全局可配即可。
- 编辑/隐藏历史后重算摘要——摘要随分支演进追加，旧消息文本变化不回溯重算（与 M5 不可变历史一致）。

---

## 1. 预算与裁剪（`budget` 模块，纯函数）

新 core 模块 `shirita-core/src/budget.rs`，无 I/O、可单测。

- **参数**（从 settings 读，缺省用内置默认）：`context.window`（默认 `200000`）、`context.threshold`（默认 `0.8`）。
  硬上限 = `window`；触发线 = `window * threshold`。
- **用量**：组装后的 `ChatRequest`（`messages` + `summary` 若有）文本拼接 → `TokenCounter::count`。
- **触发判断**：`prompt_tokens > 触发线` → 返回"本轮后应总结"标志（供 §2 使用）。
- **best-effort 裁剪（现实粒度说明）**：现状 `assembly::build_chat_messages` 把所有定义 + 世界书封包成**一条**
  `role=system` 消息，其后是逐条的 history（user/assistant）消息，最后是本次 user。因此在"已组装的 `ChatMessage` 序列"
  这个粒度上，能独立裁剪的只有 **history 消息**。本里程碑的裁剪即：组装后若 `prompt_tokens > window`，**从最旧的
  history 消息开始逐条丢弃**（最近对话、system 段、本次 user 一律保留），直到 `≤ window` 或只剩"系统 + 最近对话 + 本次
  user"。

  roadmap 列出的更细分层（系统 > 角色定义 > 最近对话 > 世界书 > 旧历史）需要回到 plan 段层（按 `PromptSegment` 类型禁用
  world 段）才能实现；本里程碑**不做**段级裁剪，保持 best-effort 简单——理由是滚动总结已是历史增长的主力治理手段，裁剪只作
  兜底。裁无可裁仍超 → **不强行截断**，照发请求；provider 报超长后，core 在 `send_message` 中转成明确的
  `SendEvent::Error("context overflow: …")`（roadmap 的"溢出优雅暴露"）。

- **接口形态**（示意，纯函数）：
  `fn over_threshold(prompt_tokens, window, threshold) -> bool`、
  `fn trim_history(messages: &[ChatMessage], window, counter) -> (Vec<ChatMessage>, trimmed_count)`
  （保留首条 system 与末尾若干消息，丢中段最旧的 history）。不回到节点层。

---

## 2. 滚动总结管道（`summarize` 模块）

新 core 模块 `shirita-core/src/summarize.rs`。

### 滚动语义（消解"套娃"）
把一个分支沿 active path 看成 `M1 … Mn`：
- 第一次到阈值：折叠较早的 `M1 … Mj` 为摘要 `S`，水位线 `cutoff = Mj`。组装 = 定义 + `S` + `Mj+1 … Mn`。
- 再次到阈值：**不**并列出 `S2`，而是 `S' = LLM(总结指令; S + Mj+1 … Ml)`，水位线推进到 `Ml`，`S'` 取代 `S`。

任意时刻一个分支只有**一个当前有效摘要**（覆盖"对话开头 → 水位线"），规模有界；旧摘要不被嵌套，而是作为输入被吸收。

### 触发与执行（后台异步，不阻塞流）
- **SSE 主流不变**：`send_message` / `regenerate` 的事件序列保持 `Delta… → 落库 assistant → set_active_leaf → Done`，
  `Done` **立即返回**——总结绝不在这条链上同步执行（总结是 5–20s 的长输入网络 I/O，同步会让前端在回复流完后仍卡死数十秒）。
- **触发点（web 层）**：`chat` SSE handler 在流 **drain 完之后** `tokio::spawn` 一个 detached 任务执行 `summarize::run(...)`
  （持有 `Arc<Storage>`/`Arc<ModelProvider>`/`Arc<TokenCounter>` clone + `session_id`/`model`）。前端无新事件、无需改动；
  总结在后台完成后落库，**之后某轮**组装自动用上（没赶上就再下一轮）。
- **per-session 互斥**：`AppState` 持一个 `Arc<Mutex<HashSet<String>>>`（正在总结的 session 集合）。spawn 前若该 session 已在集合中则跳过；
  任务结束（成功/失败）移除。避免连发消息时并发跑多个总结。
- **`summarize::run`（core，幂等可重入，自检阈值）**：
  1. 读 active path + 当前适用摘要；组装算用量；**未超阈值直接返回**（每轮 spawn 一次的轻量自检）。
  2. **选水位线**：沿 active path 保留最近 **K 条原文**（`context.keep_recent`，默认 `10`）；把"上一摘要水位线之后 → 保留窗口之前"的可见消息作为**待折叠段**；新水位线 = 待折叠段最后一条。待折叠段为空则返回。
  3. **构造请求**：`system` = 总结指令（settings `summarize.instruction`，缺省内置默认）；正文 = （上一摘要 if any）+ 待折叠段文本。调 provider，**聚合**全部 delta 成一段文本（非流式语义，丢弃增量、只取全文）。
  4. **落库**：插入一行 `summaries`（`cutoff = 新水位线 message id`，`content = 新摘要`）。旧摘要留表；组装只取适用且 `cutoff` 最靠后的（§3）。
- **失败不阻断**：总结调用失败仅 `tracing::warn!` 记录；本轮回复早已落库，下一轮 spawn 自然重试（幂等：水位线只前进）。

### 内置默认总结指令
core 内置一段中性默认（英文，便于多语对话），大意："Summarize the prior conversation faithfully and concisely,
preserving facts, decisions, character state and unresolved threads; output prose, no preamble." settings `summarize.instruction` 可整体覆盖。

---

## 3. 数据模型（迁移 0012）+ 组装替换 + fork

### `summaries` 表（迁移 0012）
| 列 | 说明 |
|---|---|
| `id` | 主键 |
| `session_id` | 所属会话（FK，随会话删除级联） |
| `cutoff_message_id` | 水位线：摘要覆盖到这条消息（含）及其之前的可见历史 |
| `content` | 摘要文本 |
| `created_at` | 生成时间 |

`Storage` 新增：`create_summary`、`list_summaries(session_id)`、（fork 用）按映射复制。无需改既有表。

### 组装时取用（conversation 层，保持 `assembly` 纯净）
- 取当前分支适用摘要：`cutoff_message_id` 必须落在当前 active path 上；多条时取 `cutoff` 在 path 中**最靠后**的那条（覆盖最多）。
- **context 构造**：在 `send_message` / `regenerate` 构造 `context: Vec<ChatMessage>` 时，只放 **cutoff 之后的可见消息**
  （cutoff 及之前的历史不进 context）。
- **摘要作为 `ChatRequest` 一等字段**：`ChatRequest` 扩展 `summary: Option<String>`。组装时把适用摘要文本填进该字段，
  **不**在组装层拼进 system —— 把"摘要放进请求体的哪里"下放给各 `ModelProvider` 自行决定（见 §4），这样不同 provider 能按各自
  最佳实践处理。`assemble_request` 增加摘要参数并设置 `ChatRequest.summary`；§1 的用量与裁剪把 `summary` 一并计入。
- 历史原文**保留在消息树里不动**（不设 `is_hidden`）；摘要只影响"发给模型的内容"，不影响 UI 展示与分支结构。

### fork
M4 的 fork 深拷消息并 mint 新 id。fork 时**复制源会话的 `summaries`**，按 old→new message id 映射重写 `cutoff_message_id`
（与 M4 复制 `snapshot_state`、M5 思路一致）。映射表来自 M4 fork 已有的拷贝过程。

---

## 4. 多 Provider 适配（独立切片）

每个 `ModelProvider` 自行决定 `ChatRequest.summary`（§3）放进请求体的哪里：

- **`OpenAiProvider`（含 Ollama）**：默认把 `summary` 拼到 system 段（首条 system 消息尾部，或作为紧跟其后的一条
  `role=system` 消息）——兼容、稳妥的默认行为。
  - **Ollama**：原生提供 OpenAI 兼容端点（`/v1/chat/completions`）→ 直接复用 `OpenAiProvider`，仅把 `OPENAI_BASE_URL` 指向
    Ollama（如 `http://localhost:11434/v1`）。本里程碑仅做配置/文档与一条冒烟验证，无新 provider 代码。
- **`AnthropicProvider`**：新增 `shirita-core/src/model/anthropic.rs` 实现 `ModelProvider`：
  - endpoint `/v1/messages`；header `x-api-key: <key>` + `anthropic-version: 2023-06-01`。
  - body：把 `Role::System` 段抽成顶层 `system` 字段（Anthropic 的 `messages` 不接受 `role=system`；多条 system 按序合并），其余作为 `messages`（user/assistant 交替）。
  - **`summary` 处理（Anthropic 最佳实践）**：不塞进 system，而是包成一条 `role=user` 消息
    `<history_summary>\n{summary}\n</history_summary>`，**插到可见历史的最前方**（顶层 `system` 字段只放稳定的角色/指令）。
  - SSE：解析 `event: content_block_delta` 的 `data` 里 `delta.text`（参照现有 `OpenAiProvider` 手动 `data:` 行解析的同构写法），逐段产出 `String` delta。
- **选择**：`main.rs` 现按"有无 API key"选 `Echo`/`OpenAi`；扩展为按配置（env `PROVIDER=openai|anthropic|ollama` 或 settings）选择具体 provider，缺省保持现状（无 key → Echo）。

---

## 5. 配置（settings 键，均有内置默认）

| 键 | 默认 | 含义 |
|---|---|---|
| `context.window` | `200000` | 上下文窗口 token 硬上限 |
| `context.threshold` | `0.8` | 触发总结的用量比例 |
| `context.keep_recent` | `10` | 滚动总结时保留不折叠的最近原文条数 |
| `summarize.instruction` | 内置默认 | 总结指令（覆盖内置默认） |

core 经现有 `Storage::get_setting` 读取（缺省回落内置默认）。前端设置页 UI 本里程碑不做，但键已可用、可由 settings API 写入。

---

## 6. 切片划分（三个 plan，依次执行）

- **Plan 1 — 预算 + 摘要基础设施**：`budget` 模块（预算判断 + best-effort 历史裁剪）；迁移 0012 `summaries` 表 + `Storage` 方法；
  `ChatRequest` 加 `summary` 字段；组装层按适用摘要填 `ChatRequest.summary`、`context` 只取 cutoff 之后；fork 复制并重映射摘要；
  `send_message`/`regenerate` 接入裁剪与溢出错误。
- **Plan 2 — 滚动总结管道（后台异步）**：`summarize::run`（自检阈值 + 选水位线 + 构造请求 + 聚合调用 + 落库，幂等可重入）；
  web `chat` handler 在 SSE drain 后 `tokio::spawn` 触发 + `AppState` per-session 互斥；内置默认指令 + settings `summarize.instruction` 覆盖；失败不阻断。
- **Plan 3 — Provider 适配**：`AnthropicProvider`（messages API + 顶层 system + `summary` 作 `<history_summary>` user 消息 + SSE 解析）；
  `OpenAiProvider` 处理 `summary`（拼 system）；Ollama 兼容端点复用 + provider 选择；冒烟验证。

### 自包含单元
- `budget`：纯函数，输入 messages(+summary) + 参数，输出裁剪结果/触发标志；可全单测，不依赖存储/网络。
- `summarize::run`：输入 active path + 上一摘要 + 指令 + provider，自检阈值后输出新摘要并落库；幂等可重入，与 SSE 主流解耦。
- `summaries` 表：侧带数据，按 `cutoff_message_id` 锚定，组装与 fork 各自按既定规则取用/复制。
- Provider 适配：仅实现 `ModelProvider` trait（含 `summary` 放置策略），与上下文治理逻辑完全正交。
