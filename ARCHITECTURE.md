# Shirita 架构文档

> Shirita 是一个本地优先的 AI 角色扮演/聊天应用，支持 Web 和桌面（Tauri）两种部署方式。

---

## 1. 项目概览

```
shirita/
├── shirita-core/          # Rust 核心业务库（与部署方式无关）
├── shirita-web/           # Rust HTTP 服务层（Axum REST + SSE）
├── shirita-tauri/         # Tauri v2 桌面壳（嵌入 shirita-web 的 HTTP 服务器）
├── shirita-ui/            # Vue 3 前端 SPA（Web + Tauri 共用）
├── Cargo.toml             # Rust 工作空间根
├── deploy/                # Docker / 部署配置
└── docs/                  # 文档 / 设计笔记
```

### 依赖关系

```
shirita-tauri ──depends on──> shirita-web ──depends on──> shirita-core
                                                                │
                              shirita-ui (Vue SPA) ──HTTP──>  Axum routes
                              (在后端 shirita-web 中 serve)
```

---

## 2. 模块 / Crate 划分及职责

### 2.1 `shirita-core` — 核心业务库

纯业务逻辑，零 HTTP 依赖，可被任意 Rust 宿主使用。

| 模块 | 职责 |
|---|---|
| `models/` | 领域模型：`Definition`（条目）、`Session`（会话）、`Message`（消息、含 `Role`）、`PromptNode`（节点树）、`Template`（模板）、`Pack`（内容包、含 `PackIdentity`）、`Summary`（摘要）、`Asset`（媒体资产）、`DefType`（容器类型注册表） |
| `storage/` | `Storage` trait + `SqliteStorage` 实现（sqlx + SQLite）。CRUD + 事务性导入/复制/物化 |
| `model/` | `ModelProvider` trait + 适配器：`OpenAiProvider`（兼容 OpenAI）、`AnthropicProvider`（Claude Messages API）、`EchoProvider`（测试用） |
| `conversation.rs` | 对话服务：`send_message` 和 `regenerate` 的完整流程（见 §5）。`StopToken`/`StopHandle` 协作终止。包含 `resolve_session_schema`（从变量砖块解析 schema）和 `resolve_session_identity`（解析身份） |
| `assembly.rs` | 提示词组装引擎：遍历节点树 → 触发激活（constant/keyword/random）→ 渲染变量 → XML 包裹 → 构建 `ChatMessage` 数组。含正则规则引擎（`apply_regex_rules`）、面板状态捕获（`capture_panel_updates`）、注释剥离、历史分割、pack 内容注入 |
| `state.rs` | 变量声明系统：`VarDecl`/`VarType` 定义、`<state_update>` 解析与应用（`apply_updates`/`parse_state_updates`）、schema 折叠、`resolve_schema_from_bricks`（从多个来源的 `variables` 砖块合并 schema）、快照渲染/剥离 |
| `budget.rs` | Token 预算管理：上下文窗口裁剪（`trim_history`） |
| `summarize.rs` | 滚动摘要生成：对话超过窗口后的自动摘要 |
| `tokenizer/` | `TokenCounter` trait + `TiktokenCounter`（tiktoken-rs 包装） |
| `adapters/` | 导入适配器：Character Card（PNG）、SillyTavern 预设/世界信息、LoreSet → Pack 转换 |
| `portable.rs` | 便携格式：导出/导入为 .zip（含定义、节点树、资产）和 .json 模板包 |
| `attachments.rs` | 附件处理：将 asset ID 解析为图片 data URL，注入到 prompt 上下文中 |
| `html_patch.rs` | HTML 卡片修补：SEARCH/REPLACE 差异格式的解析、合并与重建。模型可以增量编辑已渲染的 HTML 文档 |
| `identity.rs` | 身份解析：从会话的定义中解析助手/用户显示名和头像。支持 pack 身份覆盖 |
| `panels.rs` | 面板解析：从节点树中收集 `panel` 标签文件夹及其 `html`/`css` 砖块，渲染为 UI 可消费的 `RenderedPanel`（含 name/html/css/caps/min_messages） |
| `hashing.rs` | 内容哈希：SHA-256 哈希工具，用于资产去重 |
| `keyword.rs` | Aho-Corasick 多关键词匹配引擎 |
| `tree.rs` | 消息树遍历：`active_path` 从消息森林中提取当前分支 |
| `config.rs` | 环境变量配置读取（含 HTTP Basic Auth、provider 覆盖） |
| `seed.rs` | 首次启动种子数据（默认模板、内置定义、backfill 迁移：content 节点、全局正则标志、资产哈希） |
| `pngcard.rs` | PNG Character Card 提取 |
| `error.rs` | 统一错误类型 |

### 2.2 `shirita-web` — HTTP 服务层

基于 Axum 0.8 的 REST + SSE 服务。

