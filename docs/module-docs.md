# Shirita 模块文档

> 生成日期：2026-07-06

本文档由自动化工具生成，覆盖 shirita-core、shirita-web、shirita-tauri、shirita-ui 四个 crate/package 的全部模块。

---

# 一、shirita-core — 核心业务库

# shirita-core 模块文档

---

## error

- **职责**：定义核心错误类型和 Result 别名。
- **输入/输出**：`Error` 枚举（thiserror 派生），包含 Database、Migration、Serde、InvalidDefinitionType、Config 五个变体。`Result<T>` 是 `std::result::Result<T, Error>`。
- **依赖关系**：被全 crate 使用；依赖 sqlx、serde_json 错误类型。
- **实现方式**：标准的 thiserror derive pattern，未自定义 From 实现以外的逻辑。
- **⚠️ 需要你关注的点**：无。代码简洁正确。

---

## config

- **职责**：加载运行时配置（数据库路径、资源目录、Token 密钥、OpenAI 参数、HTTP Basic Auth 凭据）。
- **输入/输出**：`Config` 结构体暴露全部字段。`Config::from_env()` 从环境变量读取。
- **依赖关系**：被 web 层和 Tauri 入口点调用。`apply_provider_env()` 和 `apply_http_auth_env()` 可被共享入口复用。
- **实现方式**：`Config::new()` 校验 TOKEN_SECRET 非空；`from_env()` 组合环境变量读取；HTTP Auth 要求在 HTTP_AUTH_USER 和 HTTP_AUTH_PASS 同时存在时才启用，避免部分配置造成的安全隐患。
- **⚠️ 需要你关注的点**：
  - `openai_api_key` 默认为空字符串，未设置环境变量时可能造成无声的 API 认证失败。
  - `apply_provider_env()` 对 OPENAI_API_KEY 使用 `unwrap_or_default()`，缺省时不会报错。

---

## storage (mod + sqlite)

- **职责**：定义 Storage trait 抽象（约 50 个方法）和 SQLite 实现（SqliteStorage）。
- **输入/输出**：trait 覆盖 definition、session、message、template、prompt_node、pack、summary、setting、def_type、asset 的 CRUD。
- **依赖关系**：被 conversation、panels、seed 模块依赖。SQLite 实现依赖 sqlx 连接池和 migration。
- **实现方式**：
  - 连接配置使用 WAL 模式 + 5 连接池 + 5 秒 busy timeout 避免 SQLITE_BUSY。
  - 事务用在需要原子性的场景：delete_session、create_message_and_advance_leaf、import_pack、import_template、create_template_with_nodes、create_messages 等。
  - override_config 的原子更新使用 SQLite json_patch() + json_object() 实现，避免 read-rewrite 竞态。
  - delete_message_subtree 使用内存 BFS 收集子树 ID，仅在需要时回写 active_leaf。
  - materialize_owner_nodes 利用 meta._source 标记实现幂等性。
- **⚠️ 需要你关注的点**：
  - **delete_node**：先删子节点再删自身，但两次查询不在同一个事务中。并发操作下可能出现中间状态。
  - **测试助手 temp_storage()**：使用 `std::mem::forget(dir)` 故意泄露 tempdir 防止测试过程中被删除。是意图行为，但需要留意。
  - **import_pack / import_template 依赖调用者保证 parent-before-child 顺序**。违反时 FK 约束会导致事务回滚，但错误信息不直观。
  - **orphaned_definitions_for_template 和 orphaned_definitions_for_pack** 的 SQL 查询模式完全相同，仅 owner_kind 和 owner_id 不同，本应抽象为通用方法。
  - **create_message** 在插入消息后同步更新 session 的 sort_order 到当前时间戳，这会覆盖任何已有的手动排序值。
  - **message_preview** 的 SCAN_LIMIT=2000 硬编码。

---

## models (mod + 8 子模块)

包含数据模型定义文件，都是纯结构体 + new() 构造函数 + serde 序列化。所有模块代码简洁，无异常处理缺失或实现问题。

### asset
资产记录：id、name、path、kind（"avatar"/"background"）、hash（可选）、created_at。默认 kind="background"。

### definition
定义（知识条目）：id、def_type（扩展字符串）、name、content、meta。

### def_type
容器类型注册表：预定义 8 个保留类型（prompt、regex_rule、tool、first_message、protocol、html、css、variables）。提供 is_reserved() 和 is_prompt()。

### message
消息模型：id、session_id、parent_id、role、raw_content、display_content、is_hidden、is_anchor、attachments、snapshot_state。

### pack
内容包：name、identity（PackIdentity）、meta。

### prompt_node
节点：kind（Folder/Ref/History/Content）、owner_kind（Template/Session/Pack）、owner_id、parent_id、tag、definition_id、enabled、meta。

### session
会话：template_id、override_config、current_state、mounted_definitions、mounted_packs、active_leaf_id、preview（不持久化）。

### summary
滚动摘要：id、session_id、cutoff_message_id、content、created_at。

### template
模板：name、meta。

---

## conversation

- **职责**：核心对话服务——发送消息（send_message）、重新生成（regenerate）、管理停止令牌（StopHandle/StopToken）。
- **输入/输出**：send_message/regenerate 返回 `impl Stream<Item = SendEvent>`，事件类型为 Delta/Done/Stopped/Error。effective_nodes、mounted_pack_trees、effective_regex_rules 提供辅助查询。
- **依赖关系**：依赖 Storage、ModelProvider、TokenCounter、assembly、state、attachments、html_patch、budget、tree。被 web 层的 handler 直接调用。
- **实现方式**：
  - send_message 流程：检查 session 存在 -> 获取 active leaf 路径 -> 读取 branch state -> 存储 user 消息 -> 查找摘要 -> 组装 context -> assemble_request 构建 API 请求 -> budget trim -> provider streaming -> 累积 delta -> 解析 state_update + capture_vars -> 折叠 snapshot -> 存储 assistant 消息 -> 更新 active leaf -> 发送结束事件。
  - assemble_request 加载 template/session/pack 节点树和定义，调用 assemble_from_nodes_with_packs，注入 protocol 指令，应用 prompt 侧正则，build_chat_messages。
  - regenerate 流程与 send_message 高度相似，区别在于：使用已有 user 消息的 parent_id 作为上下文起点，隐藏旧的 assistant 消息，创建兄弟消息。
- **⚠️ 需要你关注的点**：
  - **send_message 和 regenerate 存在大量代码重复**（流循环、trim 逻辑、state_update 处理等）。违反 DRY 原则。
  - **load_defs 与 assemble_request 中的定义加载逻辑重复**。
  - **send_message** 中 `let _ = storage.set_session_active_leaf(...)` 静默忽略错误。
  - **StopToken::cancelled()** 实现复杂：当发送者被 drop（channel closed）时永久挂起而非返回——是设计意图但值得关注。

---

## assembly

- **职责**：Prompt 组装核心——从节点树解析 Entry、计算激活集（constant/keyword/random）、变量渲染、注释剥离、正则规则处理、XML 标签包装、消息段排序。
- **输入/输出**：assemble_from_nodes/assemble_from_nodes_with_packs 返回 AssembledPlan（含 segments、regex_rules）。build_chat_messages 将 plan + history 转为 Vec<ChatMessage>。
- **依赖关系**：依赖 state（变量渲染）、keyword（Aho-Corasick 扫描）、models。被 conversation 调用。
- **实现方式**：
  - activate() 分三轮：先处理 constant/random 激活，然后按 scan_depth 分组使用 KeywordIndex 扫描最近消息，最后对 recursive 条目进行最多 3 轮的递归扫描。
  - assemble_from_nodes_with_packs 遍历 template/session 树 + pack 树，处理 History/Content/Folder/Ref 四种节点类型。
  - build_chat_messages 按 BeforeHistory/AfterHistory 排序；当 history 末尾是用户 turn 时将其抽出，放到 AfterHistory/protocol 之后、作为最后一条消息重新追加，确保 provider 看到的对话以当前用户 turn 结尾。再合并相邻同 role 消息。
- **⚠️ 需要你关注的点**：
  - **render_vars** 每次调用都创建新的 regex::Regex，未使用 LazyLock 或静态。
  - **apply_regex_rules_for** 中对编译失败的正则仅做 tracing::warn 并跳过，做得到"运行时宽容"。

---

## state

- **职责**：变量状态沙箱——声明 schema、合并有效状态、解析和应用 `<state_update>` 指令。纯函数，无 I/O。
- **输入/输出**：parse_state_updates、apply_updates、effective_state、variables_from_nodes、resolve_schema_from_bricks。
- **依赖关系**：纯函数，依赖 models。被 conversation 调用。
- **实现方式**：
  - effective_state 使用三层覆盖：schema_initials < seed < leaf_snapshot。
  - apply_updates 对每个操作按 schema 类型进行类型检查，忽略未声明键或类型不匹配的操作。
  - List 类型有 MAX_LIST_LEN=1000 上限防止无界增长。
- **⚠️ 需要你关注的点**：
  - **num_value** 将 f64 转为 i64（当小数部分为 0），当绝对值 >= 1e15 时可能丢失精度。
  - **coerce 对 Number 类型过滤 NaN/inf**：防止 null 写入导致下次 Add/Sub 读出 0.0。设计正确。

---

## budget

- **职责**：Context budget 管理和历史裁剪。纯函数。
- **输入/输出**：over_threshold 检查是否超过阈值；trim_history 返回（保留消息, 丢弃数量）。
- **依赖关系**：依赖 TokenCounter 和 models。被 conversation 调用。
- **实现方式**：trim_history 保护第一条 system 消息和最后一条 user 消息，从中间段从旧到新丢弃消息直到 token 总量 <= window。
- **⚠️ 需要你关注的点**：
  - 算法只从中间段丢弃，如果第一条消息不是 system 且数据量极大，仍可能超出窗口。这是 best-effort 设计。

---

## model (mod + anthropic + openai + echo)

- **职责**：LLM 模型提供商抽象层——统一流式聊天接口。
- **输入/输出**：ModelProvider::stream_chat(ChatRequest) -> Result<BoxStream<Result<String>>>。
- **实现方式**：
  - decode_utf8_chunk 处理跨块边界的多字节字符。
  - render_delta/close_reasoning 将 reasoning_content 包装为 `<think>...</think>` 标签。
- **⚠️ 需要你关注的点**：
  - **ChatMessage::default()** 中 role 默认为 User，可能导致意外行为。
  - **TiktokenCounter::new()** 如果 tiktoken BPE 数据文件缺失，会在运行时 panic。
  - 两个 provider 的错误处理模式中 resp.text().await.unwrap_or_default() 使用 fallback，但如果 .text() 返回错误，返回空字符串——可能丢失错误细节。

---

## tokenizer (mod + tiktoken)

- **职责**：Token 计数抽象——用于 budget 显示和日志。
- **实现方式**：TiktokenCounter 使用 o200k_base（GPT-4o 编码）作为所有 provider 的统一近似计数。
- **⚠️ 需要你关注的点**：
  - 对所有 provider 使用同一个 o200k_base 编码，注释说明这是近似值。
  - **expect()** 在模块加载时可能 panic。

---

## summarize

- **职责**：滚动摘要管道——自检阈值 -> 选择折叠范围 -> 构造聚合请求 -> 调用 provider -> 存储摘要。
- **输入/输出**：run() 是 fire-and-forget 后台函数，不返回结果。
- **实现方式**：fold_range 计算折叠范围；构造请求时携带 Previous summary + 显式列出的 role/content 对。
- **⚠️ 需要你关注的点**：
  - **存在 4 个几乎相同的 setting_\* 函数**（setting_usize、setting_f64、setting_string、setting_bool）。可以泛化。
  - **run() 的早期返回策略**——session 不存在时静默返回。是 design intent 但调试时可能令人困惑。

---

## seed

- **职责**：首次启动数据播种——确保默认模板、内置 definition、资产 hash 回填。所有函数幂等。
- **实现方式**：ensure_builtin_definitions 创建两个内置 protocol definition。ensure_default_template 空表时创建 Default 模板。
- **⚠️ 需要你关注的点**：
  - **ensure_default_template** 和 **ensure_templates_have_content_node**：create_template 和 create_node 不在同一事务中。服务器在两者之间崩溃可能产生不一致状态。但这些函数在启动时只运行一次，实际风险低。

---

## panels

