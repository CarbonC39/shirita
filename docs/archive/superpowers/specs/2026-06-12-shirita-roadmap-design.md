# Shirita 项目路线图设计 (Roadmap Design)

> 状态：已与用户确认排期哲学、框架选型与里程碑顺序。
> 本文档是**项目级路线图**。每个里程碑（M0–M9）后续各自走独立的 spec → plan → 实现循环。
> 上游参考：`tdd.md`（Technical Design Document v3）。本文档在若干处对 TDD 做了修正，见 §3。

---

## 1. 目标与范围

**Shirita** 是一款以 Rust 后端为核心的 AI 文本交互引擎，对标并替代 SillyTavern，为角色扮演、叙事创作与深度讨论提供稳定、高性能、可定制的运行环境。

- 前端仅作视图层；网络请求、状态管理、上下文工程、数据持久化全部由 Rust 后端承担。
- 双目标：**Web 自托管**（独立二进制 + Docker）与 **Tauri 桌面端**，共享同一套 `shirita-core`。
- 本路线图的首发目标平台为 **Web 服务端**，桌面端在 core 稳定后接入。

### 设计理念（沿用 TDD）
- **万物皆定义**：角色 / 物品 / 世界规则 / 提示词 / 正则清洗规则 / 角色扮演者设定，统一为 `Definition`，以类型标签区分。
- **写时复制**：对话内修改定义不污染全局库，差异存入会话的局部覆盖。
- **后端负责上下文工程**：前端不做 Token 计算、Prompt 组装或工具调用解析。
- **安全隔离**：绝不执行 LLM 直接生成的 JavaScript 或未清洗 HTML；变量修改只走预定义指令集 / 沙箱。

---

## 2. 排期哲学：垂直切片 (Vertical Slices)

尽早打通一条最小端到端链路（发消息 → LLM → SSE → 显示），之后每个里程碑往这条链路上"加厚"。

- **收益**：最早能在浏览器里真实跑起来；最早暴露集成风险；每个里程碑都交付可感知的能力。
- **代价**：早期代码会被反复扩展——通过清晰的 trait 边界（§4）控制扩展成本。

被否决的替代方案：分层瀑布（后端全做完再做前端，集成风险后置）、广度脚手架（先搭空架子，易过度设计）。

---

## 3. 对 TDD 的修正与关键技术决策

| 决策 | 内容 | 理由 |
|------|------|------|
| **Web 框架 = Axum** | TDD 写"Axum 或 Actix"，本项目定 **Axum** | tokio 原生；tower 中间件做 Bearer 鉴权链干净；SSE 一等支持 |
| **Token 计算改可插拔 + 轻量** | `TokenCounter` trait，默认实现用 `tiktoken-rs` 单一计数器，**不**按模型加载各自 BPE 词表 | Anthropic 不公开本地分词器，逐模型精算不现实；且多数 API 在超长时直接报错，无需严格精度。轻微误差用**安全边际**兜底即可 |
| **不做严格上下文裁剪** | 预算采用保守安全边际；裁剪为 best-effort（最简按优先级丢旧历史），溢出时优雅暴露 API 错误 | 上下文工程的真正价值在**自动总结**，而非逐 token 精确裁剪 |
| **流式输出在 core 内统一** | core 服务返回 `impl Stream<Item = Event>`；Web 映射为 SSE，Tauri 映射为事件系统。工具调用拦截 / 正则缓冲解析放在 core 的流处理里 | 两端共用同一套流处理逻辑，避免适配层重复实现 |
| **前端不孤立成最后一关** | 垂直切片：M1 先用极薄前端页验证 SSE；M3 提前铺设前端主体；后续里程碑各自追加自己的 UI 切片 | 尽早可用、持续可见 |

---

## 4. 架构与边界

### Cargo Workspace 分层
```
shirita/                     (Cargo workspace, git: main)
├── shirita-core/            纯 lib，不依赖任何 Web/Tauri 框架
│   ├── 对外暴露 async 服务函数（API 控制器层）
│   ├── 上下文服务：组装、Token 计算、自动总结
│   ├── 对话服务：消息流、分支操作
│   ├── 工具调用服务：指令解析、状态更新
│   └── 三个 trait 边界：
│       ├── Storage         （SQLite/sqlx 实现）
│       ├── ModelProvider   （OpenAI / Anthropic / Ollama 实现）
│       └── TokenCounter    （tiktoken-rs 默认实现）
├── shirita-web/             Axum 二进制，薄适配（REST + SSE + 静态文件 + Bearer 中间件）
└── shirita-tauri/           (M8) Tauri 二进制，#[tauri::command] 薄包装 + 事件流
```

