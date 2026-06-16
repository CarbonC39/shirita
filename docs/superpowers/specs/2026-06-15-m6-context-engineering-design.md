# M6 — 上下文工程：自动总结 + 轻量预算（设计 spec）

> Shirita 路线图里程碑 M6（`2026-06-12-shirita-roadmap-design.md`）。建立在 M2（`assembly` 组装 + `build_chat_messages`）、
> M4（消息树 `parent_id`/`active_leaf_id` + `tree::active_path`）、M5（每条消息 `snapshot_state` 折叠）之上。本里程碑加上
> **上下文长度治理的写侧**：轻量 token 预算 + best-effort 裁剪兜底，以及（重点）**滚动重写式自动总结管道**——
> 长对话到阈值时把较早历史压缩成单一摘要、按分支隔离、不污染消息树。同时纳入多 Provider 适配（Anthropic / Ollama）。
>
> **纯后端里程碑**：本次不动前端（token 用量条、摘要指示等 UI 切片推后）。

## 目标 / 完成标志

- 组装时，若当前分支存在适用摘要，把摘要文本并入 `system` 段，水位线之前的原文不进 context；其后（cutoff 之后）原文照常带入。
- `send_message` 本轮回复落库后，若用量超阈值，**同步**生成滚动摘要并落库；下一轮组装即生效。
- 摘要是**滚动重写**：每次总结 = `LLM(上一摘要 + 新折叠的原文)`，产出覆盖范围更大的**单一**摘要，规模有界，"套娃"自然消解。
- 摘要**按分支隔离**：锚定在水位线消息（`cutoff_message_id`）上；不同 active path 各自取适用摘要；fork 复制并重映射。
- 组装后若仍超硬上限，best-effort 按优先级裁剪；裁无可裁仍超时**优雅暴露错误**，不静默截断、不崩。
- 预算参数（窗口、阈值、保留窗口、总结指令）全局可配（settings），均有保守内置默认。
- 新增 `AnthropicProvider`；Ollama 经其 OpenAI 兼容端点复用现有 provider；按配置选择 provider。

## 非目标（本里程碑不做）

- 前端 UI（用量条、摘要卡片/指示）——推后。
- 后台异步总结（任务注册/并发/取消）——本次用"本轮后同步"。
- 严格逐 token 裁剪——沿用既定决策：保守安全边际 + best-effort，溢出优雅暴露。
- 每模板/每会话独立预算与总结 prompt——本次全局可配即可。
- 编辑/隐藏历史后重算摘要——摘要随分支演进追加，旧消息文本变化不回溯重算（与 M5 不可变历史一致）。

---

## 1. 预算与裁剪（`budget` 模块，纯函数）

新 core 模块 `shirita-core/src/budget.rs`，无 I/O、可单测。

- **参数**（从 settings 读，缺省用内置默认）：`context.window`（默认 `200000`）、`context.threshold`（默认 `0.8`）。
  硬上限 = `window`；触发线 = `window * threshold`。
- **用量**：组装后的 `ChatRequest.messages` 文本拼接 → `TokenCounter::count`。
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

### 触发与执行
- **触发点**：`send_message` 本轮 assistant 落库 + `set_session_active_leaf` 之后（M5 折叠之后），若 §1 标志为真 → **同步**执行。
- **选水位线**：沿 active path 保留最近 **K 条原文**（`context.keep_recent`，默认 `10`）；把"上一摘要水位线之后 → 保留窗口之前"的可见消息作为**待折叠段**；新水位线 = 待折叠段最后一条消息。若待折叠段为空（最近 K 条已覆盖全部未折叠历史），跳过。
- **构造请求**：`system` = 总结指令（settings `summarize.instruction`，缺省内置默认）；正文 = （上一摘要 if any）+ 待折叠段文本。调 provider，**聚合**全部 delta 成一段文本（非流式语义，丢弃增量、只取全文）。
- **落库**：插入一行 `summaries`（`cutoff = 新水位线 message id`，`content = 新摘要`）。旧摘要留表；组装只取适用且 `cutoff` 最靠后的（§3）。
- **失败不阻断**：总结调用失败仅 `tracing::warn!` 记录；本轮回复已落库，下一轮再尝试。
- **SSE 不变**：事件序列 `Delta… → 落库 assistant → 同步总结 → Done`，不新增事件类型（前端无需改动；`Done` 因同步总结略迟到）。

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
- **摘要注入 system 段（不作独立消息）**：把摘要文本作为新参数传入 `assemble_request`，组装时追加到那条 `role=system`
  消息**尾部**（如 `\n\n[Summary of earlier conversation]\n{content}`）。这样发给模型的 `messages` 里始终**只有一条 system**，
  跨 provider 行为一致——尤其 Anthropic 的 `messages` 不接受 `role=system`，单条 system 抽成顶层 `system` 字段无歧义（见 §4）。
- 历史原文**保留在消息树里不动**（不设 `is_hidden`）；摘要只影响"发给模型的内容"，不影响 UI 展示与分支结构。

### fork
M4 的 fork 深拷消息并 mint 新 id。fork 时**复制源会话的 `summaries`**，按 old→new message id 映射重写 `cutoff_message_id`
（与 M4 复制 `snapshot_state`、M5 思路一致）。映射表来自 M4 fork 已有的拷贝过程。

---

## 4. 多 Provider 适配（独立切片）

- **Ollama**：原生提供 OpenAI 兼容端点（`/v1/chat/completions`）→ 直接复用现有 `OpenAiProvider`，仅把 `OPENAI_BASE_URL` 指向
  Ollama（如 `http://localhost:11434/v1`）。本里程碑仅做配置/文档与一条冒烟验证，无新 provider 代码。
- **Anthropic**：新增 `shirita-core/src/model/anthropic.rs` 的 `AnthropicProvider` 实现 `ModelProvider`：
  - endpoint `/v1/messages`；header `x-api-key: <key>` + `anthropic-version: 2023-06-01`。
  - body：把 `Role::System` 段抽成顶层 `system` 字段（§3 保证组装后只有一条 system；若仍出现多条，全部按序合并进顶层
    `system`），其余作为 `messages`（user/assistant）。
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

- **Plan 1 — 预算 + 摘要基础设施**：`budget` 模块（预算判断 + best-effort 裁剪）；迁移 0012 `summaries` 表 + `Storage` 方法；
  组装层读取并替换摘要；fork 复制并重映射摘要；`send_message`/`regenerate` 接入裁剪与溢出错误。
- **Plan 2 — 滚动总结管道**：`summarize` 模块（选水位线 + 构造请求 + 聚合调用 + 落库）；`send_message` 本轮后同步触发；
  内置默认指令 + settings `summarize.instruction` 覆盖；失败不阻断。
- **Plan 3 — Provider 适配**：`AnthropicProvider`（messages API + SSE 解析）；Ollama 兼容端点复用 + provider 选择；冒烟验证。

### 自包含单元
- `budget`：纯函数，输入 messages + 参数，输出裁剪结果/触发标志；可全单测，不依赖存储/网络。
- `summarize`：输入 active path + 上一摘要 + 指令 + provider，输出新摘要文本与水位线；总结策略与存储/组装解耦。
- `summaries` 表：侧带数据，按 `cutoff_message_id` 锚定，组装与 fork 各自按既定规则取用/复制。
- Provider 适配：仅实现 `ModelProvider` trait，与上下文治理逻辑完全正交。
