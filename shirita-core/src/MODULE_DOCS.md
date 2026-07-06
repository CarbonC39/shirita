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
- **输入/输出**：assemble_from_nodes/assemble_from_nodes_with_packs 返回 AssembledPlan（含 segments、regex_rules、depth_inserts）。build_chat_messages 将 plan + history 转为 Vec<ChatMessage>。
- **依赖关系**：依赖 state（变量渲染）、keyword（Aho-Corasick 扫描）、models。被 conversation 调用。
- **实现方式**：
  - activate() 分三轮：先处理 constant/random 激活，然后按 scan_depth 分组使用 KeywordIndex 扫描最近消息，最后对 recursive 条目进行最多 3 轮的递归扫描。
  - assemble_from_nodes_with_packs 遍历 template/session 树 + pack 树，处理 History/Content/Folder/Ref 四种节点类型。
  - build_chat_messages 按 BeforeHistory/AfterHistory 排序，depth_inserts 从距离末尾计算插入位置，合并相邻同 role 消息。
- **⚠️ 需要你关注的点**：
  - **render_vars** 每次调用都创建新的 regex::Regex，未使用 LazyLock 或静态。
  - **apply_regex_rules_for** 中对编译失败的正则仅做 tracing::warn 并跳过，做得到"运行时宽容"。

---

## state

- **职责**：变量状态沙箱——声明 schema、合并有效状态、解析和应用 `<state_update>` 指令。纯函数，无 I/O。
- **输入/输出**：parse_state_updates、apply_updates、effective_state、variables_from_nodes、resolve_schema_from_bricks。
- **依赖关系**：纯函数，依赖 models。被 conversation 和 adapters/stpreset 调用。
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

## adapters (4 个子模块)

### charcard
- **职责**：ST 角色卡 V2/V3 -> LoreSet（Template + Definitions + Nodes）转换。
- **实现方式**：将每个非空字段映射为定义 + ref 节点，组成 2 级模板树。支持正则脚本转换、tavern_helper 变量提取、Panel 转换（识别状态条模式 -> HTML/CSS/Variables 砖块）。
- **⚠️ 需要你关注的点**：
  - **charcard_to_loreset 函数超长**（约 200 行）。
  - **loreset_to_pack** 过滤 History/Content 节点——但过滤前已经修改了所有节点的 owner_kind，有冗余修改。
  - **try_convert_status_panel** 状态条检测逻辑复杂，仅当恰好有一个明确候选时才转换。

### preset
- **职责**：模板节点树 -> ST preset JSON 序列化。代码简洁。

### stpreset
- **职责**：ST chat-completion preset -> LoreSet 导入。
- **实现方式**：解析 setvar::/getvar:: 宏提取变量声明；选择最大的 prompt_order 组；检测跨节点标签跨度并合并为文件夹。
- **⚠️ 需要你关注的点**：
  - **stpreset_to_loreset 函数极长**（约 300 行）。
  - **活跃顺序处理和 inactive 尾部处理**存在明显的代码重复。
  - **find_first_span** 已优化为 O(n)，性能正确。

### worldinfo
- **职责**：ST World Info -> world 类型定义列表。
- **实现方式**：支持 entries 为 map 或 array 格式；as_u64_lenient 宽松解析数值字段。
- **⚠️ 需要你关注的点**：代码简洁，无问题。

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

## pngcard

- **职责**：解析嵌入 PNG 文件中的 ST 角色卡 JSON。
- **实现方式**：手动解析 PNG chunk 结构，优先 ccv3 后 chara，Base64 解码。
- **⚠️ 需要你关注的点**：
  - **不验证 CRC**——设计意图。
  - **MAX_TEXT_CHUNK=8MB** 保护，在读取数据之前返回错误，正确防御 OOM。
  - 代码干净，无 unwrap。

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
| adapters/charcard.rs:142 | `expect("tag is always static")` | tag 固定，安全 |

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