- **职责**：Panel 解析——从节点树收集 panel 文件夹，组装为 HTML/CSS/caps 负载。
- **输入/输出**：collect_panels 纯函数；resolve_session_panels 异步版本。
- **实现方式**：先索引子节点一次，然后遍历 panel 文件夹拼接 HTML/CSS。名称取自 meta.name -> 第一个 html 名称 -> "Panel"。
- **⚠️ 需要你关注的点**：resolve_session_panels 为每个 pack 独立加载定义，与 conversation 中的加载存在三重加载，但不同模块关注不同范围，是设计折衷。

---

## attachments

- **职责**：将消息附件的资产 ID 解析为 provider 可直接使用的 data URL。
- **实现方式**：mime_from_ext + resolve_images（查资产 -> 路径遍历检查 -> 读文件 -> base64 编码）。
- **⚠️ 需要你关注的点**：
  - **mime_from_ext** 的 `rsplit('.').next().unwrap_or("")`——rsplit 对任何长度字符串返回至少一个元素，fallback 用不到。
  - **路径遍历检查只检查 `..` 段**，不防御符号链接。
  - **透明跳过缺失文件**：不报错，调用方完全不知附件是否被解析。设计意图是"不阻塞请求"。

---

## html_patch

- **职责**：HTML 卡片 SEARCH/REPLACE 补丁——允许模型用紧凑补丁编辑 HTML 文档。
- **输入/输出**：parse_patches、apply_patches、reconstruct、is_html_document。
- **实现方式**：strip_leading_fence 移除 Markdown fence；parse_patches 逐行解析；apply_patches 要求每个 find 在文档中唯一出现。
- **⚠️ 需要你关注的点**：代码质量优秀，全面测试覆盖。无问题。

---

## tree

- **职责**：消息树辅助函数——活性路径和最深叶子节点。
- **实现方式**：active_path 从叶子向上追溯直到根再反转；deepest_leaf 沿最新子节点向下。
- **⚠️ 需要你关注的点**：代码简洁，无问题。

---

## keyword

- **职责**：多模式关键字匹配（Aho-Corasick）。
- **实现方式**：构建自动机后单次扫描文本获取匹配 ID。
- **⚠️ 需要你关注的点**：AhoCorasick::new 前的空 pattern 检查确保不会用空列表调用 new。

---

## identity

- **职责**：从 session 定义中解析聊天双方身份。
- **实现方式**：在 enabled ref 节点中按 def_type 过滤；pack identity 覆盖定义值。
- **⚠️ 需要你关注的点**：代码清晰，测试全面。无问题。

---

## hashing

- **职责**：SHA256 哈希。
- **实现方式**：标准 SHA256 摘要 + hex 格式化。
- **⚠️ 需要你关注的点**：`let _ = write!(...)` 忽略格式化错误——对 String 的 write! 永远不会失败。安全。

---

## portable

- **职责**：shirita 可移植文档格式导入/导出（纯数据转换）。
- **实现方式**：inline_subtree 用 local_id 重组 JSON；filter_enabled 含循环检测；collect_pack_assets 仅扫描指定字段。
- **⚠️ 需要你关注的点**：
  - **s() 辅助函数在 key 缺失时静默返回空字符串**——可能掩盖格式错误的数据。
  - **filter_enabled 的循环检测**：每个节点单独维护 seen set。考虑到 API 限制为 2 级树，循环不可能发生。
  - 代码质量高，测试覆盖全面。

---

## 跨模块关注点汇总

### 潜在 panic

| 位置 | 调用 | 风险 |
|------|------|------|
| tokenizer/tiktoken.rs:19 | `o200k_base().expect(...)` | tiktoken BPE 数据缺失时 panic |

### 代码重复

- send_message 和 regenerate（conversation.rs）流循环、trim、state_update 处理重复
- import_pack 和 import_template（storage/sqlite.rs）插入模式相同
- orphaned_definitions_for_template 和 orphaned_definitions_for_pack SQL 仅 owner_kind 不同
- setting_\* 四个函数（summarize.rs）结构相同
- load_defs 与 assemble_request 中的定义加载重复

### 事务缺失

- seed.rs 中 ensure_default_template 创建模板和节点不在同一事务
- seed.rs 中 ensure_templates_have_content_node 同理
- storage/sqlite.rs 中 delete_node 两个 DELETE 不在事务中

### TODO/FIXME/HACK

所有 38 个文件中未发现任何 TODO、FIXME、HACK 残留。

### 静默错误吞没

- attachments.rs::resolve_images 丢失资产时静默跳过
- conversation.rs::send_message 第 519 行静默忽略 set_session_active_leaf 错误
- conversation.rs::load_defs 定义加载失败时静默跳过
- summarize.rs::run 所有错误仅记录 warn 日志

### Tauri vs Web

本 crate 不包含任何 Tauri 或 web 特定代码——是纯粹的业务逻辑库。所有平台差异在 web 层和 Tauri 入口处理。


---

# 二、shirita-web — Axum REST API 层

# shirita-web `src/` 模块文档

---

## `lib.rs` -- Router setup, CORS, app entry

- **职责**：应用的路由定义、CORS 配置、HTTP 客户端构建。这是 `shirita-web` crate 的库入口，将路由、中间件、状态组装成一个 axum `Router` 供 `main.rs` 调用。
- **输入/输出**：
  - `new_http_client()` -> `reqwest::Client`：共享 HTTP 客户端，10 秒连接超时，无整体请求超时（流式生成需要长时间运行）。
  - `app(state: AppState) -> Router`：构造完整路由树。公开路径 `/health` 不经过 Bearer 中间件；`/api/*` 子路由全部受 `auth::require_bearer` 保护。`GET /assets/*` 走 `ServeDir` 直接服务静态文件。`embed-ui` feature 开启时，`/`、`/static/*` 和 SPA fallback 由 embed 模块处理；否则 `GET /` 返回 `static/index.html`。
  - `app_with_cors(state: AppState) -> Router`：在 `app()` 之上包装 CORS 层，允许 Tauri WebView 的 origin（`tauri://localhost`、`https://tauri.localhost`、`http://localhost:*`、`http://127.0.0.1:*`）。用于桌面端内嵌服务器。
  - `is_desktop_origin(origin) -> bool`：判断请求 origin 是否为桌面 WebView 的合法 origin。
- **依赖关系**：依赖 `routes::*` 下所有模块的路由处理函数、`auth::require_bearer` 和 `auth::require_basic` 中间件、`embed::*`、`AppState`、`Generations`、`provider_select::*`。
- **实现方式**：
  - 定义 `protected` 子路由（全部挂载在 `/api` 下），逐一注册约 30+ 条 REST 路由，覆盖 sessions、messages、definitions、templates、packs、nodes、assets、settings、provider、import/export、regex_rules、types、local_overrides、chat（SSE）等。
  - 路由外加 `route_layer` 挂载 `auth::require_bearer` 中间件（Bearer token 验证）。
  - 最外层 `require_basic` 中间件施加 HTTP Basic Auth（当配置了 `HTTP_AUTH_USER/PASS` 时），保护包括 `/`、`/health`、`/assets/*` 在内的全部路径。
  - 由 `with_state()` 注入 `AppState`。
  - CORS 层通过 `AllowOrigin::predicate` 动态校验，用于桌面端开发和生产模式。
- **⚠️ 需要你关注的点**：
  - `new_http_client()` 中 `build().unwrap_or_default()`：如果 `reqwest::Client::builder()` 构建失败（极低概率），`unwrap_or_default` 会返回 `Default::default()`，但 `Client` 没有实现 `Default`——这段代码实际会 panic。这是 `reqwest` 的一个已知坑：实际上 `Client` 实现了 `Default`（通过 `DefaultClient`），所以不会 panic，但显式的 `.build().expect(...)` 会更清晰。
  - CORS 白名单允许任何 `http://localhost:<port>` 和 `http://127.0.0.1:<port>`，这意味着同一台机器上的任何其他 web 应用可以发送跨域请求到 shirita 的桌面端服务器。但注释说明了风险（Bearer token 认证保护 `/api`），这算是一种可接受的权衡。

---

## `main.rs` -- Web server entry point (binary)

- **职责**：二进制入口。初始化 tracing、配置、数据库、确保必要数据存在、构建 `AppState`、启动 axuma HTTP 服务器。
- **输入/输出**：
  - `main()` -> `Result<(), Box<dyn Error>>`：异步 main 函数。
  - `is_loopback_bind(addr: &str) -> bool`：判断 BIND_ADDR 是否为本地回环地址。
- **依赖关系**：依赖 `shirita_core`（`Config`、`SqliteStorage`、`TiktokenCounter` 等）和 `shirita_web`（`app`、`new_http_client`、`provider_from_env`、`AppState`）。
- **实现方式**：
  1. 初始化 tracing（从环境变量 `RUST_LOG` 读取日志级别，默认 `info`）。
  2. 从环境变量读取配置，连接 SQLite 数据库，运行迁移。
  3. 调用 `shirita_core` 的 ensure 函数保证默认模板、内置 definition、content node、asset hash 存在。
  4. 创建 assets 目录。
  5. 构建共享 HTTP 客户端和环境变量决定的 provider。
  6. 构建 `AppState`，注入 Arc 封装的 storage、provider、token_counter、generations。
  7. 读取 `BIND_ADDR`（默认 `127.0.0.1:8787`），如果不是 loopback 则记录警告。
  8. 绑定并启动 axum 服务器。
- **⚠️ 需要你关注的点**：
  - `BIND_ADDR` 默认 `127.0.0.1:8787`。非 loopback 绑定时只记录警告而不阻止——因为 embed 模式下 `/` 会把 bearer token 嵌入 HTML 中。这是设计决策，生产部署者应该用反向代理。
  - `is_loopback_bind` 对 `[::1]:8787` 正确处理（测试覆盖）。
  - 所有 `unwrap_or_else` 和 `unwrap_or` 都有 fallback 值，没有直接 panic 的 unwrap。

---

## `auth.rs` -- HTTP Basic Auth + Bearer token middleware

- **职责**：实现两层认证：(1) `require_bearer` 中间件保护 `/api/*` 路径；(2) `require_basic` 中间件作为最外层可选 HTTP Basic Auth 门禁，同时内建 Bearer token fallback 以便 JS 发出的 fetch 调用能正常通过。
- **输入/输出**：
  - `require_bearer(State<AppState>, Request, Next) -> Result<Response, StatusCode>`：逐个检查 `Authorization` 头中的所有值，寻找 `Bearer <token>`。匹配成功（常量时间比较）则通过，否则返回 401。
  - `require_basic(State<AppState>, Request, Next) -> Response`：如果 `config.http_auth_user`/`http_auth_pass` 未配置，直接放行。否则检查 `Authorization: Basic <base64>` 是否匹配，也检查是否有有效的 `Bearer` token。都不匹配则返回 401 + `WWW-Authenticate` 挑战头。
  - `constant_time_eq(a: &[u8], b: &[u8]) -> bool`：常量时间比较，防止侧信道攻击。
  - `basic_challenge() -> Response`：构造 401 响应，携带 `WWW-Authenticate: Basic realm="Shirita"` 头。
- **依赖关系**：依赖 `AppState`（读取 `config.token_secret`、`config.http_auth_user`、`config.http_auth_pass`）、`base64` crate。
- **实现方式**：
  - Bearer 中间件：迭代 `headers().get_all(AUTHORIZATION)`，因为浏览器在 Basic auth 下会自动附加 `Basic` 头，而 JS fetch 显式设置 `Bearer` 头——两者在 header 多值中共存。
  - Basic 中间件：先尝试 Basic 认证，失败后尝试 Bearer fallback。这解决了 SPA 中浏览器自动附加 Basic 头但 JS fetch 只发 Bearer 头的矛盾——否则每次 API 调用都会触发 Basic 挑战。
  - 所有密码/token 比较都用 `constant_time_eq`。
  - `require_basic` 返回值是 `Response` 而非 `Result`，因为即使认证失败也要返回带特定头的 401 响应（而非简单的 `Err(StatusCode::UNAUTHORIZED)`）。
- **⚠️ 需要你关注的点**：
  - `basic_challenge` 中 `HeaderValue::from_str(...).expect("static realm string is a valid header value")`：这个 expect 是安全的（realm 是静态字符串不含非法字符），但风格上不如 `unwrap()` 一致。
  - **安全风险**：Bearer fallback 的设计是「攻击者必须先通过 Basic Auth 才能拿到嵌入 token 的 HTML」。如果攻击者已经能看到 HTML（例如通过同一网络内的 HTTP 流量嗅探），那 token 已经泄露。这不算设计缺陷，但值得注意。
  - `get_all(AUTHORIZATION)` 在 require_bearer 中正确使用，但在 require_basic 的 bearer check 中也使用了 `get_all`——这是合理的，因为可能有多值。
  - Basic 认证的 `base64::decode` 使用 `trim()`：base64 不应包含空格，但 `trim()` 是无害的防御性编程。