**核心原则**：适配层（Web / Tauri）只做协议转换；所有业务逻辑在 `shirita-core`。三个 trait 把 core 与具体实现解耦，使 core 可独立单测。

### 数据模型（沿用 TDD §4，SQLite + WAL + 连接池）
- `definitions(id, type, name, content, meta)` — 类型：`char/prompt/world/item/persona/regex_rule/tool`
- `chat_sessions(id, name, avatar, override_config, current_state)`
- `messages(id, session_id, parent_id, role, raw_content, display_content, is_hidden, snapshot_state)` — 树状结构，支持分支/分叉/隐藏
- 二进制资源（头像、背景）仅存路径，不存 Blob。

---

## 5. 里程碑路线图

> 顺序已按用户反馈调整：前端提前（M3），上下文工程与工具调用推后（M5/M6）。
> 每个里程碑后续单独出 spec + 实现计划。全程 TDD 测试先行。

### M0 — 地基 / 脚手架
- `git init`，在 `main` 上工作（后期再按里程碑分支）。
- Cargo workspace：`shirita-core`(lib) + `shirita-web`(Axum bin)。
- env 配置加载：`DATABASE_PATH`、`ASSETS_DIR`、`TOKEN_SECRET`。
- sqlx + SQLite（WAL、连接池）+ 迁移框架；三张表 migration。
- `Storage` trait + SQLite 实现骨架（先支持 definitions CRUD）。
- 测试基座（temp-file / in-memory SQLite）。
- Axum 骨架：health 路由 + Bearer 鉴权中间件（MVP 校验 env 静态 token，链路必须存在）。
- **完成标志**：服务启动、迁移通过、definitions CRUD 测试往返。

### M1 — 🔪 第一条垂直切片：最小端到端对话
- `ModelProvider` trait + 一个 OpenAI 兼容**流式**适配器。
- sessions / messages CRUD（先线性，`parent_id` 字段存在但暂不分支）。
- 最简 prompt 组装（system 定义 + 历史 → 请求）。
- `send_message` 服务：返回 `Stream<Event>`；持久化 user + assistant 消息。
- Web：`POST /sessions/{id}/messages` → SSE 流；sessions/messages REST。
- `TokenCounter` trait + tiktoken 默认计数（此阶段仅用于日志/预算展示，不裁剪）。
- **极薄前端页**（最简 HTML / 单 Vue 组件）验证 SSE。
- **完成标志**：浏览器里发消息，看回复流式返回并持久化。

### M2 — 定义体系与上下文组装
- 全类型 definitions CRUD（REST）。
- 资源上传接口 + `resolve_asset_url` 抽象 + 静态文件服务。
- 完整组装流水线：挂载解析 → 局部覆盖(`override_config.local_definitions`) → `{{var}}` 渲染(取 `current_state`) → XML 封包(按分类树层级包裹标签)。
- `regex_rule` 定义生成 `display_content`（输出正则清洗）。
- **完成标志**：用角色 + 世界书开聊，组装后的 prompt 真实引用它们；资源 URL 正确解析。

### M3 — 前端主体 (Vue 3)　*【提前】*
- 三栏响应式 + 抽屉布局；shadcn-vue + Tailwind；Pinia（仅维护当前会话 ID / 流式缓存 / UI 开关，不做业务持久化）。
- **通信适配层**（HTTP + SSE），接口设计预留 Tauri-IPC 适配位。
- 安全渲染：动态 HTML 卡片用 Handlebars 等无逻辑模板引擎，**严禁 `v-html`**。
- 全局库编辑 vs 对话内编辑两种作用域（"同步至全局"按钮，二次确认 — 后端能力在 M4 落地，此处先留 UI）。
- 右栏上下文面板骨架（token 用量；变量区在 M5 填充）。
- **范围说明**：本里程碑铺设前端主体 + 覆盖当前已有能力（聊天 / 定义 / 会话）的 UI；分支、变量、总结等后续里程碑各自追加自己的 UI 切片。
- **完成标志**：可用的桌面级 Web UI，能完成定义管理与流式对话。

### M4 — 消息树：分支 / 分叉 / 隐藏 + 写时复制　*【原 M5】*
- Regenerate → 同 `parent_id` 下新增兄弟分支。
- Fork → 深拷指定节点前的全部历史到新会话（状态隔离）。
- Hide → `is_hidden` 切换，排除出上下文。
- 写时复制：对话内编辑写入 `override_config.local_definitions`，不动全局；"同步至全局"（二次确认）后端落地。
- 前端追加：分支切换 / Fork / Hide 的 UI。
- **完成标志**：分支/分叉/隐藏可用；对话内改定义不污染全局库。

