# Shirita Technical Design Document v3

## 1. 概述

**Shirita** 是一款下一代 AI 文本交互引擎，目标替代传统纯前端方案（如 SillyTavern），为角色扮演、叙事创作和深度讨论提供稳定、高性能、可定制的运行环境。本项目基本上是SillyTavern的代替，有模糊的地方可参考SillyTavern的设计。

- **核心痛点**：浏览器生命周期导致长连接断联，前端状态机处理海量上下文时性能低下，数据存储分散易丢失。
- **工程解法**：前端仅作为视图层，由 Rust 后端承载网络请求、状态管理与数据持久化。采用 Tauri 桌面端与独立 Web 服务端双目标架构，共享同一套核心逻辑。
- **设计理念**：万物皆定义（Everything is a Definition）、写时复制（Copy-on-Write）防污染、精确 Token 统筹。

------

## 2. 核心设计原则

| 原则                   | 含义                                                         |
| ---------------------- | ------------------------------------------------------------ |
| **万物皆定义**         | 角色、物品、世界规则、提示词、正则清洗规则、角色扮演者设定等，统一为 Definition，通过类型标签区分。 |
| **写时复制**           | 在对话中修改定义不会污染全局库，差异存入会话的局部覆盖配置中。 |
| **后端负责上下文工程** | 前端不执行任何 Token 计算、Prompt 组装或工具调用解析。       |
| **协议双模但逻辑统一** | 桌面端通过 Tauri IPC 通信，Web 端通过 HTTP + SSE 通信，调用同一个核心函数。 |
| **安全隔离**           | 绝对不执行由 LLM 直接生成的 JavaScript 或未经清洗的 HTML；所有变量修改必须经过预定义指令集或沙箱。 |

------

## 3. 双目标架构

### 3.1 分层拓扑

text

```
┌────────────────────────────────────────────────────┐
│                 Vue 3 前端 (SPA)                    │
│  仅维护 UI 状态 (Pinia), 通过适配层调用后端          │
└──────────────────────┬─────────────────────────────┘
                       │
┌──────────────────────┴─────────────────────────────┐
│                通信适配层                           │
├──────────────────────┬─────────────────────────────┤
│ Tauri IPC (invoke)   │  HTTP REST + SSE + Static   │
│                      │  (Axum 或 Actix-web)         │
└──────────────────────┴─────────────────────────────┘
                       │
┌──────────────────────┴─────────────────────────────┐
│                 Rust Core Library (shirita-core)   │
│ ├─ API 控制器：参数校验，调用服务                   │
│ ├─ 上下文服务：AST 编译，Token 计算，自动总结       │
│ ├─ 对话服务：消息流管理，分支操作                   │
│ ├─ 工具调用服务：指令解析，状态更新                 │
│ ├─ 模型适配层 (Trait)：OpenAI，Anthropic，Ollama    │
│ └─ 存储抽象层 (Trait)：SQLite (sqlx) + 文件管理     │
└────────────────────────────────────────────────────┘
```



### 3.2 双入口编译

- **桌面端 (Tauri)**：所有后端命令通过 `#[tauri::command]` 暴露，二进制内嵌前端静态资源。
- **Web 服务端**：编译为独立二进制文件，启动时开启 HTTP 服务器，同时提供 API 和静态文件服务。

------

## 4. 数据模型

使用 SQLite 存储（WAL 模式开启，连接池），二进制文件（头像、背景）仅存路径，不存 Blob。

### 4.1 定义表 `definitions`

| 字段      | 类型        | 描述                                                         |
| --------- | ----------- | ------------------------------------------------------------ |
| `id`      | TEXT (UUID) | 主键                                                         |
| `type`    | TEXT        | `char`, `prompt`, `world`, `item`, `persona`, `regex_rule`, `tool` |
| `name`    | TEXT        | 显示名称                                                     |
| `content` | TEXT        | 原始 XML 文本块，可含 `{{variable}}` 占位                    |
| `meta`    | TEXT (JSON) | 扩展属性：默认头像路径、HTML 模板、初始变量、正则模式等      |

### 4.2 对话会话表 `chat_sessions`

| 字段              | 类型        | 描述                           |
| ----------------- | ----------- | ------------------------------ |
| `id`              | TEXT (UUID) | 主键                           |
| `name`            | TEXT        | 对话名                         |
| `avatar`          | TEXT        | 对话专属头像路径               |
| `override_config` | TEXT (JSON) | 局部覆盖定义及设置             |
| `current_state`   | TEXT (JSON) | 动态变量当前值，如 `{"hp":80}` |

- `override_config` 结构示例：

json

```
{
  "local_definitions": {
    "uuid-of-global-def": "<override content>"
  },
  "template_overrides": { ... }
}
```



### 4.3 消息表 `messages`

| 字段              | 类型        | 描述                                |
| ----------------- | ----------- | ----------------------------------- |
| `id`              | TEXT (UUID) | 主键                                |
| `session_id`      | TEXT        | 外键                                |
| `parent_id`       | TEXT        | 父节点 ID，根节点为 NULL            |
| `role`            | TEXT        | `user`, `assistant`, `system`       |
| `raw_content`     | TEXT        | API 返回原始文本，含 Tool Call 标记 |
| `display_content` | TEXT        | 经正则过滤后用于展示的文本（可选）  |
| `is_hidden`       | INTEGER     | 布尔，是否在上下文中隐藏            |
| `snapshot_state`  | TEXT (JSON) | 该消息产生时的变量快照              |