---

## `state.rs` -- AppState shared state

- **职责**：定义全局共享状态 `AppState` 结构体，包含所有需要在线程间共享的组件。
- **输入/输出**：
  - `AppState`：
    - `storage: Arc<dyn Storage>` — SQLite 数据库操作接口
    - `config: Arc<Config>` — 配置
    - `provider: Arc<dyn ModelProvider>` — 当前 AI provider（Trait 对象）
    - `token_counter: Arc<dyn TokenCounter>` — token 计数
    - `model: String` — 默认模型名称
    - `generations: Arc<Generations>` — 进行中的 generation 注册表
    - `http_client: reqwest::Client` — 共享 HTTP 客户端（cloning 共享连接池）
  - 结构体派生 `Clone`（axum 要求 State 可 Clone）。
- **依赖关系**：依赖 `shirita_core`（`Config`、`ModelProvider`、`Storage`、`TokenCounter`）和 `crate::generations::Generations`。
- **实现方式**：纯数据结构，无业务逻辑。

---

## `embed.rs` -- Embedded UI serving (for single-binary deployments)

- **职责**：在 `embed-ui` feature 下，将构建好的 Vue 前端（`shirita-ui/dist`）通过 `rust-embed` 编译进二进制文件，并在运行时提供 `/`、`/static/*` 和 SPA fallback 路由。始终编译的辅助函数负责 token 注入。
- **输入/输出**：
  - `inject_runtime(html: &str, token: &str) -> String`：在 HTML 的 `</head>` 前插入 `<script>window.__SHIRITA_RUNTIME__={"base":"","token":"<token>"}</script>`。对 `</` 进行转义防止 token 中的 `</script>` 突破 script 标签。
  - `is_reserved_prefix(path: &str) -> bool`：判断路径是否为 `/api`、`/assets`、`/static`、`/health` 或其子路径。用于 SPA fallback 判断。
  - `serve_index(State<AppState>) -> Response`：`GET /`，读取嵌入的 `index.html`，注入 runtime token，返回 HTML。
  - `serve_static(Path<String>) -> Response`：`GET /static/{*path}`，读取嵌入的静态资源块，通过 `mime_guess` 确定 Content-Type，设置 1 年强缓存（`immutable`）。
  - `spa_fallback(Uri, State<AppState>) -> Response`：其他所有路径的 fallback——如果是保留前缀返回 404，否则返回注入 token 的 `index.html`（支持 Vue Router history mode 深度链接）。
- **依赖关系**：依赖 `rust_embed`、`axum`、`mime_guess`、`serde_json`、`uuid`（通过 `AppState`）。
- **实现方式**：
  - `#[derive(RustEmbed)]` 在编译时嵌入 `shirita-ui/dist` 目录。
  - `inject_runtime` 在字符串级别操作 HTML：查找 `</head>` 位置插入 script 标签。如果没有 `</head>`（畸形 HTML），则在最前面插入。
  - `spa_fallback` 先检查 `is_reserved_prefix`，防止 API 路径误返回 HTML。
  - `embed-ui` feature 开启时，`app()` 中将 SPA 路由注册在 `Router` 上，并设置 `fallback`；同时将 `static` 路径映射到 `serve_static`。
- **⚠️ 需要你关注的点**：
  - `inject_runtime` 在 token 中替换 `</` 为 `<\/`：这是针对 JSON 序列化后的字符串做的，而不是针对原始 token——实际上 JSON 序列化已经会转义特殊字符，额外的 `.replace("</", "<\\/")` 是防御性的。行为正确，但注释已说明原因。
  - `Mutex<HashSet<String>>` 用于 `summarizing` 在 `chat.rs` 中，但 `inject_runtime` 不需要——注意别混淆。
  - 当 `embed-ui` feature 关闭时，fallback 行为不同（`index.rs` 返回 `static/index.html`），但这不是 `embed.rs` 的问题，是 `lib.rs` 的条件编译。

---

## `generations.rs` -- In-flight generation tracking with CancellationToken

- **职责**：按 session 追踪正在进行的 AI 生成流。新的 generation 自动替换（abort）同一 session 的旧 generation，防止竞态。支持硬中止（`AbortHandle`）和协作停止（`StopHandle`）。
- **输入/输出**：
  - `Generations` 结构体：`Mutex<HashMap<String, (u64, AbortHandle, StopHandle)>>`，key 为 session_id，value 包含单调递增的 generation id + 两种中止机制。
  - `Generations::new() -> Self`：创建空注册表。
  - `replace(&self, session_id, handle, stop) -> u64`：注册新 generation，返回 gen_id。如果该 session 已有旧 generation，硬中止它。
  - `finish(&self, session_id, gen_id)`：取消注册——但只有提供的 gen_id 与当前存储的一致时才取消（防止老的 generation 结束时机错误地清除了新的 generation）。
  - `remove(&self, session_id)`：删除 session 对应的 generation（用于 session 被删除时），硬中止 + 停止。
  - `stop(&self, session_id) -> bool`：协作停止 generation（保留已生成的部分）。返回 false 表示该 session 没有进行中的 generation。
  - `NEXT_GEN: AtomicU64`：全局单调递增 generation id 生成器。
- **依赖关系**：依赖 `futures::stream::AbortHandle`、`shirita_core::StopHandle`、`std::sync::Mutex`。
- **实现方式**：
  - `replace` 用 `Mutex.lock().unwrap()` 获取锁，insert 新值，如果有旧值则调用 `old_handle.abort()` 和 `old_stop.stop()`。
  - `finish` 先检查 gen_id 是否仍匹配，防止 "stale finish" 问题。
  - `stop` 只调用 `stop.stop()` 不取消注册——由流自身在结束时调用 `finish`。
  - 所有方法在 `Mutex::lock()` 时 `unwrap()`：如果锁 poisoned 直接 panic，这在正常操作中不应该发生。
- **⚠️ 需要你关注的点**：
  - `Mutex::lock().unwrap()` 在多个方法中使用：如果持有锁的线程 panic，整个 Generations 将无法使用。但在 tokio 环境中，在 async 代码中使用 `std::sync::Mutex` 是合适的（short critical section），`tokio::sync::Mutex` 反而会增加开销。
  - 如果 `finish` 被同一个 generation 多次调用，第二次将不生效（gen_id 不匹配或 key 已被移除），这是安全的。
  - `stop` 不删除注册项，所以多次调用 `stop` 只会第一次有效（后续 `stop()` 发现 `StopHandle` 已被触发，再次调用是 no-op）。但 `Generations` 的 `stop` 只获取 `StopHandle` 然后调用 `stop()`，不检查是否已停止过。
  - 测试覆盖率很好（5 个测试用例覆盖 replace/finish/remove/stop 的核心行为）。

---

## `provider_select.rs` -- Provider selection from environment

- **职责**：provider 选择逻辑——根据环境变量和设置选择 AI provider（Anthropic、Ollama、OpenAI、Echo 回退）。同时包含模型列表 API 的请求构建和响应规范化。
- **输入/输出**：
  - `ProviderKind` enum：`Anthropic`、`Ollama`、`OpenAi`、`Echo`。
  - `provider_kind(provider_env: &str, api_key_empty: bool) -> ProviderKind`：纯函数，根据 `PROVIDER` 环境变量和 API key 是否为空决定 provider 类型。
  - `default_base_url(source: &str) -> &'static str`：12 种来源的默认 Base URL。
  - `build_provider(client, source, base_url, api_key) -> Arc<dyn ModelProvider>`：构建具体 provider 实例。
  - `provider_from_env(config: &Config, client: reqwest::Client)`：启动时从环境变量构建 fallback provider。
  - `ModelsRequest { url, headers }`：描述模型列表请求。
  - `models_request(source, base_url, api_key) -> ModelsRequest`：构建模型列表请求的参数（不同 vendor 认证方式不同）。
  - `normalize_models_response(source, raw_json) -> Value`：将各 vendor 的模型列表响应统一为 OpenAI 风格的 `{"data": [{"id": ...}]}` 格式。
  - `resolve_provider_config(storage) -> (source, base_url, api_key, model)`：从设置中解析活跃 provider 配置，并迁移遗留的扁平 key。
  - `resolve_provider(state) -> (Arc<dyn ModelProvider>, String)`：运行时解析——设置配置优先，否则用环境的 fallback。
- **依赖关系**：依赖 `shirita_core`（`AnthropicProvider`、`OpenAiProvider`、`EchoProvider`、`Config`）、`reqwest`、`serde_json`。
- **实现方式**：
  - `provider_kind` 是纯决策函数，方便单元测试。
  - `models_request` 按 source 构建不同认证方案：Anthropic 用 `x-api-key` 头、Google 用查询参数 key、其他用 Bearer 头。Ollama 不需要认证。
  - `normalize_models_response` 处理 Google（`models[].name` 只需取 `rsplit('/')` 最后一段）和 Cohere 的非标准响应格式。
  - `resolve_provider_config` 中的 key 迁移：如果扁平的 `provider_*` key 存在但新命名空间 key 不存在，则复制过去。这是向后兼容代码。
  - `resolve_provider` 检查是否有任何 provider 设置被明确配置，如果没有则返回环境的 fallback provider。
- **⚠️ 需要你关注的点**：
  - `normalize_models_response` 的 Google 分支：`n.rsplit('/').next().unwrap_or(n)`——如果 `name` 字段不包含 `/`（实际上 Google 的模型名总是 `models/xxx` 格式），`unwrap_or(n)` 是安全的。
  - `setting_str_storage` 中的多级 `.ok()`、`.flatten()`、`.and_then()` 链：如果一个环节失败，返回 `None`，调用方用 `unwrap_or_default()` 提供 fallback。错误被静默吞噬。
  - `resolve_provider_config` 中的 migration 循环使用 `let _ = storage.set_setting(...).await` 忽略写失败——写失败不太可能发生，但如果发生，下次读取时会重试 migration。
  - 测试覆盖率好（8 个测试），覆盖了 provider_kind、base URL、models_request、normalize_models_response。

---

## `routes/mod.rs` -- Route module declarations

- **职责**：声明所有路由子模块。
- **内容**：`pub mod` 声明 19 个模块。无业务逻辑。

---

## `routes/health.rs` -- Health check endpoint

- **职责**：简单健康检查。
- **API**：`GET /health` -> `{"status": "ok"}`
- **输入/输出**：无入参，返回 `Json<Value>`。
- **依赖关系**：无。

---

## `routes/ping.rs` -- Ping endpoint

- **职责**：认证后的 ping 接口。
- **API**：`GET /api/ping` -> `{"pong": true}`
- **输入/输出**：无入参，返回 `Json<Value>`。

---

## `routes/index.rs` -- Index page (non-embed mode)

- **职责**：当 embed-ui feature 未启用时，`GET /` 返回嵌入的 `static/index.html`。
- **API**：`GET /` -> `Html<&'static str>`
- **输入/输出**：无入参。
- **实现方式**：`include_str!("../../static/index.html")` 在编译时将 HTML 嵌入二进制。

---

## `routes/sessions.rs` -- Session CRUD + identity + packs + panels + import/export

- **职责**：会话（Session）的增删改查、复制、导出、导入；会话角色身份解析；挂载的 packs/definitions 管理；面板渲染。
- **API 表面**：
  - `GET /api/sessions` -> `Json<Vec<Session>>`：列出所有 session。
  - `POST /api/sessions` -> `Json<Session>`：创建 session。请求体 `{name, template_id?, avatar?, pack_ids?}`。
  - `POST /api/sessions/import` -> `Json<Session>`：从导出的 JSON 导入 session。
  - `PUT /api/sessions/reorder`：重排序 session 列表。
  - `GET /api/sessions/{id}` -> `Json<Session>`：获取单个 session。
  - `PATCH /api/sessions/{id}` -> `Json<Session>`：更新 name/avatar。
  - `DELETE /api/sessions/{id}` -> 204：删除 session（同时 abort 正在进行的 generation）。
  - `POST /api/sessions/{id}/duplicate` -> `Json<Session>`：复制 session（含消息树）。
  - `GET /api/sessions/{id}/export` -> `Json<Value>`：导出 session + 消息。
  - `GET /api/sessions/{id}/identity` -> `Json<Identity>`：解析 session 的角色身份。
  - `GET /api/sessions/{id}/messages` -> `Json<Vec<Message>>`：列出消息（含 regex 显示处理）。
  - `POST /api/sessions/{id}/messages` -> SSE：发送消息（转交给 `chat.rs`）。
  - `PUT /api/sessions/{id}/packs`：设置 session 的挂载 pack 列表。
  - `GET /api/sessions/{id}/packs` -> `Json<Vec<String>>`：获取挂载 pack ID 列表。
  - `GET /api/sessions/{id}/panels` -> `Json<Vec<RenderedPanel>>`：渲染好的面板列表。
  - `PUT /api/sessions/{id}/mounts`：设置挂载 definition 列表。