### M5 — 动态变量与工具调用沙箱　*【推后，原 M4】*
- 指令集沙箱（Rust 实现）：`SET / ADD / SUB / TOGGLE / APPEND / REMOVE`，作用于 `current_state`。
- 首选：原生 Tool Calling（OpenAI/Anthropic `tool_calls` 在 core 流处理中拦截）。
- 降级：正则解析——对流式文本建缓冲区，检测 `<state_update action="..." .../>`，命中则拦截不外推，否则放行。
- 每条消息 `snapshot_state`（变量快照）；变量生命周期：初值声明于 `definitions.meta`，运行值存于 `chat_sessions.current_state`。
- 前端追加：右栏变量面板内容。
- **完成标志**：模型经工具/标签修改 hp/flag 等，状态按消息快照、按分支隔离。

### M6 — 上下文工程：自动总结 + 轻量预算　*【推后 + 简化，原 M3】*
- Token 预算 + 保守安全边际；best-effort 裁剪（超限时按优先级丢旧历史：系统指令 > 角色定义 > 最近对话 > 世界书 > 旧历史），溢出优雅暴露错误。
- **自动总结管道**（本里程碑重点）：`send_message` 后检查用量；超阈值（如 80%）用 `type=prompt` 专用定义触发总结；生成 `role=system` 摘要消息插入；被总结的早期消息 `is_hidden=true`；仅作用于当前分支。
- 多 Provider 适配器：Anthropic、Ollama。
  - *注：Provider 适配器是独立工作流（只实现 `ModelProvider` trait），可在 M1 之后任意时点并行插入，不强制绑定本里程碑。*
- 前端追加：上下文用量 / 摘要状态指示。
- **完成标志**：长对话不因溢出崩；到阈值自动生成摘要并隐藏旧消息。

### M7 — 迁移导入
- 内置 SillyTavern PNG 角色卡解析（`tEXt` / `chrm` 块）。
- 映射为 `definitions` 记录，保存原始头像路径。
- 冲突策略：默认"同名跳过 + 写日志"；UI 支持手动覆盖 / 另存为副本。
- **完成标志**：导入现有 SillyTavern 角色卡。

### M8 — Tauri 桌面端
- `shirita-tauri` bin：`#[tauri::command]` 薄包装同一套 core 服务。
- 流式响应走 Tauri 事件系统；前端启用 Tauri-IPC 适配器。
- `resolve_asset_url` 桌面分支返回 `asset://localhost/<path>`。
- 打包：Windows(.msi) / macOS(.dmg) / Linux(.AppImage)；数据目录置于系统用户目录。
- **完成标志**：共享同一 core 的原生桌面应用。

### M9 — 部署
- Web：前端静态文件内嵌二进制；Dockerfile；`/data` 与 `/data/assets` 卷映射；env 配置（`TOKEN_SECRET` / `DATABASE_PATH` / `ASSETS_DIR`）。
- 桌面：M8 产出的安装包。
- **完成标志**：可交付的 Docker 镜像 + 桌面安装包。

---

## 6. 依赖与并行

```
M0 → M1 → M2 ─┬→ M3 (前端主体)
              ├→ M4 (消息树/写时复制)
              ├→ M5 (变量/工具调用)
              └→ M6 (自动总结/裁剪)
M3/M4/M5/M6 在 M2 后大致可并行，但都会触及 send_message 与前端，需协调；
                 M7 (导入) 依赖 M2；
                 M8 (Tauri) 依赖 core 稳定 + M3 前端；
                 M9 (部署)  收尾。
Provider 适配器（Anthropic/Ollama）为独立横向工作流，M1 后可随时并行。
```

---

## 7. 贯穿全程的横切关注点

- **TDD 测试先行**：core 的服务函数与 trait 边界优先用单测覆盖；适配层用集成测试。
- **安全隔离**：绝不执行 LLM 生成的 JS / 未清洗 HTML；渲染走无逻辑模板引擎；变量修改只走指令集沙箱。
- **trait 解耦**：`Storage` / `ModelProvider` / `TokenCounter` 三个边界是 core 可测试、可扩展、双端共享的基石。
- **鉴权链**：Bearer 中间件从 M0 起就在链路中（MVP 阶段函数体可简单，但链路必须存在）。

---

## 8. 下一步

本路线图确认后，从 **M0** 开始为其单独编写实现计划（writing-plans），进入实现循环。
