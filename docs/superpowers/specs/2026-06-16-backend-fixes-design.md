# 后端审阅修复批 设计 spec

> 来源：对 `shirita-core` + `shirita-web` 的一次完整后端审阅。本 spec 覆盖审阅结论里用户选定要做的 7 项修复（编号沿用审阅顺序 ①②③④⑤⑥⑦），第 ⑧ 项（角色卡 personality/scenario/first_mes）**明确推迟**到后续单独讨论。
> 后续走独立的 plan → 实现循环。

## 1. 目标与完成标志

修掉审阅发现的一批后端缺陷，其中第 ① 项是「核心功能不可用」级别（设置里配的 provider/model 从不进入生成链路），其余为一致性、健壮性与可配置性问题。

**完成标志**：
- 在 Settings 配好 provider/key/model 后，**实际发消息走该配置**（不再恒为 env 的 Echo/默认模型）；无任何配置时仍回退到 env（离线 Echo 不破）。
- 死的第二套局部覆盖端点（`overrides.rs`）删除，仅留 `local-definitions`。
- 节点树深度被 API 强制为 2 层；非法嵌套返回 400。
- 创建/更新 `regex_rule` 定义时非法正则返回 400；运行期仍宽容。
- Anthropic `max_tokens`（= 回复最大长度）可配置，默认 8192。
- 会话 `override_config` 的局部覆盖/变量写入为单条原子 SQL，消除并发丢更新窗口。
- `cargo test` 全绿（含新增针对上述行为的单测）。

## 2. 已确认的关键决策（brainstorm 结论）

- **① provider/model 解析时机 = 生成时从 settings 解析**。新增 `resolve_provider(&AppState)`，读 `provider_source/base_url/api_key/model/max_tokens` 设置，构造匹配 provider；**settings 有配置则胜出**，否则回退到 env 构造的 `state.provider`/`state.model`。无「热重载」机制——每次生成现读现建（即解决了 ⑤「没有动态重载」）。client 复用缓存留作后续优化，不在本轮。
- **② 两套局部覆盖 = 删除 `overrides.rs`（裸字符串、死代码），保留 `local_overrides`（结构化 patch）**。前端只调 `/local-definitions`、且直接读 `session.override_config.local_definitions`，确认零前端影响。
- **③ 节点树深度 = API 层强制 2 层**（而非实现 n 层递归）。匹配现有 UI 与装配器；附带消除环→`export_template` 死循环风险。
- **④ 非法正则 = 创作期校验**（定义 create/update）。运行期 `apply_regex_rules` 保持宽容（跳过 + `warn!`）。
- **⑥ max_tokens = `ChatRequest.max_tokens: Option<u32>`，来源 `provider_max_tokens` 设置，默认 8192**。语义是「回复（输出）最大 token 数」，非上下文窗口（上下文窗口是另一个设置 `context.window`，用于 `budget.rs` 裁剪输入）。Anthropic 用 `unwrap_or(8192)`；OpenAI 仅当 `Some` 时下发（未配置时保持「省略=服务端默认」）。两端均流式，故抬高默认安全。
- **⑦ 丢更新 = 原子 SQL（JSON1）**。`override_config` 的局部定义/变量写入改为单条 `UPDATE … json_patch(COALESCE(override_config,'{}'), json_object(...))`，下沉到 storage 层；键由 `json_object` 绑参构造，不拼 path。

### 不做（YAGNI / 明确推迟）

- ⑧ 角色卡 personality/scenario/first_mes 的实际入 prompt 与开场白注入 —— 留待后续讨论。
- 节点树 n 层递归装配 / `copy_nodes` 多层拓扑（被 ③ 的「强制 2 层」取代）。
- `update_node` 的 null-vs-缺省合并修复（无法把节点移回根 / 清空字段）—— **本轮不修**，仅在 spec §8 记为后续。前端不做「移到根」，删建替代。
- `list_models` 对非 OpenAI 源（anthropic/google/cohere）的适配 —— **本轮不修**，记为后续；本轮只让 `test_connection` 复用统一 builder。
- provider client（reqwest）按 base_url 缓存复用。
- auth 常量时间比较、assets 公开静态、API key 明文存储等安全项 —— 不在本轮。

## 3. 现状与接口事实（写设计时已核实）