- **关键辅助函数**：
  - `clone_messages`：用新 ID 和 remapped parent 链接复制一组消息。
  - `seed_first_message`：创建 session 时从模板/pack 中查找 `first_message` type 的 definition，生成 anchor `<start>` 用户消息 + 助手的问候语（含 alternate greetings）。
- **依赖关系**：依赖 `shirita_core`（`Session`、`Message`、`PromptNode`、`PackIdentity`、`conversation::effective_nodes`、`conversation::resolve_session_schema`、`state::schema_initials`、`identity::resolve_identity_with_packs`、`panels::resolve_session_panels`、`apply_regex_rules_for`、`render_vars`）。
- **⚠️ 需要你关注的点**：
  - `session.avatar.as_deref().unwrap_or("").is_empty()` 这种链式调用可能被简化（`filter` + `is_none` 或直接 `map` + `unwrap_or_default`）。但不影响正确性。
  - `list_messages` 中 `m.display_content.clone().unwrap_or_else(|| m.raw_content.clone())`：`display_content` 是 `Option<String>`，优先用 `display_content`，回退到 `raw_content`。
  - `list_messages` 逐条应用 regex rules——如果 rules 很多，这可能是 O(n*m) 的开销。但注释提到跳过了 HTML document，这避免了误改写卡片标记。没有 batch 优化。
  - `get_session_identity` 函数长度较大（约 80 行），涉及多步数据组装（获取 session -> 获取 effective nodes -> 加载 pack nodes -> 加载 definitions -> 确定 pack 绑定角色 -> 调用 `resolve_identity_with_packs`）。复杂度合理但可读性一般。
  - `import_session` 和 `duplicate_session` 中的 message clone 逻辑重复——两者都用 `clone_messages` 辅助函数处理核心逻辑，但周围的管理代码（重设 active_leaf 等）有相似模式。
  - `set_packs` 和 `set_mounts` 中两次 `map_err` 转换处理相同模式：先检查 session 是否存在，再执行更新。可以考虑提取为通用 ensure_session 辅助（`local_overrides.rs` 已有这样的辅助函数但当前文件中未使用——在 `local_overrides.rs` 中定义了一个 `ensure_session`）。

---

## `routes/chat.rs` -- Send message + regenerate + abort (SSE streaming)

- **职责**：AI 消息生成核心接口。发送消息、重新生成、中止，全部通过 Server-Sent Events (SSE) 流式返回。
- **API 表面**：
  - `POST /api/sessions/{id}/messages` -> `Sse<...>`：发送用户消息，返回 AI 回复的 SSE 流。请求体 `{text, attachments?}`。
  - `POST /api/sessions/{id}/messages/{msg_id}/regenerate` -> `Sse<...>`：重新生成指定消息，返回 SSE 流。
  - `POST /api/sessions/{id}/abort` -> `Json<Value>`：协作停止 session 的进行中 generation。
- **关键辅助函数**：
  - `summarizing()`：`OnceLock<Mutex<HashSet<String>>>`——进程级集合，防止同一 session 同时触发多个自动摘要。
  - `try_claim(session_id) -> bool`：尝试获得摘要运行权。失败返回 false。
  - `release(session_id)`：释放摘要运行权。
  - `spawn_summary(state, session_id)`：在后台 tokio 任务中运行摘要，使用 `resolve_provider` 获取从设置解析的 provider/model。
- **实现方式**：
  - `send` 和 `regenerate_message` 结构相似：解析 provider -> 创建 `StopHandle` -> 调用 `send_message` / `regenerate` -> 包装 `abortable` stream -> 在 `state.generations` 注册 -> 返回 SSE。
  - SSE 流在每个事件上检查 `Done`、`Stopped`、`Error`，调用 `generations.finish()` 并在 `Done` 时触发后台摘要。
  - `abort_message` 只调用 `generations.stop()`——协作停止，保留已生成文本。
- **依赖关系**：依赖 `shirita_core`（`send_message`、`regenerate`、`summarize::run`、`SendEvent`、`StopHandle`）、`crate::provider_select::resolve_provider`。
- **⚠️ 需要你关注的点**：
  - `summarizing()` 使用 `std::sync::Mutex` 而不是 `tokio::sync::Mutex`：因为临界区极短（查找 + 插入/删除），这是合理的选择。
  - `try_claim` 和 `release` 中的 `Mutex::lock().unwrap()`：如果锁 poisoned 会 panic。
  - SS 事件处理中存在代码重复：`send` 和 `regenerate_message` 的 SSE map 回调几乎完全一样。可以提取为一个共享函数。
  - `send` 和 `regenerate_message` 中的 summary spawn 模式：`Done` 时触发，摘要运行在后台线程，不阻塞 SSE 流。如果摘要失败（`summarize::run` 返回错误），错误被静默吞噬（无日志——确认 `summarize::run` 内部是否记录日志）。
  - `abort_message` 返回 `{"stopped": bool}` 而不是 `StatusCode`——即使不存在进行中的 generation 也是 200 成功。这是设计决策（幂等）。
  - SSE 使用 `Infallible` 作为 error 类型，意味着流闭包中 `Err` 情况不会导致 SSE 连接关闭。
  - 三个终止条件（Done/Stopped/Error）都调用 `generations.finish()`——确保不会泄漏 generation 槽位。

---

## `routes/messages.rs` -- Message edit, delete, set active leaf, fork session

- **职责**：会话消息的编辑、删除（含子树）、切换活跃分支、分支会话（fork）。
- **API 表面**：
  - `PUT /api/sessions/{id}/messages/{msg_id}` -> `Json<Message>`：编辑消息内容/隐藏状态。请求体 `{content?, is_hidden?}`。
  - `DELETE /api/sessions/{id}/messages/{msg_id}` -> `Json<Value>`：删除消息及其全部子消息。返回 `{"activeLeafId": new_leaf}`。
  - `PUT /api/sessions/{id}/active-leaf` -> `Json<Session>`：将活跃分支切换到指定消息的最深叶子。请求体 `{message_id}`。
  - `POST /api/sessions/{id}/fork` -> `Json<Session>`：从指定消息分支出去创建一个新 session。请求体 `{message_id}`。
- **实现方式**：
  - `edit_message`：验证消息属于该 session，更新 `raw_content`（重置 `display_content` 让读取时重新应用 regex），可选更新 `is_hidden`。
  - `delete_message`：验证归属、调用 `storage.delete_message_subtree`（返回新的 active_leaf_id）。
  - `set_active_leaf`：找到消息到其最深叶的路径，设置为 active leaf。
  - `fork_session`：复制 session（含模板引用、覆盖配置、挂载定义）-> 从根到指定消息的线性路径中的消息 -> 分配新 ID 并 remap parent -> 创建新消息 -> 复制 digest summaries（仅 remap cutoff_message_id 存在于 idmap 中的部分）。
- **⚠️ 需要你关注的点**：
  - `fork_session` 中 `let node = slice.last().unwrap()`：如果 `slice` 为空已在前面返回 404，所以 `unwrap` 是安全的。但使用 `if slice.is_empty()` 检查后跟 `unwrap` 的模式略显脆弱——如果未来有人删除了检查，这里会 panic。可用 `if let Some(node) = slice.last()` 重构。
  - `delete_message` 只返回 `activeLeafId` 而不返回完整 session，可以减少响应体积但客户端需要额外跟踪。
  - `fork_session` 中 `copy_nodes` 的 `let _ =` 忽略错误：如果节点复制失败，fork 会继续（节点缺失但消息已复制）。这可能造成不一致状态。但 `copy_nodes` 是 session 级别的（`OwnerKind::Session`），大多数 session 没有自己的节点树（只有引用模板的节点），所以失败通常无害。
  - `fork_session` 的 digest summary 复制只复制 `cutoff_message_id` 在 idmap 中的 summaries——正确。但如果存在大量 summaries，遍历是无害的（不会全量加载内存，`list_summaries` 是一个查询）。

---

## `routes/definitions.rs` -- Definition CRUD

- **职责**：Definition（提示模板节点引用的定义）的增删改查，支持按 type 过滤，创建时验证 regex_rule 合法性。
- **API 表面**：
  - `GET /api/definitions?type=xxx` -> `Json<Vec<Definition>>`：列出所有 definition，可选按 type 过滤。
  - `POST /api/definitions` -> `Json<Definition>`：创建 definition。请求体 `{type, name, content, meta?}`。
  - `GET /api/definitions/{id}` -> `Json<Definition>`：获取单个 definition。
  - `PUT /api/definitions/{id}` -> `Json<Definition>`：更新 definition。
  - `DELETE /api/definitions/{id}` -> 204：删除 definition。
- **实现方式**：
  - `validate_type`：检查 type 是否为保留类型或已注册的容器类型。如果是保留类型（如 `char`、`persona`、`regex_rule` 等）通过；如果是自定义容器类型，必须在 `def_types` 表中注册。
  - `validate_regex_rule`：如果 type 是 `regex_rule`，尝试验证 `meta.pattern` 的正则合法性。
  - `build` 辅助函数：从请求体构建 `Definition` 对象，处理 `meta` 的 null → `{}` 转换。
  - `list` 用 `Query` 解析 `type` 参数，SQL 层面过滤（`list_definitions_by_type`），避免全表加载。
- **⚠️ 需要你关注的点**：
  - `validate_type` 中的 `list_container_types` 查询在每次 create/update 时都执行。如果频繁操作，可以缓存容器类型列表。但目前规模不大，不是问题。
  - `update` 中只调用了 `validate_type` 和 `validate_regex_rule`——没有验证 definition 实际上是否存在（`storage.update_definition` 返回 `bool` 表是否更新了行）。如果 type 被修改为不存在的 type，不会报错，但写入的 definition 可能引用了一个不存在的 type。
  - `delete` 不检查 definition 是否被任何 node 引用（Foreign Key 约束应该会处理——`definition_id` 在 prompt_nodes 表中有 `ON DELETE SET NULL`）。这是数据库层面的保证，Rust 代码没有额外检查。

---

## `routes/export.rs` -- Export definitions/templates/packs

- **职责**：定义、模板、pack 的导出接口。Pack 导出支持 zip 打包（含资产文件）。
- **API 表面**：
  - `GET /api/definitions/{id}/export` -> 带 `Content-Disposition: attachment` 的 JSON 下载。
  - `GET /api/templates/{id}/export` -> 同上，但输出用 `shirita_core::export_template` 封装。
  - `GET /api/packs/{id}/export` -> JSON 或 ZIP（取决于是否有二进制资产）。
- **实现方式**：
  - `safe_filename`：仅保留字母数字、`-`、`_`，其他字符替换为 `_`，防止路径遍历或文件名注入。
  - `export_pack`：读取所有被引用的资产文件，如果不存在则跳过（记录 warning）。如果没有资产，返回 `.json` 文件；如果有，使用 `zip` crate 构建内存 zip（`manifest.json` + `assets/*`），返回 `.zip` 文件。
- **⚠️ 需要你关注的点**：
  - `export_pack` 中的 `unwrap_or_else(|_| tracing::warn!(...))`：资产文件缺失时跳过，而不是报错。如果用户删除了文件，导出的 zip 中将缺失这些资产，导入时 `rewrite_pack_assets` 会清空它们的引用。这是可接受的行为，但用户可能不知道导出不完整。
  - `zip::ZipWriter` 的创建和写入：所有 `.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)` 转换——zip 写入是纯内存操作（写入 `Cursor<Vec<u8>>`），一般不会失败，但防御性处理是正确的。
  - `export_pack` 和 `export_template`/`export_definition` 的返回类型不同：前两个返回 `impl IntoResponse`（允许元组 + Json），`export_pack` 返回显式的 `Response`，因为需要根据是否有资产动态决定响应类型。

---

## `routes/import_export.rs` -- Native portable import/export