- **树状结构**：支持分支（Regenerate）、分叉（Fork）和隐藏（Hide）。
- **Fork** 深度复制指定节点前的所有历史消息到新会话，保证状态隔离。

------

## 5. 通信协议

### 5.1 桌面端 (Tauri)

- 所有后端调用使用 `invoke(cmd, args)`，流式响应通过 Tauri 事件系统推送。

### 5.2 Web 服务端

- **API**：RESTful 风格，JSON 格式。
- **聊天流**：**HTTP SSE (Server-Sent Events)**，单向推送模型回复文本块。
- **文件服务**：静态文件路由提供头像、背景等用户资源。
- **鉴权**：所有 API 路由均通过 Bearer Token 中间件（MVP 阶段由启动参数设置静态 Token，函数留空但链必须存在）。

------

## 6. 资源文件管理

- **统一抽象函数**：`fn resolve_asset_url(local_path: &str) -> String`
  - Tauri 入口：返回 `asset://localhost/<path>` 格式。
  - Web 入口：返回相对 URL 如 `/assets/<path>`。
- 上传接口返回统一相对路径，前端使用时调用该函数转换。

------

## 7. 上下文工程

### 7.1 Prompt 组装流水线 (Rust)

1. **挂载解析**：根据会话的模板及用户选择，拉取对应 `definitions`。
2. **局部覆盖**：检查 `override_config.local_definitions`，若存在则替换同 ID 全局定义。
3. **变量渲染**：用 `current_state` 替换文本中的 `{{var}}`。
4. **XML 封包**：按分类树层级包裹标签，如 `<world_rules>...</world_rules>`。

### 7.2 精确 Token 计算

- 使用 `tokenizers` 库加载对应模型的 BPE 词表。
- 词表加载过程异步执行，使用 `OnceCell` 或 `Lazy` 缓存，避免阻塞启动。
- 每次组装后精确计数；若超出上下文窗口，按优先级丢弃（系统指令 > 角色定义 > 最近对话 > 世界书 > 旧历史）。

### 7.3 自动总结管道

- 在 `send_message` 完成后同步检查 Token 使用率；若超过预设阈值（如 80%），触发总结任务。
- 使用 `type="prompt"` 的专用定义作为总结提示词。
- 生成摘要后插入为 `role="system"` 的新消息，被总结的早期消息 `is_hidden` 置为 true。
- 仅在当前对话分支上操作，不影响其他分支。

------

## 8. 动态变量与工具调用

### 8.1 变量生命周期

- 初始值可声明在 `definitions.meta` 中。
- 运行时值完全存于 `chat_sessions.current_state`，不修改定义表。
- 变量不是定义，而是状态。

### 8.2 指令集（沙箱）

允许的动词（由 Rust 后端实现）：

- `SET`（覆盖值）
- `ADD` / `SUB`（数字）
- `TOGGLE`（布尔）
- `APPEND` / `REMOVE`（数组）

### 8.3 触发方式

1. **首选：原生 Tool Calling**
   使用 OpenAI/Anthropic 标准，`tool_calls` 独立于文本流，后端拦截处理。
2. **降级方案：正则解析**
   对流式文本建立缓冲区，检测 `<state_update action="..." .../>`。匹配成功则拦截，不推送到前端；否则正常放行。

------

## 9. UI/UX 设计约束

- **布局**：响应式三栏/抽屉，左侧导航，中栏聊天区，右侧上下文面板。
- **组件库**：shadcn-vue + Tailwind CSS。
- **前端状态 (Pinia)**：仅维护当前会话 ID、流式文本缓存、UI 开关。不做任何业务数据持久化。
- **操作作用域**：在全局库编辑 → 保存至全局；在对话内编辑 → 存入局部覆盖，并提供“同步至全局”按钮（二次确认）。
- **安全渲染**：动态 HTML 卡片必须使用 Handlebars 等无逻辑模板引擎，严禁 `v-html`。

------

## 10. 迁移导入

- 内置 SillyTavern PNG 角色卡解析器，读取 `tEXt`/`chrm` 块。
- 自动映射为 `definitions` 记录，保存原始头像路径。
- **冲突策略**：默认“同名跳过，写入日志”；用户可在 UI 手动覆盖或另存为副本。

------

## 11. 部署策略

### 11.1 桌面版 (Tauri)

- 安装程序，数据目录位于系统用户目录（`AppData` 或 `~/.config/shirita/`）。
- 打包为 Windows (.msi)、macOS (.dmg)、Linux (.AppImage)。

### 11.2 Web 自托管版

- 编译为二进制，包含嵌入式前端静态文件。
- 提供 Docker 部署方案，将数据库目录 `/data` 和资源目录 `/data/assets` 映射到宿主机。
- 环境变量配置：`TOKEN_SECRET`、`DATABASE_PATH`、`ASSETS_DIR`。