| 模块 | 职责 |
|---|---|
| `main.rs` | 独立 Web 入口：绑定地址 → 初始化 DB/Provider → `axum::serve`；含 loopback 绑定检测警告 |
| `lib.rs` | `app()` 路由组装 + `app_with_cors()`（Tauri 用 CORS 包装） |
| `routes/` | 按资源划分的 22 个路由模块（见下方路由结构） |
| `auth.rs` | 双层认证：`require_bearer`（API 层，Bearer token）+ `require_basic`（最外层，可选 HTTP Basic Auth，用于公网部署） |
| `state.rs` | `AppState`：共享的 `Storage`、`ModelProvider`、`Config`、`TokenCounter`、`Generations` 注册表、`reqwest::Client` |
| `generations.rs` | 生成注册表：追踪每个 session 的进行中 SSE 流，支持协作终止和新生成自动废弃旧生成 |
| `provider_select.rs` | 运行时 Provider 解析：settings 中的命名空间配置优先，否则 fallback 到环境变量。支持多个 provider source（OpenAI/Anthropic/Ollama/Google/OpenRouter/Mistral/DeepSeek/Groq/xAI/Cohere/Together/Perplexity）的默认 base URL、模型列表请求构建和响应归一化 |
| `embed.rs` | 可选嵌入 UI（`embed-ui` feature）：编译时将前端 dist 嵌入二进制 |

**路由结构**：

```
/health                          — 健康检查（无认证）
/api/ping                        — Bearer
/api/sessions[/{id}]             — 会话 CRUD + 复制/导出/排序
/api/sessions/import             — 会话导入
/api/sessions/reorder            — 会话排序
/api/sessions/{id}/duplicate     — 复制会话
/api/sessions/{id}/export        — 导出会话
/api/sessions/{id}/identity      — 会话身份
/api/sessions/{id}/messages         — 消息列表 + SSE 发送
/api/sessions/{id}/messages/{id}    — 编辑/删除消息
/api/sessions/{id}/active-leaf      — 设置 active leaf
/api/sessions/{id}/messages/{id}/regenerate — 重新生成
/api/sessions/{id}/abort         — 协作终止生成
/api/sessions/{id}/fork          — 分支克隆会话
/api/sessions/{id}/mounts        — 设置挂载定义
/api/sessions/{id}/packs         — 挂载 pack 列表
/api/sessions/{id}/panels        — 面板
/api/sessions/{id}/local-definitions/{id} — 会话级局部定义覆盖
/api/sessions/{id}/local-definitions/{id}/promote — 提升局部定义为全局
/api/sessions/{id}/materialize-nodes       — 物化模板节点树
/api/sessions/{id}/materialize-pack         — 物化 pack 节点树
/api/sessions/{id}/state               — 变量状态快照
/api/sessions/{id}/local-variables      — 局部变量覆盖
/api/sessions/{id}/state-updates        — 应用状态更新
/api/definitions[/{id}]          — 条目 CRUD + 导出
/api/templates[/{id}]            — 模板 CRUD + 复制/导出
/api/templates/{id}/nodes        — 模板节点树 CRUD
/api/templates/{id}/orphan-definitions — 孤立定义
/api/nodes/{id}                  — 节点更新/删除
/api/templates/{id}/nodes/reorder — 节点排序
/api/packs[/{id}]                — 包 CRUD + 复制/导出
/api/packs/{id}/orphan-definitions — 孤立定义
/api/packs/{id}/nodes            — pack 节点树 CRUD
/api/types                       — 容器类型注册表
/api/settings                    — 键值设置
/api/provider/*                  — Provider 连接测试/模型列表
/api/import/*                    — 导入（WorldInfo/CharCard/通用包）
/api/regex-rules/scopes          — 正则规则作用域
/api/assets                      — 媒体库上传/列表/删除
/assets/*                        — 静态文件 ServeDir
/                                — SPA index.html（Dev: proxy, Prod: embed）
```

### 2.3 `shirita-tauri` — Tauri 桌面壳

| 文件 | 职责 |
|---|---|
| `main.rs` | 启动 embedded Axum 服务器 → 创建 WebView 指向该服务器 → 注入 `window.__SHIRITA_RUNTIME__`；管理优雅关闭；插件初始化（dialog、notification） |
| `tauri.conf.json` | Tauri v2 配置：前端 dist 路径、CSP、窗口设置 |

**核心模式**：Tauri 版不调用任何 Tauri 命令（IPC）。它启动一个 `shirita-web` 的 Axum 服务器绑定到 `127.0.0.1:0`（随机端口），然后创建 WebView 加载 `http://127.0.0.1:{port}`。前后端通信完全走标准 HTTP。启动步骤包括：创建数据目录 → 随机生成 Bearer token → 运行迁移 → 种子数据（不含 `ensure_builtin_definitions`）→ 启动 Axum → 创建 WebView。

### 2.4 `shirita-ui` — Vue 3 前端 SPA