- **职责**：导入和导出 Shirita 原生便携格式——`shirita.definition` / `shirita.template` / `shirita.pack`（JSON 或 ZIP bundle）。仅接受这三种显式格式，其余一律 `400`。
- **API 表面**：
  - `POST /api/import`（multipart）-> `Json<ImportSummary>`：接受一个 `file` 字段。ZIP 签名 -> `shirita.pack` bundle；否则按 JSON 解析并要求显式 `format` 判别值。
- **关键结构体和函数**：
  - `ImportQuery`、`ImportSummary`、`ImportItem`：导入结果摘要。
  - `OnConflict` enum：`Skip`、`Overwrite`、`Duplicate`——冲突处理策略。
  - `unzip_pack(bytes) -> (Value, HashMap<String, Vec<u8>>)`：解包 zip，验证安全（路径遍历保护、entry 数量/大小限制、总大小限制、必须包含 `manifest.json`）。
  - `persist_pack_bundle`：持久化 pack bundle——hash-dedup 资产、重写引用、创建 pack + defs + nodes。
  - `persist_defs`：按 `on_conflict` 策略持久化 definition 列表。
  - `import_template_bundle`：导入 shirita.template bundle——topological node 排序后事务性写入。
  - `first_field`：从 multipart 中读取第一个字段的字节。
- **实现方式**：
  - `import` 函数按固定顺序分发：ZIP 签名 -> `shirita.pack` bundle（要求有效 manifest）；否则 JSON -> 仅接受 `format` 为 `shirita.definition` / `shirita.template` / `shirita.pack` 的文档；其余（外来卡片/预设/World Info JSON、任意 PNG 字节、未知 JSON）返回 `400`，不写入任何数据库行或资产文件。
  - `import_template_bundle` 和 `persist_pack_bundle` 都实现了拓扑排序 node 插入逻辑（大括号内 `remaining`/`progressed` 循环），确保父节点在子节点之前创建。
  - pack 导入时 hash-dedup 资产：同内容的资产（同 sha256）复用已有或同一批中刚创建的 Asset 行。
  - 导入时检查 `on_conflict`：pack/template 按名称检查冲突，definition 按 name+def_type 检查冲突。
- **⚠️ 需要你关注的点**：
  - **代码重复**：`import_template_bundle` 和 `persist_pack_bundle` 中的 topological node 排序逻辑几乎完全相同（变量名略有差异：一个用 `OwnerKind::Pack`，另一个用 `OwnerKind::Template`）。这是一个明显的抽象缺失——可以提取为共享函数 `topo_sort_nodes(owner_kind, owner_id, nodes, def_map, parent_local_ids) -> Vec<PromptNode>`。
  - `unzip_pack` 中的安全性不错：`MAX_ZIP_ENTRIES=512`、`MAX_ENTRY_BYTES=32MiB`、`MAX_TOTAL_BYTES=64MiB`，使用 `enclosed_name()` 防止路径遍历，拒绝嵌套 `assets/` 目录。但读取时先用 `entry.size()` 检查再用 `take(MAX_ENTRY_BYTES + 1).read_to_end` 进一步防护——double-check 模式。

---

## `routes/settings.rs` -- Settings get/set

- **职责**：全局设置的读取和写入（键值对）。
- **API 表面**：
  - `GET /api/settings` -> `Json<Value>`（`{key: value, ...}` 对象格式）。
  - `PUT /api/settings`：批量写入设置。请求体 `{key: value, ...}`。
- **实现方式**：
  - `get_all`：`list_settings` 返回 `Vec<(String, Value)>`，转为 `serde_json::Map` 然后 `Value::Object`。
  - `update_all`：体必须是 JSON 对象（否则 400），所有 key-value 对在一个事务中写入（`set_settings`）。
- **⚠️ 需要你关注的点**：
  - `update_all` 中 `body.as_object().ok_or(StatusCode::BAD_REQUEST)?`：如果 body 不是对象直接拒绝，防止恶意请求。
  - 没有部分写入——所有 key 要么全部写入要么全部失败（事务性）。

---

## `routes/packs.rs` -- Pack CRUD + duplicate + orphan definitions + delete with orphan GC

- **职责**：Pack 的完整 CRUD、复制、查看孤立 definition、删除时清理孤立 avatar。
- **API 表面**：
  - `GET /api/packs` -> `Json<Vec<Pack>>`
  - `POST /api/packs` -> `Json<Pack>`：请求体 `{name, identity?, meta?}`。
  - `GET /api/packs/{id}` -> `Json<Pack>`
  - `PUT /api/packs/{id}` -> `Json<Pack>`：更新 pack（含 avatar GC if changed）。
  - `DELETE /api/packs/{id}?delete_orphans=false` -> 204：删除 pack，可选删除孤立 definition。删除后清理引用计数为 0 的 avatar。
  - `POST /api/packs/{id}/duplicate` -> `Json<Pack>`：复制 pack（含节点树）。
  - `GET /api/packs/{id}/orphan-definitions` -> `Json<Vec<Definition>>`：列出仅被此 pack 引用的 definition。
- **⚠️ 需要你关注的点**：
  - `update` 中的 avatar GC：如果更新改变了 avatar，旧的 avatar 如果没有任何引用（其他 pack/definition/session）则被删除。这是靠 `crate::routes::assets::gc_avatar_if_orphaned` 实现的。
  - `delete` 中先读取 pack 获取 avatar，删除后清理 orphaned avatar——如果删除失败，avatar 读取的 `ok_or` 链会正确返回 500。
  - `duplicate` 中 `state.storage.create_pack` 后调用 `copy_nodes`——如果节点复制失败（`map_err` 返回 500），但 pack 已经创建。失败可能导致 pack 存在但没有节点。但 `copy_nodes` 的 `map_err` 会将错误传播为 500。这是合理的。
  - `orphan_definitions` 在删除确认对话框中使用。

---

## `routes/templates.rs` -- Template CRUD + duplicate + orphan definitions

- **职责**：Template 的 CRUD、复制、查看孤立 definition。创建时自动添加 `content` 和 `history` 魔法节点。
- **API 表面**：
  - `GET /api/templates` -> `Json<Vec<Template>>`
  - `POST /api/templates` -> `Json<Template>`：请求体 `{name, meta?}`。
  - `GET /api/templates/{id}` -> `Json<Template>`
  - `PUT /api/templates/{id}` -> `Json<Template>`：更新 name/meta。
  - `DELETE /api/templates/{id}?delete_orphans=false` -> 204：删除模板。
  - `POST /api/templates/{id}/duplicate` -> `Json<Template>`：复制模板（含节点树）。
  - `GET /api/templates/{id}/orphan-definitions` -> `Json<Vec<Definition>>`
- **实现方式**：
  - `create` 调用 `storage.create_template_with_nodes` 在一个事务中创建模板 + content node + history node。
  - `create` 中 content node 的 `kind = NodeKind::Content`，history node 的 `kind = NodeKind::History`——两者都是根级别 folder，有特定 sort_order（0 和 1）。
- **⚠️ 需要你关注的点**：
  - `duplicate` 创建新模板后复制节点树——与 pack 的 replicate 模式相同。
  - `delete` 接受 `delete_orphans` 参数：如果 true，删除任何仅被此模板引用的 definition。
  - `create` 没有验证 name 唯一性——调用者需要在前端或上层处理。

---

## `routes/prompt_nodes.rs` -- Prompt node CRUD + reorder (template/pack node tree management)

- **职责**：模板和 pack 内的 prompt node 树管理（文件夹/引用节点的增删改、排序）。强制 2 层树结构约束。
- **API 表面**：
  - `GET /api/{owner_kind}/{owner_id}/nodes?owner_kind=xxx` -> `Json<Vec<PromptNode>>`：列出节点的树（flat 列表，前端构建树）。
  - `POST /api/{owner_kind}/{owner_id}/nodes?owner_kind=xxx` -> `Json<PromptNode>`：创建节点。
  - `PUT /api/nodes/{id}` -> `Json<PromptNode>`：更新节点（移动、重命名、开关等）。
  - `DELETE /api/nodes/{id}` -> 204：删除节点。
  - `PUT /api/{owner_kind}/{owner_id}/nodes/reorder?owner_kind=xxx`：重排序节点。
- **实现方式**：
  - `enforce_two_level`：验证 tree 约束——`Folder`/`History`/`Content` 必须在根；`Ref` 必须在根或文件夹下。违反返回 400.
  - `create_node`：计算 `next_order`（同级最大 sort_order + 1），验证 `History`/`Content` 不可手动创建，`Ref` 必须提供 `definition_id`。
  - `update_node` 使用 `double_option` 反序列化技巧：区分「未提供字段」和「提供为 null」，允许显式清空 `parent_id`、`tag`、`definition_id`。
- **⚠️ 需要你关注的点**：
  - `double_option` 反序列化宏：这是一个巧妙的设计，允许 `PATCH` 语义正确地清除字段。但会增加反序列化复杂性。
  - `create_node` 中 `body.tag.unwrap_or_else(|| "unnamed".into())`：为 Folder 提供默认 tag。
  - `reorder_nodes` 需要 `owner_kind` 查询参数（`?owner_kind=pack` 或 `?owner_kind=template`），路径参数是 `owner_id`。
  - `list_nodes` 接受 `owner_kind` 作为查询参数——这在路由注册时通过 `{owner_kind}` 无法匹配，因为节点列表路由挂载在两次不同的路径下（`/templates/{id}/nodes` 和 `/packs/{id}/nodes`），所以需要显式查询参数区分。但路由路径是固定的，不会包含 `{owner_kind}` 占位符——这是在 `lib.rs` 中硬编码的（`.route("/templates/{id}/nodes", ...)` 和 `.route("/packs/{id}/nodes", ...)`），`{owner_id}` 参数是通过 `Path(owner_id)` 提取的，`owner_kind` 通过查询参数传递。这有点冗余——理论上可以从路径前缀推导，但当前设计是显式的。

---

## `routes/provider.rs` -- Provider test connection + list models

- **职责**：AI provider 的连接测试和模型列表查询。
- **API 表面**：
  - `POST /api/provider/test` -> `Json<Value>`：发送 "ping" 到 provider，返回 `{"ok": true/false}` 或 `{"error": "..."}`。
  - `GET /api/provider/models` -> `Json<Value>`：从 provider 获取可用模型列表，返回前规范化。
- **实现方式**：
  - `test_connection`：使用 `resolve_provider_config` + `build_provider` 构建 provider，发送一个 `{"role": "user", "content": "ping"}` 请求（`max_tokens=16`），检查能否收到第一个流式 chunk。
  - `list_models`：使用 `models_request` 构建 vendor 特定的 API 请求，设置 30 秒超时，发送 GET 请求后用 `normalize_models_response` 规范化。
- **⚠️ 需要你关注的点**：
  - `test_connection` 中 `model` 为空时使用 `"gpt-4o"` 作为默认——这假设了 OpenAI 兼容格式。如果 provider 是 Anthropic 或其他，这可能返回错误，但错误会被捕获并以 `{"ok": false, "error": "..."}` 形式返回。
  - `list_models` 中的 `resp.text().await.unwrap_or_default()`：如果读取响应 body 失败，使用空字符串。正常情况不会发生，但防御是好的。
  - `list_models` 所有分支都返回 200——即使 provider 返回了 HTTP 错误或 body 解析失败，也以 JSON 错误信息形式返回而非 HTTP 错误码。这对前端友好。
  - `test_connection` 也是全 200 返回。
  - 两个 handler 都使用 `resolve_provider_config` 而非 `resolve_provider`——这意味着它们不使用环境 fallback provider，只读取设置。这与 `chat.rs` 中的 `resolve_provider` 行为一致（设置优先）。

---

## `routes/regex_rules.rs` -- List regex scopes

- **职责**：为设置 UI 提供所有 regex_rule definition 的作用域、引用模板和模式有效性信息。
- **API 表面**：
  - `GET /api/regex-rules/scopes` -> `Json<Vec<RegexScope>>`：每个 regex_rule 的作用域（`global`/`template`）、引用该规则的模板名称列表、模式编译错误信息。
- **实现方式**：
  - 查询所有 definition，过滤 `def_type == "regex_rule"`。
  - 调用 `storage.template_definition_refs()` 一次获取所有 (template_name, def_id) 对，在内存中建立映射（替代逐模板 N+1 查询）。
  - 通过 `shirita_core::regex_error(pattern)` 检查正则合法性。
- **⚠️ 需要你关注的点**：
  - `template_definition_refs` 是一次 JOIN 查询，比之前的 N+1 更好。
  - `template_names` 去重（`if !names.contains(&template_name)` 检查）。

---

## `routes/local_overrides.rs` -- Session-local definition overrides + materialize