- `ChatRequest { model: String, messages: Vec<ChatMessage>, summary: Option<String> }`（`shirita-core/src/model/mod.rs:25`）。无 `max_tokens`。
- `AnthropicProvider::anthropic_body`（`model/anthropic.rs:54`）硬编码 `"max_tokens": 4096`。`OpenAiProvider`（`model/openai.rs`）不下发 `max_tokens`。
- 生成入口 `chat::send`/`chat::regenerate`（`routes/chat.rs:45-47,62-64,97-99`）与 `spawn_summary`（`chat.rs:48-49`）用 `state.provider`/`state.model`；二者在 `main.rs:24-34`（及 Tauri `main.rs:38-39,53`）启动时由 env 经 `provider_from_env` 一次性构造。
- `provider.rs` 的 `test_connection`/`list_models` 读 `provider_source/base_url/api_key/model` 设置，但**仅这两个端点**读；生成链路从不读。`test_connection` 恒用 `OpenAiProvider`；`default_base_url(source)`（`provider.rs:45`）已含 11 个源的默认 base url。
- `routes/overrides.rs`：`set_override` 写 `local_definitions[def_id] = Value::String(content)`（裸串）；装配器 `effective_def_content`（`assembly.rs:230`）取 `overrides.get(id).get("content")`，对裸串恒 `None` → 回退全局 → **该端点对装配无效果**。前端 grep 仅命中 `/local-definitions`（`shirita-ui/src/api/client.ts:139-149`、`BookView.vue:116` 读 `local_definitions` 为对象形）。`lib.rs:89-91` 注册 4 条 overrides 路由；`routes/mod.rs` 有 `pub mod overrides;`。
- `local_overrides.rs`：`set_local_definition`/`clear_local_definition`/`promote_local_definition` 经 `with_local_defs` 做 get→clone→mutate→`update_session_override_config` 写回。`variables.rs::set_local_variables` 同模式写 `local_variables`。
- `prompt_nodes.rs::create_node`（`:27`）：folder 用 `body.tag`，ref 必带 `definition_id`，history 经此端点 400。`update_node`（`:42`）用 `body.parent_id.or(existing.parent_id)` 合并。前端建节点：folder 恒 `parent_id: null`，ref 才 `parent_id: <folderId>`（`NewChatPromptView.vue:37,41`、`BookView.vue:280,291,496,508`）—— 实际只产 2 层树。
- 装配器 `assemble_from_nodes`（`assembly.rs:281-373`）只遍历根节点 + 根 folder 的直接子 ref；`copy_nodes`（`storage/sqlite.rs:419`）按 `(parent_id.is_some(), sort_order)` 排序，仅 2 层正确。`portable.rs::filter_enabled`（`:28`）祖先回溯 `loop{}` 无环检测 → 成环则 `export_template` 死循环。
- `definitions.rs::create`/`update`（`:72,101`）经 `validate_type` 校验 type，但不校验 `regex_rule` 的 `meta.pattern`。`assembly.rs::apply_regex_rules`（`:159`）对非法 pattern 静默 `if let Ok(re)` 跳过。
- storage：`SqliteStorage` 用 sqlx + SQLite，`foreign_keys(true)`，WAL。`update_session_override_config`（`sqlite.rs:440`）整列覆盖写。`reorder_sessions`（`:242`）已用事务，`reorder_nodes`（`:410`）未用（非本轮）。
- settings 表：`get_setting/set_setting`（JSON 值）。`context.window` 默认 200000（`conversation.rs:30`、`summarize.rs:69`）。
- SQLite JSON1：sqlx 0.8 捆绑的 SQLite 含 `json`, `json_set`, `json_remove`, `json_extract`, `json_patch`, `coalesce`。

## 4. 逐项设计

### ① provider/model 生成时解析（最高优先）

**共享 reqwest::Client 单例**：`AppState` 增字段 `http_client: reqwest::Client`，在两处入口（`shirita-web/src/main.rs`、`shirita-tauri/src/main.rs`）启动时各建一次。`reqwest::Client` 克隆即共享底层连接池（内部 Arc），故所有 provider（含 env 兜底）复用同一 client，**杜绝 per-generation `Client::new()`**。为此，provider 构造函数改为接收 client：`OpenAiProvider::new(client, base_url, api_key)` / `AnthropicProvider::new(client, base_url, api_key)`（`EchoProvider` 无需 client）。

**新增**（`shirita-web/src/provider_select.rs` 内）一个异步解析器与一个纯构造函数：

