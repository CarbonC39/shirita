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
| `models/` | 领域模型：`Definition`（条目）、`Session`（会话）、`Message`（消息）、`PromptNode`（节点树）、`Template`（模板）、`Pack`（内容包）、`Summary`（摘要）、`Asset`（媒体资产）、`DefType`（容器类型注册表） |
| `storage/` | `Storage` trait + `SqliteStorage` 实现（sqlx + SQLite）。CRUD + 事务性导入/复制/物化 |
| `model/` | `ModelProvider` trait + 适配器：`OpenAiProvider`（兼容 OpenAI）、`AnthropicProvider`（Claude Messages API）、`EchoProvider`（测试用） |
| `conversation.rs` | 对话服务：`send_message` 和 `regenerate` 的完整流程（见 §5）。`StopToken`/`StopHandle` 协作终止 |
| `assembly.rs` | 提示词组装引擎：遍历节点树 → 触发激活（constant/keyword/random）→ 渲染变量 → XML 包裹 → 构建 `ChatMessage` 数组。含正则规则引擎（`apply_regex_rules`）和面板状态捕获（`capture_panel_updates`） |
| `state.rs` | 变量声明系统：schema 定义、值折叠（`<state_update>` → 快照）、渲染/剥离 |
| `budget.rs` | Token 预算管理：上下文窗口裁剪（`trim_history`） |
| `summarize.rs` | 滚动摘要生成：对话超过窗口后的自动摘要 |
| `tokenizer/` | `TokenCounter` trait + `TiktokenCounter`（tiktoken-rs 包装） |
| `adapters/` | 导入适配器：Character Card（PNG）、SillyTavern 预设/世界信息 |
| `portable.rs` | 便携格式：导出/导入为 .zip（含定义、节点树、资产） |
| `attachments.rs` | 附件处理：将 asset ID 解析为图片 data URL |
| `html_patch.rs` | HTML 卡片修补：SEARCH/REPLACE 差异格式的合并与重建 |
| `keyword.rs` | Aho-Corasick 多关键词匹配引擎 |
| `panels.rs` | 面板解析：从节点树提取 HTML/CSS 面板 |
| `tree.rs` | 消息树遍历：`active_path` 从消息森林中提取当前分支 |
| `config.rs` | 环境变量配置读取 |
| `seed.rs` | 首次启动种子数据（默认模板、内置定义） |
| `identity.rs` | 用户/助手身份信息计算 |
| `pngcard.rs` | PNG Character Card 提取 |
| `error.rs` | 统一错误类型 |

### 2.2 `shirita-web` — HTTP 服务层

基于 Axum 0.8 的 REST + SSE 服务。

| 模块 | 职责 |
|---|---|
| `main.rs` | 独立 Web 入口：绑定地址 → 初始化 DB/Provider → `axum::serve` |
| `lib.rs` | `app()` 路由组装 + `app_with_cors()`（Tauri 用 CORS 包装） |
| `routes/` | 按资源划分的 17 个路由模块：`sessions`、`chat`、`messages`、`definitions`、`templates`、`prompt_nodes`、`packs`、`types`、`settings`、`assets`、`provider`、`export`、`import_export`、`variables`、`local_overrides`、`regex_rules`、`health`、`index`、`ping` |
| `auth.rs` | 双层认证：`require_bearer`（API 层，Bearer token）+ `require_basic`（最外层，可选 HTTP Basic Auth，用于公网部署） |
| `state.rs` | `AppState`：共享的 `Storage`、`ModelProvider`、`Config`、`TokenCounter`、`Generations` 注册表 |
| `generations.rs` | 生成注册表：追踪每个 session 的进行中 SSE 流，支持协作终止 |
| `provider_select.rs` | 运行时 Provider 解析：settings 中的配置优先，否则 fallback 到环境变量 |
| `embed.rs` | 可选嵌入 UI（`embed-ui` feature）：编译时将前端 dist 嵌入二进制 |

**路由结构**：

```
/health                          — 健康检查（无认证）
/api/ping                        — Bearer
/api/sessions[/{id}]             — 会话 CRUD + 复制/导出/排序
/api/sessions/{id}/messages      — 消息列表 + SSE 发送
/api/sessions/{id}/messages/{id} — 编辑/删除消息
/api/sessions/{id}/abort         — 协作终止生成
/api/definitions[/{id}]          — 条目 CRUD + 导出
/api/templates[/{id}]            — 模板 CRUD + 复制/导出
/api/templates/{id}/nodes        — 模板节点树 CRUD
/api/packs[/{id}]                — 包 CRUD + 复制/导出
/api/types                       — 容器类型注册表
/api/settings                    — 键值设置
/api/assets                      — 媒体库上传/列表/删除
/api/provider/*                  — Provider 连接测试/模型列表
/api/import/*                    — 导入（WorldInfo/CharCard/通用）
/api/sessions/{id}/panels        — 面板
/api/sessions/{id}/state         — 变量状态
/api/sessions/{id}/local-*       — 会话级局部覆盖
/api/sessions/{id}/materialize-* — 节点物化
/assets/*                        — 静态文件 ServeDir
/                                — SPA index.html（Dev: proxy, Prod: embed）
```