- **职责**：会话级别的 definition 本地覆盖（copy-on-write 模式）、节点物化（从模板/pack 复制节点树到 session）、本地覆盖 promotion 到全局。
- **API 表面**：
  - `PUT /api/sessions/{id}/local-definitions/{def_id}` + JSON patch：设置 definition 的本地覆盖。
  - `DELETE /api/sessions/{id}/local-definitions/{def_id}`：清除本地覆盖。
  - `POST /api/sessions/{id}/local-definitions/{def_id}/promote`：将本地覆盖合并到全局 definition 并清除覆盖。
  - `POST /api/sessions/{id}/materialize-nodes`：将模板的节点树复制到 session（copy-on-write 前准备）。
  - `POST /api/sessions/{id}/materialize-pack`：将 pack 的节点树复制到 session。
- **实现方式**：
  - `set_local_definition` / `clear_local_definition`：直接委托到 `storage.set_local_definition` / `storage.clear_local_definition`，存储到 `override_config`。
  - `materialize_nodes`：如果 session 没有自己的节点树（第一次编辑），调用 `storage.materialize_session_nodes` 事务性复制模板的节点树到 session。
  - `materialize_pack_nodes`：类似，但专门针对 pack 的节点树。
  - `promote_local_definition`：从 `override_config.local_definitions` 读取 patch，应用到 global definition，然后调用 `storage.promote_local_definition` 在单事务中完成合并和清理。
- **⚠️ 需要你关注的点**：
  - `ensure_session` 辅助函数在文件顶部定义——与 `sessions.rs` 中重复的 session 存在性检查模式相同，但这里提取为单独函数。其实 `sessions.rs` 和 `local_overrides.rs` 可以共享这个辅助。
  - `promote_local_definition` 中 `def.meta.as_object_mut().unwrap()`：在 `!def.meta.is_object()` 时已重置为 `json!({})`，所以 `unwrap` 是安全的。但用 `unwrap_or_else` 创建空对象替代会更健壮。
  - `promote_local_definition` 只合并 `content`、`name`、`trigger`、`scan` 字段——其他 `meta` 字段不会被合并。如果未来添加了新字段，需要更新此函数。

---

## `routes/types.rs` -- Container type management

- **职责**：自定义 container type（def_type）的增删。用于扩展 `char`、`persona`、`regex_rule`、`first_message` 等保留类型之外的自定义类型。
- **API 表面**：
  - `GET /api/types` -> `Json<Vec<DefType>>`：列出所有容器类型。
  - `POST /api/types` -> `Json<DefType>`：创建新的容器类型。请求体 `{id, label, sort?}`。
  - `DELETE /api/types/{id}` -> 204：删除容器类型。内建类型不可删除；有 definition 使用的类型不可删除（返回 409 Conflict）。
- **实现方式**：
  - `create` 验证 id 不为空且不与保留类型冲突。
  - `delete` 先检查是否是内建类型（400），再检查是否有 definition 正在使用（409）。
- **⚠️ 需要你关注的点**：
  - `is_reserved` 在 `shirita_core` 中定义——如果更新了保留类型列表，此处会自动同步。
  - 删除时的 `CONFLICT` 状态码是正确的 REST 实践。

---

## `routes/variables.rs` -- Session state + local variables + state updates (panel actions)

- **职责**：会话变量状态管理——获取当前变量状态（含schema）、设置局部变量、应用面板驱动的变量更新。
- **API 表面**：
  - `GET /api/sessions/{id}/state` -> `Json<{"schema": ..., "values": ...}>`：获取当前活跃分支的变量状态和 schema。
  - `PUT /api/sessions/{id}/local-variables`：替换 session 的局部变量声明。
  - `POST /api/sessions/{id}/state-updates` -> `Json<{"values": ...}>`：应用面板驱动的变量变更。
- **实现方式**：
  - `get_state`：解析 schema（模板 + 挂载的 pack + 局部变量），找到活跃路径的最后一个消息的 `snapshot_state`，调用 `effective_state` 合并。
  - `apply_state_updates`：接收 `{updates: [{action, key, value}]}`，解析 action 类型，调用 `apply_updates` 生成新的 snapshot，创建一个隐藏的系统消息节点存储新状态并将活跃叶子移到该节点（`create_message_and_advance_leaf`）。
  - `set_local_variables`：原子替换 `override_config.local_variables`。
- **⚠️ 需要你关注的点**：
  - `apply_state_updates` 中的 `Action::parse(&u.action)` 返回 `Option<Action>`——不认识的 action 被静默跳过。这是合理的设计。
  - `apply_state_updates` 和 `get_state` 中都有 `active_path(...).last()` 模式——可以提取为辅助函数。
  - `apply_state_updates` 创建的消息节点 `is_hidden = true`、`role = System`、内容为空——这是纯状态载体节点，不会在 UI 中显示。
  - `create_message_and_advance_leaf` 是事务性操作——如果失败，没有任何副作用。

---

## `routes/assets.rs` -- Asset/media library management

- **职责**：媒体库（avatar/background 图片）的增删改查。Ghost collection（孤立 avatar 的自动清理）。
- **API 表面**：
  - `GET /api/assets?kind=avatar|background` -> `Json<Vec<Value>>`：列出资产，按 kind 过滤。返回含 `url` 字段（`/assets/<path>`）。
  - `POST /api/assets?kind=avatar|background`（multipart）-> `Json<Value>`：上传资产。
  - `PUT /api/assets/{id}` -> OK：重命名资产。请求体 `{name}`。
  - `DELETE /api/assets/{id}` -> 204：删除资产（文件 + 数据库记录）。
- **辅助函数**：
  - `resolve_asset_url(relative) -> String`：相对路径转 URL（`/assets/<rel>`）。
  - `gc_avatar_if_orphaned(state, avatar_path)`：如果某 avatar 没有被任何 pack/definition/session 引用，删除它（文件 + 记录）。在 `packs.rs` 的 update/delete 中被调用。
  - `asset_json(a)`：Asset 转前端期望的 JSON 格式。
- **实现方式**：
  - `upload`：从 multipart 中读取第一个文件字段，用 UUID + 原扩展名存储到 `assets_dir`，计算 hash，创建 Asset 数据库记录。如果 DB 写入失败，删除已写入的文件防止孤立。
  - `delete`：先读取 asset 获取文件路径，尝试删除文件（best-effort），再删除数据库记录。
  - `gc_avatar_if_orphaned`：使用 `storage.is_avatar_referenced` 方法（一个查询跨三个表），只有无人引用才删除。
- **⚠️ 需要你关注的点**：
  - `delete` 中文件删除是 best-effort（`let _ = tokio::fs::remove_file`），记录删除才是关键。如果文件删除失败（权限等），数据库记录仍会被删除，文件变成孤立。但这不是安全问题。
  - `upload` 中 `multipart.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)?`：如果多部分请求格式错误返回 400。如果没有字段返回 400（`if let Some(field)` 块后的 `Err(StatusCode::BAD_REQUEST)`）。
  - `rename` 中 `body.name.trim()`：空格被修剪，但空字符串也会被修剪——`rename_asset` 可能会以空名称更新。但前端应该会处理 name 验证。
  - `norm_kind` 函数将未知 kind 默认为 `background`。
  - `resolve_asset_url` 有测试覆盖。

---

### 整体关注点总结

1. **代码重复**：`import_template_bundle` 和 `persist_pack_bundle` 中的拓扑 node 排序逻辑几乎完全相同（约 40 行），可以提取为共享函数。

2. **SSE 事件处理重复**：`chat.rs` 中 `send` 和 `regenerate_message` 的 SSE map 回调完全一样（约 12 行），可以提取共享函数。

3. **`unwrap` 风险**：
   - `chat.rs:summarizing().lock().unwrap()` — low risk，但 Mutex poisoned 会导致 panic。
   - `generations.rs` 中所有 `Mutex::lock().unwrap()` — 同理。
   - `fork_session` 中 `slice.last().unwrap()` — 有前面的空检查保护，但脆弱。
   - `promote_local_definition` 中 `def.meta.as_object_mut().unwrap()` — 有前面的 `is_object()` 检查保护。

4. **错误静默吞噬**：
   - `provider_select.rs` 的 key migration 中 `let _ = storage.set_setting(...)`。
   - `messages.rs` fork 中 `let _ = state.storage.copy_nodes(...)` 和 `let _ = state.storage.set_session_active_leaf(...)`。
   - 多个地方的 `let _ = tokio::fs::remove_file(...)`（best-effort 文件删除，可以理解）。

5. **Tauri/Web 差异**：`embed.rs` 中的 `is_reserved_prefix` 和 `assets.rs` 中的 `resolve_asset_url` 都有明确的条件编译或注释说明 Tauri 行为差异。但整体看，Tauri 版和 Web 版在 Rust 层没有大的分歧——大部分差异在前端。

6. **TODO/FIXME/HACK 注释**：文件中没有发现 TODO/FIXME/HACK 注释。注释质量高，详细解释了设计决策和边界情况。

7. **安全方面**：
   - `auth.rs` 使用常量时间比较，正确。
   - `import_export.rs` 的 `unzip_pack` 有适当的 Zip Slip 防护和大小限制。
   - `embed.rs` 的 `inject_runtime` 转义 `</` 防止 script 注入。
   - CORS 白名单受限（仅桌面 origin），跨域攻击面小。
   - Bearer token 嵌入 HTML —— 在非 loopback 绑定时是安全风险（已记录日志警告）。

8. **测试覆盖**：部分模块有测试（`auth.rs`、`generations.rs`、`provider_select.rs`、`chat.rs`、`embed.rs`、`assets.rs`、`main.rs`），但大部分 route 模块没有测试。不过 route handler 本身由 `StatusCode` 映射解耦，核心逻辑在 `shirita_core` 中测试。


---

# 三、shirita-tauri — 桌面应用入口


## main.rs

- **职责**：Tauri 桌面应用入口。启动嵌入式 Axum 服务器，初始化数据库，注入运行时配置到 WebView。
- **输入/输出**：
  - `boot(base: PathBuf) -> Result<(AppState, SqlitePool, u16, CancellationToken, String), String>`：初始化所有后端组件。
  - `data_paths(base: &Path) -> (PathBuf, PathBuf)`：数据库和资产目录路径。
  - `struct Shutdown { token: CancellationToken, pool: SqlitePool }`：优雅关闭。
- **依赖关系**：依赖 `shirita_core`（Config, SqliteStorage, TiktokenCounter）、`shirita_web`（app_with_cors, AppState, Generations）、`tauri`、`tauri_plugin_dialog`。
- **实现方式**：
  - boot() 创建数据目录、初始化 Config、连接 SQLite 并运行迁移，确保默认模板/定义/资产 hash 存在。
  - 构建 `AppState`，在 `127.0.0.1:0` 绑定随机端口启动 Axum 服务器（`app_with_cors` 允许 Tauri WebView origin）。
  - `main()` 在 `tauri::Builder::setup()` 中同步调用 `boot()`，将 `base`/`token` 通过 `window.__SHIRITA_RUNTIME__` 注入到 WebView。
  - `RunEvent::Exit` 时优雅关闭：cancel token → wait 50ms → close pool。
- **⚠️ 需要你关注的点**：
  - **boot 的所有错误都用 `format!(...)` 转为 String**，导致上游无法区分错误类型。但 Tauri 入口直接在 dialog 中显示后 exit(1)，不需要区分。
  - **`std::fs::create_dir_all` 是同步的**，在 async boot 函数中使用同步文件操作（但 boot 被 `block_on` 包裹，在独立线程中运行，不是 tokio 运行时问题）。
  - 服务绑定 `127.0.0.1:0`（随机端口），Windows 防火墙可能弹出提示——注释中已提示用户。
  - **`tauri::async_runtime::spawn` 中 `axum::serve`**：如果服务器启动失败（端口被占），错误仅被 `tracing::error!` 记录，不会传播到 `setup()`。但实际上在 `boot()` 中已绑定成功后才 spawn，所以服务器不会因 bind 失败。
  - WebView **初始化脚本**将运行时配置注入为 `window.__SHIRITA_RUNTIME__`，其中包含明文 token——这是设计的（桌面场景下攻击面有限）。



---

# 四、shirita-ui — Vue 3 前端

I have now read every source file. Below is the complete documentation.

---

# Shirita Frontend Module Documentation

File paths are absolute, rooted at `/home/cc/workspace/shirita/shirita-ui/src/`.

---

## Main entry: `main.ts`