| 目录 | 职责 |
|---|---|
| `src/views/` | 页面级组件：`HomeView`（会话列表）、`ChatView`（聊天）、`NewChatView`（新建）、`BookView`（文库/模板编辑器）、`SettingsView` |
| `src/components/` | 可复用组件：`AppShell`（布局壳）、`Composer`（输入框）、`MessageList/Item/Content`（消息展示）、`PromptTree`（节点树编辑器）、`PackEditor`、`DefinitionEditor`、`NodePicker`、`EntityPicker`、`VariablesEditor`/`VariablesPanel`、`PanelView`、`HtmlCardFrame`、`MarkdownText`、`AssetPicker`、`AvatarPicker`、`ImageCropper`、`RegexRuleEditor`、`TriggerEditor`、`ChatCard`、`ToastHost`、`SegmentedControl`、`SliderControl`、`ToggleSwitch`、`FullscreenEditor`、`EntityToolbar` |
| `src/components/book/` | 文库组件：`BookNavigator`（堆栈式导航）、`DefinitionSection`、`PackSection`、`TemplateSection`、`SessionTemplateRoot` |
| `src/stores/` | Pinia 状态管理：`chat`、`sessions`、`library`（定义/模板/包）、`media`、`settings`、`ui` |
| `src/api/` | HTTP 客户端：`client.ts`（所有 API 调用 + SSE 解析）、`types.ts`（TypeScript 类型定义）、`modelCatalog.ts` |
| `src/composables/` | Vue composables：`useTheme`、`useCustomCss`、`useDefinitionOps`、`usePackOps`、`useTemplateOps`、`useImportExport`、`useLocalOverrides`、`useTreeEditor`、`useToast` |
| `src/utils/` | 工具函数：`tree.ts`（消息树）、`tokens.ts`、`thinking.ts`（think 标签渲染）、`markdown.ts`、`notify.ts`、`panel.ts`、`regexRule.ts`、`providerKeys.ts`、`clone.ts`、`time.ts` |
| `src/locales/` | i18n：`en`、`zh-Hans`、`zh-Hant`、`ja` + `resolve.ts`（运行时语言选择）+ i18n 切换测试 |
| `src/router/` | vue-router 路由配置：`/`、`/chat/:id`、`/new`、`/book`、`/settings` |

---

## 3. Web 版 vs Tauri 版

### 共享代码

| 层 | 共享程度 |
|---|---|
| `shirita-core` | **完全共享** — Tauri + Web 使用完全相同的 crate |
| `shirita-web` | **完全共享** — Tauri 将 `shirita-web` 作为库依赖 |
| `shirita-ui` (Vue SPA) | **完全共享** — 同一份 dist，两处 serve |

### 差异点

| 方面 | Web 版 | Tauri 版 |
|---|---|---|
| 入口 | `shirita-web/src/main.rs` 独立二进制 | `shirita-tauri/src/main.rs` Tauri 壳 |
| 启动方式 | `cargo run -p shirita-web` 或 Docker | `cargo tauri dev` 或 `cargo tauri build` |
| 服务器绑定 | `BIND_ADDR` 环境变量（默认 127.0.0.1:8787） | 硬编码 `127.0.0.1:0`（随机端口） |
| 静态资源 | `rust-embed` 嵌入二进制（feature `embed-ui`）或 Vite proxy（dev） | 开发期 `devUrl: localhost:5173`；生产期 `frontendDist` 由 Tauri 管理 |
| CORS | 不需要（同源） | `app_with_cors()` 允许 WebView origin |
| 认证 | Bearer token 从环境变量读取 | Bearer token 运行时随机生成（`Uuid::new_v4()`） |
| 种子数据 | `ensure_builtin_definitions`（内置定义 + 默认模板）+ `ensure_global_regex_flag` + `ensure_asset_hashes` | 不调用 `ensure_builtin_definitions`（只有默认模板），调用 `ensure_global_regex_flag` + `ensure_asset_hashes` |
| 数据库位置 | 环境变量 `DATABASE_PATH`（默认 `shirita.db`） | `app_data_dir / shirita.db`（OS 标准数据目录） |
| 崩溃处理 | 终端打印错误 | 弹出原生错误对话框（`tauri-plugin-dialog`） |
| 通知 | 无（浏览器可能支持 Web Notification） | `tauri-plugin-notification` 原生桌面通知 |
| UI 主题 | 浏览器默认 | 桌面窗口标题 "Shirita"、默认 1100×760 |

### 检测运行时

前端通过 `window.__SHIRITA_RUNTIME__` 检测运行时：

```typescript
// Tauri 版注入：{ base: "http://127.0.0.1:{port}", token: "uuid" }
// Web 版：Vite 编译期注入 VITE_API_BASE / VITE_API_TOKEN
const RT = globalThis.__SHIRITA_RUNTIME__
const BASE = RT?.base ?? import.meta.env.VITE_API_BASE ?? ''
const TOKEN = RT?.token ?? import.meta.env.VITE_API_TOKEN ?? ''
```

---

## 4. 前后端通信方式

### 4.1 REST（JSON）— 常规 CRUD

所有非流式操作使用标准 HTTP JSON：