### 2.3 `shirita-tauri` — Tauri 桌面壳

| 文件 | 职责 |
|---|---|
| `main.rs` | 启动 embedded Axum 服务器 → 创建 WebView 指向该服务器 → 注入 `window.__SHIRITA_RUNTIME__` |
| `tauri.conf.json` | Tauri v2 配置：前端 dist 路径、CSP、窗口设置 |

**核心模式**：Tauri 版不调用任何 Tauri 命令（IPC）。它启动一个 `shirita-web` 的 Axum 服务器绑定到 `127.0.0.1:0`（随机端口），然后创建 WebView 加载 `http://127.0.0.1:{port}`。前后端通信完全走标准 HTTP。

### 2.4 `shirita-ui` — Vue 3 前端 SPA

| 目录 | 职责 |
|---|---|
| `src/views/` | 页面级组件：`HomeView`（会话列表）、`ChatView`（聊天）、`NewChatView`（新建）、`BookView`（文库/模板编辑器）、`SettingsView` |
| `src/components/` | 可复用组件：`AppShell`（布局壳）、`Composer`（输入框）、`MessageList/Item/Content`（消息展示）、`PromptTree`（节点树编辑器）、`PackEditor`、`DefinitionEditor`、`NodePicker`、`EntityPicker`、`VariablesEditor`、各 book 组件（`BookNavigator`、`DefinitionView`、`SessionTemplateRoot`） |
| `src/stores/` | Pinia 状态管理：`chat`、`sessions`、`library`（定义/模板/包）、`media`、`settings`、`ui` |
| `src/api/` | HTTP 客户端：`client.ts`（所有 API 调用 + SSE 解析）、`types.ts`（TypeScript 类型定义）、`modelCatalog.ts` |
| `src/composables/` | Vue composables：`useTheme`、`useCustomCss` |
| `src/utils/` | 工具函数：`tree.ts`（消息树）、`tokens.ts`、`thinking.ts`（think 标签渲染）、`markdown.ts`、`notify.ts`、`panel.ts`、`regexRule.ts`、`providerKeys.ts`、`clone.ts` |
| `src/locales/` | i18n：`en`、`zh-Hans`、`zh-Hant`、`ja` + `resolve.ts`（运行时语言选择） |
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
| 种子数据 | `ensure_builtin_definitions`（内置定义 + 默认模板） | 不调用 `ensure_builtin_definitions`（只有默认模板） |
| 数据库位置 | 环境变量 `DATABASE_PATH`（默认 `shirita.db`） | `app_data_dir / shirita.db`（OS 标准数据目录） |
| 崩溃处理 | 终端打印错误 | 弹出原生错误对话框（`tauri-plugin-dialog`） |
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
          ├── 3. 解析 schema + 折叠 branch state
          ├── 4. 存储 user message（含 snapshot_state）
          ├── 5. resolve_session_schema(): 模板节点树 + 已挂载 pack
          ├── 6. assemble_request():
          │   ├── effective_nodes(): session 自有节点优先，否则用 template
          │   ├── mounted_pack_trees(): 按挂载顺序加载 pack 节点树
          │   ├── assemble_from_nodes_with_packs():
          │   │   ├── 构造 Entry（trigger/scan/recursive）
          │   │   ├── activate(): constant/keyword/random 触发
          │   │   ├── render_vars(): {{var}} → state 值
          │   │   ├── strip_comments(): {{// ... }}
          │   │   ├── maybe_wrap(): XML 标签包裹
          │   │   └── build_chat_messages(): segment → ChatMessage[]
          │   ├── 注入 protocol 定义（state_update / html_patch）
          │   ├── effective_regex_rules(): 全局 + 作用域规则
          │   └── trim_history(): 上下文窗口裁剪
          ├── 7. provider.stream_chat(req) → SSE delta stream
          ├── 8. 流式累积 full text，逐 delta yield
          ├── 9. 折叠 state_update 到 snapshot_state
          ├── 10. capture_panel_updates() 从正则提取变量
          ├── 11. strip_state_tags() 清理显示文本
          ├── 12. resolve_display(): 可选 HTML 卡片重建
          ├── 13. 存储 assistant message + 更新 active leaf
          └── 14. yield Done / Stopped / Error
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
  ensure_asset_hashes()
  provider_from_env()        ← OPENAI_API_KEY 等
  axum::serve(listener, app(state))

【Tauri 版】
  Config::new(db, assets, random_token)
  apply_provider_env()       ← 环境变量覆盖
  SqliteStorage::connect() + run_migrations()
  ensure_default_template()
  ensure_templates_have_content_node()
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
├── messages           — 消息（树状结构：parent_id 自引用 FK）
│   ├── raw_content    — 原始回复（含 <state_update> 标签）
│   ├── display_content — 可选显示文本（HTML 重建后 / 标签剥离后）
│   └── snapshot_state  — 分支状态快照（每一轮的变量值快照）
├── definitions        — 条目（char/world/prompt/regex_rule...）
├── prompt_nodes       — 节点树（Template/Session/Pack 三级 owner）
├── templates          — 模板
├── packs              — 内容包
├── def_types          — 容器类型注册表
├── assets             — 媒体库
├── summaries          — 滚动摘要
└── settings           — 键值设置
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

### 6.4 变量系统：`<state_update>` 标签 + 分支快照

**决策**：
- 模型在其回复中嵌入 `<state_update action="SET|SUB|ADD" key="hp" value="5"/>` 标签
- 服务端解析标签，折叠到 `snapshot_state`，从 `display_content` 中剥离
- 每条消息记录其分支的 `snapshot_state`（作为会话当前状态的"快照"）
- 当用户从分支 A 切换到分支 B 时，使用 B 的最新 snapshot 作为上下文

**状态优先级**：`schema 初始值` < `会话 current_state` < `叶子快照`，下层覆盖上层。

**理由**：让 LLM 能通过结构化标签管理游戏状态（HP、位置等），同时保持显示文本的干净。分支快照确保切换分支时状态正确还原。

### 6.5 消息树 + 分支 + 覆盖（Swipe 风格）

**决策**：消息以树状结构存储（`parent_id` 自引用 FK），`active_leaf_id` 决定当前显示的分支。重新生成（regenerate）时：
1. 将旧 assistant 消息标记为 `is_hidden`
2. 创建同 parent 的兄弟消息
3. 更新 active_leaf 指向新消息

前端通过 `activePath()` 从消息森林中提取当前分支（从根到 active leaf 的路径）。

**理由**：SillyTavern 风格的 "swipe"（滑动切换回复）用户体验。

### 6.6 协作停止（Cooperative Stop）

**决策**：停止按钮不 kill 请求，而是：
1. `POST /api/abort` → `watch::Sender::send(true)`
2. SSE 流中的 `tokio::select!` 在下一个 delta 到达前检测到停信号
3. 将已累积的部分文本持久化为 assistant message
4. 前端 reload 显示部分回复

**理由**：用户不想丢失模型已经生成的内容，只想打断继续生成。

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
- **Character Card (PNG)**：从 PNG 文件读取角色卡片 JSON（`pngcard.rs`）
- **SillyTavern 预设**：将 JSON 预设转换为本地模板 + 节点树
- **SillyTavern 世界信息**：将世界信息 JSON 转换为 definitions
- **通用导入**：`/import` 端点处理 .zip 便携格式

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
- 用户编辑 Session 节点 → `materialize_session_nodes()` 将 Template 树深拷贝到 Session
- 此后 Session 使用自己的节点树，与 Template 完全独立

Pack 的节点树在组装时"注入"到 Content 节点的位置，按包类型（char/world/etc.）分组包裹。

### 7.3 面板（Panel）由模板节点树定义，而非独立配置

HTML/CSS 面板不是独立创建的 UI 元素，而是模板节点树中的 `html`/`css` 类型条目。`panels.rs` 遍历节点树收集面板，然后由前端渲染为可交互的 HTML 片段。面板还可以声明权限（`PanelCaps: write/insert/send`），后端 `capture_panel_updates` 从 LLM 回复中提取变量变化。

这就意味着面板是"提示词结构的一部分"而非"前端插件"，这是与 SillyTavern 不同的设计哲学。

### 7.4 状态变量跨越三轮：schema → seed → snapshot

变量系统是三段式的：
1. **Schema**：由 `variables` 条目的 `meta.decls` 定义（名称、类型、初始值）  
2. **Seed**：会话的 `current_state`（可覆盖初始值）
3. **Snapshot**：每条消息的 `snapshot_state`（按分支传播）

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

规则可以按目标（`ai_output` / `user_input`）和范围（`display` / `prompt` / `both`）细粒度控制。这使得 SillyTavern 兼容的"发前替换、显示替换"都能在统一规则引擎下实现。

### 7.8 纯函数式的推理标签渲染

`model/mod.rs` 中的 `render_delta` 是一个纯函数，通过 `&mut bool` 参数追踪 think 标签的开关状态。它把模型的 `reasoning_content`（DeepSeek 等推理模型特有）转换为标准的 `<think>...</think>` 包裹格式，保持与 Anthropic 的 extended thinking 相同的用户体验。流结尾的 `close_reasoning` 确保即使流在没有 content delta 的情况下结束（如只推理不回答），也不会留下未闭合的 `<think>` 标签。

### 7.9 没有专门的"设置"模型

`Settings` 被简化为键值对存储（`settings` 表），所有配置项（上下文窗口大小、max_tokens、通知开关、自定义 CSS 等）都存为 `(key, JSON value)`。后端不定义设置模型——前端直接读写 JSON blob。这降低了耦合但也意味着没有设置校验。