```rust
/// 由 source/base_url/api_key + 共享 client 构造 provider（纯决策，复用 default_base_url）。
fn build_provider(client: reqwest::Client, source: &str, base_url: &str, api_key: &str)
    -> Arc<dyn ModelProvider> {
    match source {
        "anthropic" => Arc::new(AnthropicProvider::new(client, base_url, api_key)),
        "ollama"    => Arc::new(OpenAiProvider::new(client, base_url, "ollama")),
        _           => Arc::new(OpenAiProvider::new(client, base_url, api_key)), // openai 兼容
    }
}

/// 生成时解析：settings 有配置则胜出，否则回退 env 构造的 state.provider/model。
/// 返回 (provider, model, max_tokens)。复用 state.http_client。
pub async fn resolve_provider(state: &AppState)
    -> (Arc<dyn ModelProvider>, String, Option<u32>);
```

- 读设置：`provider_source`、`provider_base_url`、`provider_api_key`、`provider_model`、`provider_max_tokens`（u64→u32）。
- **「已配置」判定**：`provider_source` 非空 **或** `provider_api_key` 非空 **或** `provider_model` 非空。满足则用 `build_provider(state.http_client.clone(), …)` 构造：`base_url` 缺省走 `default_base_url(source)`；`model` 缺省走 `state.model`（兜底）。否则整体回退 `(state.provider.clone(), state.model.clone())`。这样无任何配置（桌面首启）仍是 env 的 Echo/默认，不破离线。
- `provider_from_env(config, client)` 增 client 形参，把 `client.clone()` 传给各 provider 构造（env 兜底也共享单例）。
- `default_base_url` 从 `provider.rs` 提升为该模块内 `pub(crate)` 复用（或移到此处），`provider.rs` 改为引用。

**接线**：
- `chat::send`、`chat::regenerate`、`spawn_summary`：把 `state.provider.clone()`/`state.model.clone()` 换成 `let (provider, model, max_tokens) = resolve_provider(&state).await;`。`max_tokens` 需透传进 `ChatRequest`（见 ⑥），故 `send_message`/`regenerate`/`summarize::run` 的签名增加一个 `max_tokens: Option<u32>` 参数，下沉到 `assemble_request` 写入 `ChatRequest.max_tokens`。
- `provider::test_connection`：改用 `build_provider(state.http_client.clone(), source, base_url, api_key)` 而非恒 `OpenAiProvider`，使「测试」与真实生成一致（顺带让 anthropic/ollama 源能正确测试）。`list_models` 本轮不动（OpenAI 形，记为后续）。

**注**：provider 实例每生成新建（轻量：仅持 client clone + url/key 字符串），底层 `reqwest::Client` 是 AppState 单例，无 per-call `Client::new()`。

### ② 删除第二套覆盖（`overrides.rs`）

- 删除文件 `shirita-web/src/routes/overrides.rs`。
- `routes/mod.rs` 删 `pub mod overrides;`。
- `lib.rs` 删 4 条路由（`:89-91`）：`/sessions/{id}/overrides`（GET）、`/sessions/{id}/overrides/{def_id}`（PUT/DELETE）、`/sessions/{id}/overrides/{def_id}/promote`（POST）。
- 前端无引用，无需改 UI。`tests/` 若有覆盖 overrides 的用例需删（写实现时核查 `local_overrides_test.rs` 与 grep `/overrides`）。

### ③ API 强制节点树 2 层

`prompt_nodes.rs` 内新增校验，`create_node` 与 `update_node`（仅当 `parent_id` 改变）共用：

- **folder / history**：`parent_id` 必须为 `None`，否则 `400`。
- **ref**：`parent_id` 为 `None`（根 ref）合法；若 `Some(pid)`，则 `pid` 必须指向一个**存在、`kind == Folder`、且 `owner_kind/owner_id` 与本节点相同**的节点，否则 `400`。
- 校验需读父节点：`update_node` 已有 `existing`，`create_node` 按 `owner_kind/owner_id` 从 `list_nodes` 或 `get_node(pid)` 取父校验。

效果：树恒 ≤2 层，`copy_nodes`/装配的 2 层假设始终成立；且无法再经 API 造环 → `filter_enabled` 死循环风险关闭（既有 DB 数据本就 2 层）。

### ④ 创作期正则校验

- `shirita-core`（`assembly.rs`）新增、并经 `lib.rs` 重导出：
  ```rust
  pub fn is_valid_regex(pattern: &str) -> bool { regex::Regex::new(pattern).is_ok() }
  ```
- `definitions.rs::create`/`update`：当 `body.r#type == "regex_rule"`，取 `meta.pattern`（若有且非空），`!is_valid_regex(p)` → `400`（`BAD_REQUEST`）。pattern 缺省/空：放行（空规则无副作用）。
- 运行期 `apply_regex_rules`：`Regex::new(p)` Err 分支加 `tracing::warn!`（仍跳过，不中断生成）。