**职责**：应用入口。创建 Vue 应用实例，挂载 Pinia、Vue Router、vue-i18n，以及在 mount 之前注入缓存的 custom CSS 以避免 FOUC。

**输入/输出**：无导出。作为 CLI 入口被 `index.html` 引用。副作用：创建并挂载全局应用实例，往 `<head>` 写入 `<style id="user-custom-css">`。

**依赖关系**：依赖 `App.vue`（根组件）、`router/index.ts`、`i18n.ts`、`styles.css`、`composables/useCustomCss`。

**实现方式**：在 `createApp` 之前调用 `bootCustomCss()` 从 localStorage 恢复 custom CSS，然后串行 `use(Pinia).use(Router).use(i18n).mount('#app')`。

**⚠️ 需要你关注的点**：无。

---

## `App.vue`

**职责**：根组件。注入主题和 custom CSS composable，用 `<AppShell>` 包裹 `<router-view>`，并在路由切换时应用 `page` 过渡动画（out-in）。

**输入/输出**：无 Props/Emits。渲染一个模板层。

**依赖关系**：依赖 `AppShell`、`useTheme`、`useCustomCss`。

**实现方式**：`<script setup>` 中调用两个 composable，模板中 `<router-view v-slot>` + `<transition name="page">`。

**⚠️ 需要你关注的点**：无。

---

## `router/index.ts`

**职责**：Vue Router 配置。定义 5 条路由：`/` (HomeView)、`/chat/:id` (ChatView)、`/new` (懒加载 NewChatView, 带面包屑 meta)、`/book` (懒加载 BookView)、`/settings` (懒加载 SettingsView)。使用 `createWebHistory`。

**输入/输出**：导出 `router` 实例。

**依赖关系**：依赖所有 View 组件。

**⚠️ 需要你关注的点**：`/settings` 和 `/book` 的路由没有 `meta.crumbs`，而 `/new` 有。这似乎是设计意图，但与其他页面一致性问题值得关注。

---

## `i18n.ts`

**职责**：创建 vue-i18n 实例。语言为 en（fallback）、zh-Hans、zh-Hant、ja。初始 locale 由 `resolveInitialLocale` 决定。

**输入/输出**：导出 `i18n` 实例。

**依赖关系**：依赖 `locales/en`、`zh-Hans`、`zh-Hant`、`ja`、`locales/resolve`。

**⚠️ 需要你关注的点**：无。

---

## API layer: `api/client.ts`

**职责**：Http API 客户端。封装所有后端 REST 端点的 fetch 调用。约 50 个导出函数，覆盖会话 CRUD、消息流式发送/重生成、定义增删改、模板 CRUD、节点树操作、设置、媒体库、导入导出、Pack 操作、面板等。

**输入/输出**：每个函数接受路径参数 + 可选 body，返回 Promise<T> 或 AsyncGenerator<SseEvent>。`assetUrl` 将相对路径转为带 BASE 前缀的绝对 URL。`abortSession` 是 best-effort 的 stop 请求。

**依赖关系**：依赖 `api/types`。无框架依赖。

**实现方式**：
- 运行时配置通过 `window.__SHIRITA_RUNTIME__` 注入（用于 Tauri 嵌入场景），fallback 到 `VITE_API_BASE` / `VITE_API_TOKEN` 环境变量。
- SSE 流式处理：`readSse` 使用 `ReadableStream.getReader()` 逐行解析 `data: {...}\n` 格式，yield 出 `SseEvent`。
- 下载功能（`downloadExport`、`downloadPackExport`）使用 blob URL + 临时 `<a>` 触发浏览器下载。

**⚠️ 需要你关注的点**：
1. **错误处理泛化**：所有 API 函数都在 `!res.ok` 时 throw `new Error(...)`，但错误消息只包含 HTTP status 码，没有任何响应体内容。后端可能返回更详细的错误信息（例如 JSON body 中的 error 字段）但前端未读取。
2. **`abortSession` 的 try/catch 为空**：注释说 "Best-effort"，但空的 catch 会吞掉所有异常，可能掩盖真实问题。
3. **`readSse` 中的 `reader.releaseLock()`** 放在 `finally` 块中，但如果在 `reader.read()` 中抛异常，锁可能已被释放。这实际上是标准做法，没有问题。
4. **`modelCatalog.ts`** 是一个纯数据文件（硬编码 fallback 模型列表），没有测试文件。

---

## `api/types.ts`

**职责**：TypeScript 类型定义。包含 `Session`、`Message`、`Definition`、`Template`、`VarDecl`、`PromptNode`、`Pack`、`PackIdentity`、`SessionPanel`、`PanelCaps`、`PanelAction`、`DefType`、`Trigger`、`RegexRule`、`RegexScope`、`Identity`、`ImportSummary`、`OnConflict` 等类型。还包含两个辅助函数：`triggerFromMeta`（从 meta 读取 Trigger 对象）和 `PanelAction` 联合类型。

**实现方式**：纯类型定义。`triggerFromMeta` 有 lenient 解析逻辑（默认值回退）。

**⚠️ 需要你关注的点**：无。

---

## `api/modelCatalog.ts`

**职责**：提供每个提供商的硬编码 fallback 模型列表。当没有 API key 时无法查询 `/models` endpoint，使用此列表作为备选。

**实现方式**：`Record<string, string[]>` 常量。

**⚠️ 需要你关注的点**：无测试。这些列表需要定期更新。

---

## Stores (Pinia)

### `stores/chat.ts`

**职责**：聊天会话的状态管理。管理消息列表、活动分支叶子、流式状态（streaming text/error）、消息的 CRUD、分支切换、frok、optimistic user message。

**输入/输出**：暴露 `messages`、`activeLeafId`、`displayed`（computed：活动分支路径）、`loading`、`error`、`isStreaming`、`streamingText`、`streamingError`、`activeSessionId`。动作：`loadMessages`、`send`、`regenerate`、`stop`、`abortActive`、`switchLeaf`、`editMsg`、`toggleHidden`、`fork`、`remove`、`clearStreaming`。

**实现方式**：
- `send` 使用 optimistic 更新：先插入本地 placeholder 消息，然后消费 SSE 流，收到 `done` 后重新加载消息列表。如果流错误，回滚 placeholder。
- `stop` 调用后端 `abortSession` + 本地 `AbortController.abort()`。
- `abortActive` 只 abort 本地的 fetch（不通知后端），用于导航离开时。
- `consume` 处理三种 SSE 事件：`delta`（累积文本）、`done`（刷新消息 + 桌面通知）、`stopped`（刷新但不报错）、`error`（设置 streamingError）。

**测试覆盖**：测试了 `loadMessages`、`send` 的 optimistic/rollback/error 流程、`clearStreaming`、分支路径计算、`switchLeaf`。未测试：`regenerate`、`stop`、`editMsg`、`toggleHidden`、`fork`、`remove`。

**⚠️ 需要你关注的点**：
1. `stop()` 先 await `abortSession` 再 `activeAbort?.abort()`。如果 `abortSession` 慢，用户会看到延迟。但这是有意设计的：先让后端持久化，再硬 abort。
2. `abortActive()` 不调用 `abortSession`，所以导航离开时后端 SSE 连接会继续运行直到超时。
3. `makeOptimisticUserMessage` 使用 `Date.now()` 生成 id 前缀 `__pending-`，不保证唯一性（极端情况下快速连续发送可能冲突）。

### `stores/sessions.ts`

**职责**：会话列表的状态管理。加载、删除、重命名、复制、重新排序会话。

**输入/输出**：暴露 `items`、`loading`、`error`。动作：`load`、`remove`、`duplicate`、`rename`、`reorder`。

**实现方式**：`reorder` 先本地重新排列列表（乐观更新），然后 PUT 到后端；失败时将 error 写入 store 但不回滚列表。

**测试覆盖**：无独立的测试文件，但在 HomeView.test.ts 中覆盖了 `rename`。

**⚠️ 需要你关注的点**：无。

### `stores/library.ts`

**职责**：管理所有"库"数据：definitions、templates、container types、packs。提供 `loadAll` 并行加载全部、以及 `addType`/`removeType`。

**输入/输出**：暴露 `definitions`、`templates`、`containerTypes`、`packs`、`loading`、`error`。动作：`loadDefinitions`、`loadTemplates`、`loadTypes`、`loadPacks`、`addType`、`removeType`、`loadAll`。

**实现方式**：`loadTypes` 按 builtin 优先、sort 次之排序。`addType` 自动设置 sort 为当前长度。

**测试覆盖**：library.test.ts 只测试了 `loadPacks` 和 `loadAll` 中的 pack 加载。`loadDefinitions`、`loadTemplates`、`loadTypes`、`addType`、`removeType` 未被测试。

**⚠️ 需要你关注的点**：无。

### `stores/media.ts`

**职责**：媒体库（上传的图片）的状态管理。按 kind（avatar/background）分组缓存 assets。

**输入/输出**：暴露 `assets`（`Record<kind, Asset[]>`）、`loaded`（缓存标志）、`error`。动作：`byKind`、`load`、`invalidate`、`upload`、`rename`、`remove`。

**实现方式**：按 kind 缓存，`invalidate` 清除加载标志下次重新拉取。`rename` 使用乐观更新（先改本地，失败回滚）。`upload` 将新 asset 插入到列表头部。

**测试覆盖**：media.test.ts 测试了按 kind 加载和 invalidate 逻辑。未测试：`upload`、`rename`、`remove` 的乐观更新和错误回滚。

**⚠️ 需要你关注的点**：`rename` 的乐观更新很精细，但如果在 `renameAsset` 调用有其他并发的 rename，可能出现 stale 数据。

### `stores/settings.ts`

**职责**：应用设置的状态管理。加载/保存设置、测试 provider 连接、拉取模型列表。

**输入/输出**：暴露 `data`、`loading`、`error`、`testStatus`、`testError`、`models`、`modelsLoading`、`modelsError`、`modelsSource`。动作：`load`、`save`、`testConnection`、`useFallbackModels`、`fetchModels`。

**实现方式**：`fetchModels` 处理三种响应模式：正常模型数组、error 对象（字符串或 `{message}`）、未知响应。`save` 会将 patch merge 到本地 `data` 并重新 throw 错误给调用方。

**测试覆盖**：无独立测试文件。（在 SettingsView 测试中覆盖）。

**⚠️ 需要你关注的点**：
1. `save` 会 `throw e` 让调用方处理，但 store 内已设置 `error.value`。这可能导致重复的错误处理。
2. `modelsSource` 只在 fetch 成功时设为 `live`，但没有在失败时重置的逻辑。

### `stores/ui.ts`

**职责**：UI 偏好设置的状态管理。使用选项式 API（非 setup 函数）。持久化到 localStorage。

**输入/输出**：暴露 `messageStyle`、`theme`、`background`、`activeChatId`、`locale`、`contentWidth`。动作：`setActiveChatId`、`setMessageStyle`、`setTheme`、`setBackground`、`setContentWidth`、`setLocale`。

**实现方式**：每个 setter 更新 state 并同步到 localStorage。`setLocale` 额外更新 `i18n.global.locale.value`。

**测试覆盖**：ui.test.ts 覆盖了默认值、持久化、activeChatId 的基本设置。

**⚠️ 需要你关注的点**：
1. **`activeChatId` 只在 AppShell 中设置**：路由 watch `/chat/:id` 时设为 route params 的 id，路由 `/` 时设为 null。这意味着如果用户直接输入 URL `/chat/xxx`，必须先经过 AppShell 渲染。这是当前的设计，但耦合度较高。
2. locale 切换通过 `setLocale` 调用，但没有同步到服务器设置——它只是客户端偏好。

---

## Views

### `views/HomeView.vue`

**职责**：对话列表主页。显示会话卡片、支持拖拽排序、重命名、删除、复制、导出/导入会话。

**输入/输出**：无 Props。使用 `useSessionsStore`。渲染 `ChatCard` 列表 + 导入/编辑/新建按钮。

**实现方式**：
- 拖拽排序：native HTML5 drag-and-drop，`dragFrom` 记录源索引，`onDragOver` 实时重排列表，`onDrop` 调用 `store.reorder`。
- 编辑模式：`editMode` 切换聊天卡片的菜单/拖拽句柄/删除按钮。
- 导入：文件 input + `importSession` API + 重新加载。
- 导出：`exportSession` API + blob URL 下载。

**测试覆盖**：基本渲染、编辑模式切换、空状态、store rename。未测试：拖拽排序、导出、导入。