```typescript
// 前端 api/client.ts
async function apiGet<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}/api${path}`, {
    headers: { Authorization: `Bearer ${TOKEN}` }
  })
  return res.json()
}
```

后端使用 Axum 的 `Json` extractor / `Json` response。

### 4.2 SSE（Server-Sent Events）— 流式生成

聊天消息发送和重新生成使用 SSE：

```
POST /api/sessions/{id}/messages   → SSE stream
POST /api/sessions/{id}/messages/{id}/regenerate → SSE stream
```

事件格式：

```
data: {"type": "delta", "text": "Hello..."}
data: {"type": "done", "message_id": "uuid"}

// or:
data: {"type": "error", "message": "..."}
data: {"type": "stopped", "message_id": "uuid"}
```

前端解析：

```typescript
async function* readSse(res: Response): AsyncGenerator<SseEvent> {
  const reader = res.body.getReader()
  // 按行解析 data: {...}
}
```

### 4.3 协停机制

停止按钮通过两条路径实现：

1. **HTTP 请求**：`POST /api/sessions/{id}/abort` → 触发 `Generations::stop()` → 发送 `watch::Sender` 信号
2. **本地 AbortController**：前端自身的 `AbortController` 切断 fetch 连接

后端 SSE 流中的 `tokio::select!` 同时监听 stop token 和数据流：

```rust
tokio::select! {
    _ = stop.cancelled() => { stopped = true; break; }
    item = stream.next() => { /* 处理 delta */ }
}
```

停止后，已累积的部分文本会被持久化保存（不丢弃）。

Generations 注册表保证每个 session 同一时间只有一个进行中的生成：新生成自动废弃旧生成（`futures::stream::AbortHandle` + `StopHandle`），防止多个流竞态写入。

### 4.4 Tauri 版无 IPC 调用

Tauri 版**不使用**任何 `tauri::command` 或 `invoke()`。所有前后端通信通过标准 HTTP 完成 — Tauri 只充当 WebView 壳。

---

## 5. 数据流向

### 5.1 核心流程：发送消息

```
用户输入 → Composer.vue
  → chat store.send()
    → 乐观更新：插入临时 user message
    → api client.sendMessage() → POST /api/sessions/{id}/messages
      → Axum routes::chat::send()
        → resolve_provider()  // 运行时解析 provider/model
        → conversation::send_message()
          ├── 1. 验证 session 存在
          ├── 2. 从 active leaf 获取当前分支 (active_path)
          ├── 3. 解析 schema + 折叠 branch state（含 pack 变量砖块）
          ├── 4. 存储 user message（含 snapshot_state + resolve_session_identity）
          ├── 5. resolve_session_schema(): 模板节点树 + 已挂载 pack 的变量砖块
          ├── 6. resolve_session_identity(): 模板 + pack 的身份定义
          ├── 7. resolve_images(): 附件解析为 data URL
          ├── 8. assemble_request():
          │   ├── effective_nodes(): session 自有节点优先，否则用 template
          │   ├── mounted_pack_trees(): 按挂载顺序加载 pack 节点树
          │   ├── assemble_from_nodes_with_packs():
          │   │   ├── 构造 Entry（trigger/scan/recursive）
          │   │   ├── activate(): constant/keyword/random 触发
          │   │   ├── render_vars(): {{var}} → state 值
          │   │   ├── strip_comments(): {{// ... }}
          │   │   ├── maybe_wrap(): XML 标签包裹
          │   │   ├── inject_content(): pack 内容注入（按类型分组）
          │   │   └── build_chat_messages(): segment → ChatMessage[]
          │   ├── 注入 protocol 定义（state_update / html_patch）
          │   ├── effective_regex_rules(): 全局 + 作用域规则（含 is_global 判定）
          │   └── trim_history(): 上下文窗口裁剪
          ├── 9. provider.stream_chat(req) → SSE delta stream
          ├── 10. 流式累积 full text，逐 delta yield
          ├── 11. 折叠 state_update 到 snapshot_state
          ├── 12. capture_panel_updates() 从回复提取变量变化
          ├── 13. 解析 HTML patch（parse_patches + apply_patches）→ display_content
          ├── 14. strip_state_tags() 清理显示文本
          ├── 15. resolve_display(): HTML 卡片重建
          ├── 16. 存储 assistant message + snapshot_state + display_content
          ├── 17. 更新 active leaf
          └── 18. yield Done / Stopped / Error
    → SSE 流返回到前端
      → chat store.consume(): delta 追加到 streamingText
      → Event: done → loadMessages() 全量刷新
      → Event: stopped → loadMessages() 刷新（部分保留）
```

### 5.2 重新生成（Regenerate）

```
chat store.regenerate(msgId)
  → POST /api/sessions/{id}/messages/{msgId}/regenerate
    → conversation::regenerate()
      → 找到 target assistant message
      → 使用其 parent（user turn）作为上下文起点
      → 隐藏旧 assistant message（is_hidden=true）
      → 创建兄弟 assistant message
      → 更新 active leaf 指向新消息
```

### 5.3 启动流程

```
【Web 版】
  Config::from_env()         ← DATABASE_PATH, TOKEN_SECRET 等
  SqliteStorage::connect()
  run_migrations()
  ensure_default_template()
  ensure_builtin_definitions()
  ensure_templates_have_content_node()
  ensure_global_regex_flag()  ← 为旧的自定义正则规则设置 is_global
  ensure_asset_hashes()      ← 为现有资产计算 SHA-256 哈希
  provider_from_env()        ← OPENAI_API_KEY 等
  axum::serve(listener, app(state))

【Tauri 版】
  Config::new(db, assets, random_token)
  apply_provider_env()       ← 环境变量覆盖
  SqliteStorage::connect() + run_migrations()
  ensure_default_template()
  ensure_templates_have_content_node()
  ensure_global_regex_flag()
  ensure_asset_hashes()
  provider_from_env()
  axum::serve(127.0.0.1:0, app_with_cors(state))   ← 随机端口
  WebviewWindowBuilder::new("http://127.0.0.1:{port}")
    .initialization_script("window.__SHIRITA_RUNTIME__ = {...}")
```

### 5.4 持久化架构

```
shirita.db (SQLite via sqlx)
├── sessions           — 会话元信息 + 当前状态 (current_state JSON)
│   └── active_leaf_id  — 当前分支叶子
│   └── mounted_packs    — 挂载的 pack ID 列表
├── messages           — 消息（树状结构：parent_id 自引用 FK）
│   ├── raw_content    — 原始回复（含 <state_update> 和 SEARCH/REPLACE 标签）
│   ├── display_content — 可选显示文本（HTML 重建后 / 标签剥离后）
│   ├── snapshot_state  — 分支状态快照（每一轮的变量值快照 + 身份信息）
│   ├── attachments     — 附件 JSON
│   └── is_hidden       — 隐藏标记（regenerate 用）
├── definitions        — 条目（char/world/prompt/regex_rule/protocol/html/css/variables...）
├── prompt_nodes       — 节点树（Template/Session/Pack 三级 owner）
├── templates          — 模板
├── packs              — 内容包（含 PackIdentity）
├── def_types          — 容器类型注册表
├── assets             — 媒体库（含 hash 字段用于去重）
├── summaries          — 滚动摘要
└── settings           — 键值设置（含 namespaced provider.* 配置）
```

---

## 6. 关键设计决策

### 6.1 Tauri 版走 HTTP 而非 IPC

**决策**：Tauri 版不调用任何 `tauri::command`，而是启动嵌入式 Axum 服务器 + WebView 加载 HTTP。

**理由**：
- 前后端代码完全复用（同一套 Axum 路由、同一套 Vue SPA）
- 开发期只需启动 `shirita-web` + Vite dev server 就能调试，不需要 Tauri
- 避免维护两套 API（IPC + HTTP）
- 协停（SSE AbortController + `POST /abort`）在纯 HTTP 下工作良好
- 验证方便：可以用 `curl` 测试所有端到端功能

### 6.2 节点树驱动的提示词组装

**决策**：模板定义一棵 `PromptNode` 树（Folder/Ref/History/Content），assembly 引擎遍历该树组装最终提示词。

**架构**：
```
Template/Session 节点树           Pack 节点树（多个，按挂载顺序）
       │                                │
       └──────────┬─────────────────────┘
                  ▼
      assemble_from_nodes_with_packs()
                  │
                  ├── 遍历 root 节点（按 sort_order）
                  │   ├── History → 分割 Before/After 段
                  │   ├── Content → 从 pack 注入分组内容
                  │   ├── Folder → 收集子节点 → XML <tag> 包裹
                  │   └── Ref → 检查触发条件 → 渲染变量 → 包裹
                  │
                  ▼
            AssembledPlan
            { segments[], history_enabled, regex_rules[], depth_inserts[] }
                  │
                  ▼
            build_chat_messages()
            → System 段 + History + System 段 + depth_inserts
            → 合并相邻同角色消息
```

**理由**：这种设计让用户可以结构化管理提示词（类似 SillyTavern 的"人物卡片"概念），而非手工编辑原始 prompt。

### 6.3 三层触发机制（Constant / Keyword / Random）

**决策**：每个 `world` 类型的条目可以有三种激活模式：
- **Constant**：总是激活（等效于角色卡片的固定描述）
- **Keyword**：根据最近 N 条消息中是否出现关键词激活
- **Random**：按概率随机激活

支持递归扫描：激活的内容本身会再次触发关键词扫描（最多 3 轮）。每个条目的 `scan_depth` 和 `recursive` 可单独配置。

**理由**：与 SillyTavern 的世界信息（World Info）兼容，同时提供更大灵活性。

### 6.4 变量系统：`<state_update>` 标签 + 分支快照 + 砖块化 schema

**决策**（核心三层结构）：
1. **Schema 定义**：由 `variables` 类型条目的 `meta.decls` 定义（名称、类型、初始值）。这些"变量砖块"可以存在于模板或 pack 的节点树中
2. **Schema 合并**：`resolve_schema_from_bricks` 将模板/会话的变量砖块与所有已挂载 pack 的变量砖块按顺序合并
3. **状态演化**：模型在回复中嵌入 `<state_update>` 标签 → 引擎解析、折叠到 `snapshot_state`、从 `display_content` 剥离
4. **状态优先级**：`snapshot > 会话 current_state > schema 初始值`，下层覆盖上层

**理由**：
- 将变量声明从 Pack/Template 的元数据"上帝对象"中解耦，成为独立的 definition 类型
- Pack 作者可以在自己的节点树中声明变量，无需修改模板
- Schema 在组装时由引擎从多个来源（模板/各 pack）动态合并

### 6.5 消息树 + 分支 + 覆盖（Swipe 风格）

**决策**：消息以树状结构存储（`parent_id` 自引用 FK），`active_leaf_id` 决定当前显示的分支。重新生成（regenerate）时：
1. 将旧 assistant 消息标记为 `is_hidden`
2. 创建同 parent 的兄弟消息
3. 更新 active_leaf 指向新消息

前端通过 `activePath()` 从消息森林中提取当前分支（从根到 active leaf 的路径）。

**理由**：SillyTavern 风格的 "swipe"（滑动切换回复）用户体验。

### 6.6 协作停止（Cooperative Stop）

**决策**：停止按钮不 kill 请求，而是：
1. `POST /api/sessions/{id}/abort` → `Generations::stop()` → `watch::Sender`
2. SSE 流中的 `tokio::select!` 在下一个 delta 到达前检测到停信号
3. 将已累积的部分文本持久化为 assistant message
4. 前端 reload 显示部分回复

此外，`Generations` 注册表保证每个 session 同一时间只有一个进行中的生成：新发送/regenerate 自动废弃旧生成（`AbortHandle`）。

**理由**：用户不想丢失模型已经生成的内容，只想打断继续生成。多生成防竞态确保不会有两个流写入同一 session。

### 6.7 双层认证

**决策**：
- **Bearer token**（内层）：保护 `/api/*`，JS 主动在 fetch 中设置
- **HTTP Basic Auth**（外层，可选）：保护整个应用（包括 HTML/JS/CSS），用于公网部署

当 Basic Auth 开启时：
- 浏览器原生登录对话框保护页面加载
- 已登录页面中的 fetch() 发 Bearer（JS 显式设置后会覆盖浏览器缓存的 Basic header）
- 后端 `require_basic` 中间件同时接受有效的 Bearer token（避免 API 调用重复弹登录框）

**理由**：公网部署时，仅靠 Bearer token 不足以防止匿名访问 UI（HTML 页面本身需要保护）。

### 6.8 HTML 卡片 + SEARCH/REPLACE 修补

**决策**：模型可以生成完整 HTML 文档作为"卡片"。后续回复中，模型可以用 SEARCH/REPLACE 差异格式只修改卡片的一部分：

```
<<<<<<< SEARCH
<p>HP: 100</p>
=======
<p>HP: 80</p>
>>>>>>> REPLACE
```

后端 `html_patch.rs` 将差异合并到上一轮的卡片，生成完整的 `display_content`，同时 `raw_content` 保留紧凑的差异格式。需要在上下文中注入修补指令。

**理由**：让 LLM 高效编辑长文档而非每次重新生成整个 HTML 页面。

### 6.9 可选嵌入 UI

**决策**：通过 Cargo feature `embed-ui` 控制前端静态资源的 serve 方式：
- `embed-ui` 启用：`rust-embed` 将 `shirita-ui/dist` 编译进二进制，作为 SPA 服务
- `embed-ui` 禁用：开发期通过 Vite proxy 反向代理到 `:5173`（hot reload）

**理由**：生产部署时是单一二进制方便分发；开发期热重载提升体验。

### 6.10 导入适配器（适配存量工具）

**决策**：`adapters/` 模块支持从其他工具导入数据：
- **Character Card (PNG)**：从 PNG 文件读取角色卡片 JSON（`pngcard.rs`），转换为 `LoreSet` → 再转换为 Pack（`loreset_to_pack`）
- **SillyTavern 预设**：将 JSON 预设转换为本地模板 + 节点树（`stpreset_to_loreset` + `tree_to_preset`）
- **SillyTavern 世界信息**：将世界信息 JSON 转换为 definitions（`worldinfo_to_defs`）
- **通用导入**：`/import` 端点处理 .zip 便携格式和 .json 模板包

### 6.11 Provider 多源隔离

**决策**：每个 provider source（OpenAI/Anthropic/Ollama/Google 等）在存储中使用命名空间 key `provider.{source}.{field}` 保存自己的配置。settings 中记录当前活跃 source，切换时不会丢失其他源的配置。

```typescript
// 命名空间示例
provider_source = "openai"
provider.openai.base_url = "https://api.openai.com/v1"
provider.openai.api_key = "sk-..."
provider.openai.model = "gpt-4o"
// Anthropic 的配置同时保留
provider.anthropic.api_key = "sk-ant-..."
```

**理由**：用户可能在多个 provider 之间切换测试，不应在切换时重新输入其他 provider 的 API key。

### 6.12 面板即砖块（Panels as Bricks）的设计转变

**决策**：面板不再是 pack 元数据中的独立配置（`pack.meta.panel`），而是由节点树中的 `panel` 标签文件夹 + 其下的 `html`/`css` 定义砖块组成。

**架构**：
```
Folder (tag="panel", meta.name="Status", meta.caps={...})
├── Ref → html def "HP Bar"     (<div id="hp">...</div>)
├── Ref → css def "HP Style"    (#hp { color: red })
└── Ref → html def "Log"        (<div id="log">...</div>)
```

`panels.rs` 的 `collect_panels()` 遍历节点树收集所有 `panel` 标签文件夹，合并其子节点的 HTML/CSS 内容，返回 `RenderedPanel` 列表（含 name、html、css、caps、min_messages）。

**理由**：面板作为提示词结构的一部分（与条目定义解耦），让 pack 作者可以结构化地组织面板内容，复用已有的定义编辑器和节点树 UI。

### 6.13 变量即砖块（Variables as Bricks）的设计转变

**决策**：变量声明从 Pack/Template 的 `meta.decls` 上帝对象解耦为独立的 `variables` 类型定义。schema 在运行时由 `resolve_schema_from_bricks()` 从有效节点树中收集所有 `variables` 砖块合并而成。

**架构**：
```
Template 节点树:
  Folder "Character"
    ├── Ref → variables def "Stats" (meta.decls: {hp: number, level: number})
    └── Ref → char def "Alice"

Pack 节点树:
  Folder "Pack Rules"
    └── Ref → variables def "Pack Vars" (meta.decls: {score: number})
```

合并后 schema: `[hp, level, score]`

**理由**：变量声明归 pack 作者管理，无需修改模板就能携带自己的变量体系。多个 pack 的变量自动合并，与模板变量无冲突。

### 6.14 正则规则的作用域与 `is_global` 标志

**决策**：正则规则（`regex_rule`）通过 `meta.is_global` 控制作用域：
- **全局规则**（`is_global: true`）：未在任何节点树中引用，作用于所有 session
- **局部规则**（`is_global: false` 或未设置）：通过 Ref 节点引用，只作用于包含该节点的 session

`effective_regex_rules()` 从模板/会话节点树的 Ref 节点收集局部规则，再加上全局规则。`ensure_global_regex_flag()` 在启动时自动为旧的未标记规则补全 `is_global`。

**理由**：与 SillyTavern 的"全局正则 vs 角色正则"概念对应，让通用规则和特定规则共存而不混淆。

### 6.15 Per-message 身份快照

**决策**：每条消息存储创建时活跃的身份信息（`$assistant_name`/`$assistant_avatar`/`$user_name`/`$user_avatar`）在 `snapshot_state` 中。之后的模板或 pack 变更不会重写旧消息的身份。

身份数据包括：char 定义名称、persona 定义名称、session 头像、pack 的 `PackIdentity`（display_name/avatar）。pack 身份优先级高于定义名称。

**理由**：确保历史消息中的对话上下文不因模板编辑而改变。每个消息"记住"了谁在什么时候说/听到了它。

---

## 7. 设计上比较特殊 / 取巧的地方

### 7.1 Tauri 作为纯 WebView 壳，零 IPC

这是最出格的决定。大多数 Tauri 应用大量使用 `tauri::command` 调用后端，但 Shirita 完全不走这条路。所有逻辑都在 `shirita-web`（Axum）中，Tauri 只负责：
- 创建 WebView
- 管理窗口生命周期
- 启动时注入运行时配置（`__SHIRITA_RUNTIME__`）
- 崩溃时弹原生对话框

这实际上把 Tauri 降级成了一个"原生窗口管理器"，好处是前后端分离清晰，坏处是失去了 Tauri 的原生能力（文件系统、系统托盘等）。

### 7.2 节点树（PromptNode）复用三级 owner

一个 `PromptNode` 可以属于 `Template`、`Session` 或 `Pack`。Session 的节点树优先于 Template（copy-on-write 风格的物化策略）：
- 首次创建 Session 时，没有自己的节点树 → 回退到 Template 的树
- 用户编辑 Session 节点 → `materialize_nodes()` 将 Template 树深拷贝到 Session
- 此后 Session 使用自己的节点树，与 Template 完全独立

Pack 的节点树在组装时"注入"到 Content 节点的位置，按包类型（char/world/etc.）分组包裹。

### 7.3 面板（Panel）由节点树中的 `panel` 标签文件夹定义

HTML/CSS 面板不是独立创建的 UI 元素，也不是 pack 元数据的一部分，而是节点树中 `panel` 标签文件夹下的 `html`/`css` 砖块。`panels.rs` 遍历节点树收集面板，前端的 `PanelView` 使用 sandboxed iframe 渲染。

面板声明 `min_messages` 属性控制最早显示时机，`caps` 声明面板权限（write/insert/send）。

这意味着面板是"提示词结构的一部分"而非"前端插件" — 与 SillyTavern 不同的设计哲学。

### 7.4 状态变量跨越三轮：schema → seed → snapshot

变量系统是三段式的：
1. **Schema**：由 `variables` 砖块的 `meta.decls` 定义（名称、类型、初始值）
2. **Seed**：会话的 `current_state`（可覆盖初始值）
3. **Snapshot**：每条消息的 `snapshot_state`（按分支传播 + 身份数据）

三者的优先级：`snapshot > current_state > schema_initial`。这种设计让用户可以在会话层面预设变量值，同时每条分支能独立演化。

### 7.5 `std::mem::forget(dir)` 模式

测试代码中使用了 `std::mem::forget(dir)` 来阻止临时目录在测试结束时被自动删除。这是因为 `SqliteStorage` 在 `Drop` 时会关闭连接，而 sqlx 的连接池可能有异步清理操作——直接 drop tempdir 会导致 SQLite 文件在被读写时被删除。

### 7.6 重新生成的处理方式

与某些平台直接修改历史消息不同，Shirita 的 `regenerate` 采用"隐藏 + 兄弟"方式：
- 旧回复标记为 `is_hidden = true`
- 新回复作为同 parent 的兄弟创建
- 前端可以通过 `switchLeaf` 在不同回复间切换（swipe）

这种方式保留了完整的编辑历史，回退操作就是简单地切换 active leaf。

### 7.7 正则规则的双阶段处理

`regex_rule` 在**两个不同阶段**对文本进行处理：
- **Prompt 阶段**：在发送给 LLM 之前，对即将发送的消息文本进行变换（不影响存储的 `raw_content`）
- **Display 阶段**：在展示给用户之前，对 AI 回复进行变换（不影响 `raw_content`）

规则可以按目标（`ai_output` / `user_input`）和范围（`display` / `prompt` / `both`）细粒度控制。作用域通过 `is_global` 标志区分全局和局部规则。

### 7.8 纯函数式的推理标签渲染

`model/mod.rs` 中的 `render_delta` 是一个纯函数，通过 `&mut bool` 参数追踪 think 标签的开关状态。它把模型的 `reasoning_content`（DeepSeek 等推理模型特有）转换为标准的 `<think>...</think>` 包裹格式，保持与 Anthropic 的 extended thinking 相同的用户体验。流结尾的 `close_reasoning` 确保即使流在没有 content delta 的情况下结束（如只推理不回答），也不会留下未闭合的 `<think>` 标签。

### 7.9 没有专门的"设置"模型

`Settings` 被简化为键值对存储（`settings` 表），所有配置项（上下文窗口大小、max_tokens、通知开关、自定义 CSS 等）都存为 `(key, JSON value)`。后端不定义设置模型——前端直接读写 JSON blob。Provider 配置使用命名空间 key（`provider.{source}.{field}`）实现多源隔离。

### 7.10 Provider 模型列表的厂商归一化

不同 vendor 的 `/models` 端点返回结构不同，`provider_select.rs` 中的 `normalize_models_response` 将 Anthropic、Google、Cohere 等不同的响应格式统一转换为 OpenAI 的 `{ data: [{ id: ... }] }` 格式，前端无需感知 vendor 差异。

### 7.11 Generations 注册表的防竞态设计

`Generations` 不是一个简单的流注册表——它通过单调递增的 generation id 和 `AtomicU64`，保证：
1. 新生成自动废弃旧生成（调用 `AbortHandle::abort()`）
2. 只有当前活跃的 generation 能自己 deregister（`finish` 检查 `gen_id` 是否仍匹配）
3. 已停止的旧 generation 的 `finish` 不会错误地清除新 generation 的注册
4. `stop()` 和 `remove()` 分别处理协作终止和暴力删除

这种设计解决了快速连续发送/regenerate/停止操作时的竞态条件。

### 7.12 Content hashing 资产去重

`hashing.rs` 为每个资产计算 SHA-256 哈希，存储在 `assets.hash` 列中。启动时的 `ensure_asset_hashes()` 为旧数据回填。后续可在导入时检测重复文件并跳过。

### 7.13 Book UI 的堆栈导航

`BookNavigator` 采用堆栈式导航（push/pop），支持前进和返回动画过渡。导航状态（L0 列表 / L1 缩略树 / L2 完整编辑器）通过一个 `BookNavEntry[]` 栈管理，每个条目记录视图类型和上下文参数。这种设计让文库浏览体验类似于移动端的 drill-down 导航。

### 7.14 启动时 backfill 链

启动时运行一系列迁移和 backfill，保证旧数据库向前兼容：
1. `ensure_default_template` — 空数据库时创建默认模板
2. `ensure_builtin_definitions` — 创建内置定义（Web 版）
3. `ensure_templates_have_content_node` — 为旧模板添加 Content 节点
4. `ensure_global_regex_flag` — 为旧正则规则设置 `is_global` 标志
5. `ensure_asset_hashes` — 为现有资产计算哈希