### ⑥ 可配置 max_tokens

- `ChatRequest` 增字段 `pub max_tokens: Option<u32>`（`model/mod.rs`）。所有构造点补该字段：
  - `assemble_request`（`conversation.rs`）：`max_tokens` 由上层透传（见 ①）。
  - `summarize::run`：签名加 `max_tokens: Option<u32>`，由 `spawn_summary` 经 `resolve_provider` 透传，写入摘要 `ChatRequest`（与 send/regenerate 一致；未配置时 `None` → Anthropic 默认 8192 / OpenAI 省略）。
  - `provider::test_connection` 的 ping：`max_tokens: Some(16)`（廉价探针）。
  - 测试内的 `ChatRequest` 字面量补 `max_tokens: None`。
- `anthropic_body`：`"max_tokens": req.max_tokens.unwrap_or(8192)`。
- `openai_messages`/body：仅当 `req.max_tokens` 为 `Some(v)` 时在 body 加 `"max_tokens": v`；`None` 保持现状（不下发）。
- `provider_max_tokens` 设置经 ① 的 `resolve_provider` 读出并透传；未配置时为 `None` → Anthropic 取 8192 默认，OpenAI 省略。

### ⑦ 原子 override_config 写入（JSON1）

`Storage` trait 新增 3 个原子方法，sqlite 实现用单条 `UPDATE`：

```rust
async fn set_local_definition(&self, session_id: &str, def_id: &str, patch: &Value) -> Result<()>;
async fn clear_local_definition(&self, session_id: &str, def_id: &str) -> Result<()>;
async fn set_local_variables(&self, session_id: &str, variables: &Value) -> Result<()>;
```

统一用 `json_patch` + `json_object`（RFC 7396 合并补丁），键由 `json_object` 绑定参数构造，**不拼 JSON path 字符串**：

- **set_local_definition**：把 `{"local_definitions": {"<def_id>": <patch>}}` 合并进现有 config（父对象不存在则由合并创建）：
  ```sql
  UPDATE chat_sessions
  SET override_config = json_patch(
      COALESCE(override_config, '{}'),
      json_object('local_definitions', json_object(?2, json(?3))))
  WHERE id = ?1
  ```
  绑定：`?1=session_id, ?2=def_id, ?3=serde_json::to_string(patch)`。
- **clear_local_definition**：合并补丁里把该键置 JSON null，RFC 7396 即删除该键（不动其它 def）：
  ```sql
  UPDATE chat_sessions
  SET override_config = json_patch(
      COALESCE(override_config, '{}'),
      json_object('local_definitions', json_object(?2, json('null'))))
  WHERE id = ?1
  ```
- **set_local_variables**（整列替换数组 —— RFC 7396 对数组是整体替换，正合所需）：
  ```sql
  UPDATE chat_sessions
  SET override_config = json_patch(
      COALESCE(override_config, '{}'),
      json_object('local_variables', json(?2)))
  WHERE id = ?1
  ```
- 路由改写：
  - `local_overrides.rs::set_local_definition`/`clear_local_definition` 调对应原子方法（删 `with_local_defs` 的 get→clone→write 路径；`with_local_defs` 可保留供 `promote` 的 clear 复用或一并替换为 `clear_local_definition`）。
  - `local_overrides.rs::promote_local_definition`：定义更新（另一张表）保持；末尾的「清 local」改调 `clear_local_definition`（原子）。读 patch 仍需先 `get_session`（仅读，无写竞争）。
  - `variables.rs::set_local_variables` 调 `set_local_variables`。
- 这些方法把读改写收敛为单条 UPDATE，消除并发写同一 session override_config 的丢更新窗口。

> 语义说明：`json_patch` 对 `local_definitions.<def_id>` 是**递归合并**（非整列替换该 def 的 patch 对象）。前端每次都发完整 patch（copy-on-write 全量对象），故合并结果与「替换」一致；此为有意接受的语义。键经 `json_object(?2, …)` 绑定参数构造，SQLite 负责 JSON 键转义，无 path 拼接、无注入面。clear 用 `json('null')` 触发 RFC 7396 删除。promote 仍先读 patch（仅读）+ 更新定义表 + 调 `clear_local_definition`。

## 5. 影响面与文件清单