**⚠️ 需要你关注的点**：
1. **拖拽时 `dragFrom` 是模块级变量**，在多实例场景下可能互相影响（虽然 HomeView 通常单例）。
2. `onExport` 中直接写 `store.error = (e as Error).message`，绕过了 store 的 action，直接操作内部属性。

### `views/ChatView.vue`

**职责**：聊天详情页。加载消息、发送/重新生成消息、管理会话变量状态和面板。

**输入/输出**：无 Props。从路由 params 读取 `sessionId`。使用 `chat`、`settings`、`ui` store。渲染 `MessageList`、`VariablesPanel`、`Composer`、PanelView 列表、header（角色名称/头像）。

**实现方式**：
- 加载状态、身份、面板、会话变量。
- `effectiveIdentity` computed 合并了运行时变量（`$assistant_name`、`$avatar`）的覆盖逻辑。
- `onPanelAction` 根据面板 caps 处理三种 action：diff（写入状态变量）、insert（填入 composer）、send（直接发送）。
- `sanitizeCardContent` 截断 HTML card 注入的文本内容。
- 从 `window.addEventListener('message')` 监听 HTML card 的 `shirita-add-input` postMessage。
- 离开时调用 `chat.abortActive()` 硬中止流连接。

**测试覆盖**：加载消息、渲染消息、加载状态、错误状态、发送消息、变量面板、身份显示、面板渲染、`$assistant_name` 覆盖。未测试：面板 action 处理、postMessage 监听、`handleRegenerate`、隐藏/分支/删除消息的流程。

**⚠️ 需要你关注的点**：
1. **`loadState` 和 `loadIdentity` 的静默错误处理**：如果 API 失败，它们默默地回退到空值/默认值，不给用户任何反馈。
2. **`loadPanels` 同上**：失败时回退到空数组。
3. **`settings.data` 的 `user.name` 和 `user.avatar` 类型断言**：用 `as string` 假设存在，如果实际存储的是非字符串类型会返回 `undefined`，`??` 会正确处理。
4. **`headerName` computed 的 `|| t('chat.title')` 降级**：如果 API 返回 null 且没有 sessionState 覆盖，显示 i18n 标题。

### `views/NewChatView.vue`

**职责**：新建聊天页。选择一个模板、挂载 packs、设置头像覆盖、创建聊天会话。

**输入/输出**：无 Props。使用 `library` store。渲染名称输入、模板选择器、pack 选择器（带拖拽排序 chips）、头像选择器、创建按钮。

**实现方式**：
- 自动选择标记为 `default` 的模板。
- 拖拽排序 pack chips（与 PromptTree 相同的 `grabbedHandle` 模式）。
- `createChat` 中名字回退逻辑：用户输入 > 第一个 pack 名 > "Untitled"。

**测试覆盖**：渲染、pack 增删排序、默认模板选择、创建带 packs 的会话。比较全面。

**⚠️ 需要你关注的点**：无。

### `views/SettingsView.vue`

**职责**：设置页。约 1040 行的巨型组件。涵盖身份、Provider、生成参数、上下文/自动摘要、外观、通知、正则规则、语言选择、关于。

**输入/输出**：无 Props。使用 `settings`、`ui` store。渲染一系列表单控件。

**实现方式**：
- **自动保存机制**：所有 `settings.data` 的更改通过一个 debounced watch（600ms）自动保存。watch 的依赖列表显式列出所有设置字段，使用 `JSON.stringify` 的数组比较模式。
- Provider 配置：使用 `provider.<source>.<field>` 命名空间（`providerKey` 工具），切换 source 不会互相污染。
- 模型列表：有 API key 时从 `/provider/models` 实时拉取（debounced 800ms），无 key 时使用 fallback catalog。
- 正则规则管理：在 Settings 页内直接 CRUD 定义（type=`regex_rule`），只显示 global scope 的规则。
- 旧设置迁移：`onMounted` 中检查并迁移旧的扁平 provider keys（`provider_base_url` 等）到命名空间格式。

**测试覆盖**：SettingsView.fixes.test.ts 测试生成参数默认值、max tokens unlimited、custom CSS placeholder 不泄露、API key 显示切换。SettingsView.i18n.test.ts 测试 locale 切换、通知切换的交互行为（包括权限被拒/不支持时的错误提示）。未测试：provider 切换、模型加载、正则规则 CRUD、设置保存。

**⚠️ 需要你关注的点**：
1. **组件体积过大**：1040 行的单一组件包含了大量状态、计算属性和模板。逻辑分散，抽取成子组件的机会很大（参见 memory 中#13 settings polish）。
2. **自动保存 watch 的依赖数组**：使用 `JSON.stringify` 风格的数组比较（Vue 3 watch 默认对数组进行内部值比较）。这意味着如果数组内数组引用变化但值相等，watch 不会触发——但有时这正好是需要的。
3. **`notifyEnabled` toggle 的处理**：`handleNotifyToggle` 先设置 `notifyEnabled`，然后检查权限，如果未授权则回退。这会导致 UI 短暂闪烁（toggle 先开再关）。
4. **`ruleTimers` Map 使用 `setTimeout` 进行 debounce**：每个正则规则的修改都有独立的 500ms debounce timer，但没有清理旧的 timers 的机制（虽然 `clearTimeout` 已经做到了）。
5. **`exportAll` 和 `importAll` 按钮没有绑定任何 handler**：模板中渲染为 `button` 但没有 `@click` 处理。

### `views/BookView.vue`

**职责**：核心编辑器（"Book"）页。约 1240 行。管理模板编辑、Pack 编辑、定义编辑器、本地会话覆盖（local copy-on-write）、导入/导出。

**输入/输出**：无 Props。使用 `library`、`ui`、`media` stores。渲染三个部分：Local section（会话上下文覆盖）、Global section（模板+Packs+定义编辑器）。

**实现方式**：
- 分三个"section"：模板（mauve 色调）、Pack（teal 色调）、定义（中性色调）。
- Local section：当 `ui.activeChatId` 存在时显示。初始状态只有一个 "Customize locally" 按钮，点击后展开 `BookNavigator`。
- **copy-on-write 系统**：
  - `localSession` 通过 `getSession` 加载，`localDefs` 从 `override_config.local_definitions` 读取。
  - `editLocal` 深拷贝全局定义到 `localEditDef`，允许编辑后通过 `setLocalDefinition` 保存 diff。
  - `saveLocal` 只发送与全局不同的字段（patch）。
  - `revertLocal` 调用 `clearLocalDefinition`。
  - `localVariables` 读取并保存会话级变量。
  - 模板节点的本地覆盖：调用 `materializeNodes` 先物化，然后在会话级别创建/更新/删除节点。
- **`LocalBookApi` 注入**：通过 `provide(LOCAL_BOOK_KEY)` 将 `localBookApi` 对象传递给 BookNavigator 的子组件。

**测试覆盖**：
- 本地/全局 section 的显示条件。
- 自定义后通过 navigator drill into definition。
- 新建 session node（无 `_source` meta）正确显示。
- 深拷贝保证全局不可变。
- Pack section 的渲染、导入按钮、导入后自动跳转。
- panel 导入提示。
- 模板选择持久化（localStorage）。
- 默认模板 toggle。

**⚠️ 需要你关注的点**：
1. **组件体积极大**：1240 行，与 SettingsView 类似，抽取子组件可以显著改善可维护性。
2. **`materializeAll` / `ensureMaterialized` **：`materializeNodes` 只有 `localNodes.value.length === 0` 时才执行，但物化后如果用户删除了所有节点（剩余 0 个），再次编辑不会触发物化，因为列表已经为 0 且不会再从 0 变回 0。
3. **`localAddPrompt` 和 `localAddContainer` 等函数**：每次都调用 `ensureMaterialized()`。如果会话已经有本地节点，这只是个无操作的条件检查，但不必要的 `loadLocalNodes()` 每次都会被调用。
4. **`selectedTemplateId` 和 `selectedPackId` 的 localStorage 持久化**：使用 try/catch 忽略可能的 QuotaExceededError。
5. **`importSummary` 中 panel hint 的条件**：只有当 `created.some(c => c.kind === 'panel')` 时才显示提示。这是从 backend 转换的 panel 到 bricks 的提示，正确。

---

## Components

### `components/AppShell.vue`

**职责**：应用外壳。提供背景图、头部导航（品牌 logo、面包屑、三个图标按钮：Chat/Book/Settings）、内容区域 slot。

**输入/输出**：无 Props。默认 slot 渲染路由内容。

**实现方式**：
- 通过 `useRoute` 检测当前 section（chat/book/settings）。
- `activeChatId` 通过 watch route 维护，使得 Chat 图标始终指向当前会话（即使浏览其他页面）。
- 面包屑从路由 meta 读取（只在 `/new` 路由设置）。
- 背景图片通过 computed `bgStyle` 生成 CSS background-image。

**测试覆盖**：三个导航链接、active section 高亮、Chat 图标指向（导航离开时保持、返回列表后忘记）、品牌图标、无 footer。

**⚠️ 需要你关注的点**：无。

### `components/ChatCard.vue`

**职责**：会话卡片组件。显示会话名称、预览、更新时间、头像（tinted 占位符）。支持编辑模式（拖拽+删除）和正常模式（三点菜单）。

**输入/输出**：Props: `session`、`editMode`。Emits: `duplicate`、`export`、`delete`、`rename`。

**实现方式**：
- 头像 tint 使用会话 id 的 hash 从 4 个预设颜色中选取。
- 菜单使用 `<transition name="expand">` + click-away overlay。
- 编辑模式阻止卡片点击导航。

**测试覆盖**：链接、菜单操作（包括 delete emit）、编辑模式的拖拽/删除/导航阻止。

**⚠️ 需要你关注的点**：无。

### `components/Composer.vue`

**职责**：消息输入框组件。支持文本输入（auto-grow textarea）、图片附件上传、发送/停止按钮、token 估算显示。暴露 `setText` 方法供外部（ChatView 的 HTML card postMessage）注入内容。

**输入/输出**：Props: `disabled`、`streaming`。Emits: `send`、`stop`。Expose: `setText`。

**实现方式**：
- Enter 发送、Shift+Enter 换行。
- 图片上传使用 `uploadAsset` 后添加附件缩略图。
- 发送时清空文本和附件列表。

**测试覆盖**：渲染、发送（trimmed text）、Enter/Shift+Enter、空文本不发送、disabled、附件上传、仅附件可发送。比较全面。

**⚠️ 需要你关注的点**：无。

### `components/MessageList.vue`

**职责**：消息列表容器。过滤掉 anchor 消息，计算活跃分支的 sibling 信息，渲染 streaming ghost 消息和 streaming 错误。

**输入/输出**：Props: `messages`、`allMessages`、`style`、`isStreaming`、`streamingText`、`streamingError`、`identity`、`tokens`。Emits: 冒泡 `copy`、`regenerate`、`fork`、`edit-save`、`toggle-hidden`、`delete`、`swipe`。

**实现方式**：
- `sibInfo` 使用 `siblings` 工具计算每条消息的同级位置。
- 最后一条可见消息显示 token 计数。
- streaming ghost 构造一个伪 `Message` 对象。

**测试覆盖**：消息渲染、空状态、anchor 过滤、streaming ghost、streaming error、样式传递、事件冒泡、token 计数、identity 传递。比较全面。

**⚠️ 需要你关注的点**：无。

### `components/MessageItem.vue`

**职责**：单条消息显示。支持 bubble 和 flat 两种样式模式，包含操作按钮（复制、重新生成、分支、编辑、隐藏、删除、swipe 切换）、内联编辑、附件显示。

**输入/输出**：Props: `message`、`style`、`isStreaming`、`siblingIndex`、`siblingCount`、`identity`、`tokens`。Emits: `copy`、`regenerate`、`fork`、`edit-save`、`toggle-hidden`、`delete`、`swipe`。

**实现方式**：
- bubble 模式下用户消息右对齐，assistant 消息左对齐。
- 使用 `side` computed 根据角色选择身份（assistant/user）。
- 附件通过 `media` store 将 asset id 解析为 URL。

**测试覆盖**：bubble/flat 渲染、display_content 控制标签隐藏、头像显示、操作按钮可见性条件、复制 emit、streaming cursor、swipe 指示器、token 显示、内联编辑。identity 名称显示。

**⚠️ 需要你关注的点**：
1. **`onMounted` 加载 media store**：每个 MessageItem 实例都触发 `media.load('avatar')` 和 `media.load('background')`，这会多次调用 API（但 store 有缓存，第二次开始是空操作）。

### `components/MessageContent.vue`

**职责**：消息内容渲染。使用 `splitThinking` 分割 `` 和未闭合（streaming）的 `