- `shirita-core/src/model/mod.rs`：`ChatRequest` 加 `max_tokens`。
- `shirita-core/src/model/anthropic.rs` / `openai.rs`：构造函数 `new` 首参改为 `reqwest::Client`；body 用 `req.max_tokens`。
- `shirita-web/src/state.rs`：`AppState` 加 `http_client: reqwest::Client`。
- `shirita-web/src/main.rs`、`shirita-tauri/src/main.rs`：启动时建一个 `reqwest::Client`，传给 `provider_from_env` 并存入 `AppState`。
- `shirita-core/src/assembly.rs`：导出 `is_valid_regex`；`apply_regex_rules` 加 warn。
- `shirita-core/src/conversation.rs`：`send_message`/`regenerate`/`assemble_request` 透传 `max_tokens`。
- `shirita-core/src/summarize.rs`：`run` 透传 `max_tokens`（或固定 `None`）。
- `shirita-core/src/storage/mod.rs` + `sqlite.rs`：trait 加 3 原子方法 + 实现。
- `shirita-web/src/provider_select.rs`：`build_provider` + `resolve_provider` + `default_base_url` 迁入/复用。
- `shirita-web/src/routes/chat.rs`：send/regenerate/spawn_summary 用 `resolve_provider`。
- `shirita-web/src/routes/provider.rs`：`test_connection` 用 `build_provider`；`default_base_url` 引用迁移。
- `shirita-web/src/routes/definitions.rs`：regex_rule 校验。
- `shirita-web/src/routes/prompt_nodes.rs`：2 层校验。
- `shirita-web/src/routes/local_overrides.rs` / `variables.rs`：调原子方法。
- 删除 `shirita-web/src/routes/overrides.rs`；改 `routes/mod.rs`、`lib.rs`。
- 测试：`tests/` 中 overrides 相关删除；新增/调整下列单测。

## 6. 测试策略

- **provider 解析**（`provider_select.rs` 单测）：settings 全空 → 回退 env（Echo）；设 `provider_source=anthropic` + key → `build_provider` 得 Anthropic；OpenAI 兼容源 → OpenAi；`provider_model` 覆盖 model；`provider_max_tokens` 读出为 `Some`。
- **生成走 settings**（web 集成测，用现有 `RecordingProvider` 思路较难，因 provider 由 settings 构造真 HTTP）——退一步：单测 `resolve_provider` 的决策即可；端到端「设置→生成」由 `build_provider` + 决策覆盖。
- **overrides 删除**：确认路由 404（或编译期移除）；`local-definitions` 行为不变（既有 `local_overrides_test.rs` 仍绿）。
- **2 层强制**：folder 带 parent → 400；ref parent 指向 ref/history/异 owner/不存在 → 400；ref parent 指向同 owner 根 folder → 201。
- **正则校验**：create/update `regex_rule` 带非法 pattern → 400；合法 / 无 pattern → 201。
- **max_tokens**：`anthropic_body` 在 `Some(8192)`/`None(→8192)` 下输出对应值；`openai_messages` 仅 `Some` 时含 `max_tokens`。
- **原子覆盖**（storage 单测）：`set_local_definition` 在 `override_config` 无 `local_definitions` 时由合并创建并写键；`clear`（`json('null')`）删该键且不动同对象其它 def；`set_local_variables` 整列替换数组；二次 set 同键合并为最新 patch；不串其它会话。
- 回归：`cargo test` 全绿。

## 7. 风险与缓解

- **resolve 时机改 provider 实例**：provider 每生成新建（仅 client clone + 字符串），底层 `reqwest::Client` 为 AppState 单例、连接池复用，无 per-call `Client::new()`。摘要也走 settings provider，行为一致。
- **OpenAI 默认下发 max_tokens 的语义**：本设计选择「仅 `Some` 时下发」，未配置不改 OpenAI 现状；只有 Anthropic 默认从 4096 抬到 8192（流式，安全）。
- **JSON1 path 注入**：仅 `local_definitions` 子键，键为 UUID，安全（§4⑦ 注）。
- **2 层强制与既有数据**：UI 历来只产 2 层，无迁移；强制仅作用于新写。

## 8. 后续（本轮不做，记录在案）

- ⑧ 角色卡 personality/scenario/first_mes 入 prompt 与开场白注入（单独讨论）。
- `update_node` 区分 null 与缺省（修「移到根 / 清字段」）。
- `list_models` 适配非 OpenAI 源（anthropic/google/cohere）。
- provider client 缓存复用；`reorder_nodes` 事务化；auth 常量时间比较；API key 加密存储。
