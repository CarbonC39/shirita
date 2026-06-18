# 聊天身份（头像 + 显示名）与会话标题重命名 设计 spec

**日期:** 2026-06-17 · **状态:** 待实现（设计已与用户敲定）

来源：前端 review 的 #5–#8（头像与显示名不一致 / 无法展示）+ #4 的瘦身版（会话标题重命名）。

## 背景 / 问题

当前"身份"散落在多处、无单一真相，导致聊天 UI 无法正确展示头像与名称：

- 角色的规范名其实在 **模板/`char` 定义** 里（ST 导入把 `template.name` 与主 `char` 定义名都设为角色名）。
- `session.name` 只是用户在新建流程里输入的**聊天标题**，可与角色不同。
- `session.avatar` 由新建流程写入（本批已接通），用于首页列表缩略图。
- `$avatar` 是分支快照状态变量（模型可经 `<state_update>` 改）。

结果：聊天头部显示通用 "Chat"（#5）；气泡永远是纯色圆圈 + 写死的 "Assistant"/"You"（#6）；头部头像只读 `$avatar` 不读 `session.avatar`（#7）；用户 persona 没有头像/名称概念进入 UI（#8）。

## 目标

1. 助手与用户在聊天中显示**真实的名称 + 头像**（头部、气泡、flat 抬头一致）。
2. 头像取自既有的**素材库**（`AssetPicker`）。
3. 用户可重命名会话标题（#4 瘦身：仅标题，不做完整聚焦页）。

## 非目标（推迟）

- 完整的 `/chat/:id/options` 聚焦页。
- Provider 原生 reasoning 字段捕获；Composer 附件（已各自有计划/记录）。

## 核心决策：定义默认 + 分支变量覆盖

身份 = **定义提供的静态默认**，叠加 **分支级状态变量的可选覆盖**（与现有 `$avatar` 完全对称）。

| | 显示名 | 头像 |
|---|---|---|
| **助手** | char 定义的 `name`（可被 `$assistant_name` 覆盖） | `session.avatar`（可被 `$avatar` 覆盖） |
| **用户** | persona 定义的 `name` | persona 定义的 `meta.avatar` |

精确优先级：

- 助手名：`$assistant_name`（分支变量，非空时）› char 定义名 › i18n `chat.assistant`
- 助手头像：`$avatar`（分支变量，非空时）› `session.avatar` › 占位圆圈
- 用户名：persona 定义名 › i18n `chat.you`
- 用户头像：persona 定义 `meta.avatar` › 无

**只有两个头像需要用户维护**：`session.avatar`（助手/角色，本会话级）与 persona 定义的 `meta.avatar`（用户）。**不**给 `char` 定义加头像字段（合并进 `session.avatar`）。

为什么助手身份允许变量覆盖、用户 persona 不允许：助手身份是会随剧情演变的"场景状态"（`$avatar` 已如此），用户 persona 是用户自选的静态配置——模型不应改写用户头像/名。

## 数据模型

- 在 **`persona` 定义的 `meta`** 上增加 `avatar` 字段：素材相对路径字符串（与 `session.avatar` / `$avatar` 同形）。`meta` 是自由 JSON，**无需迁移**。
- `char` 定义**不**加头像字段（助手头像走 `session.avatar`）。
- 新增系统变量 **`$assistant_name`**（`state.rs::system_variables()`，初值空串，scope=system），与 `$avatar` 并列。

## 身份解析规则（后端）

新增只读端点 `GET /api/sessions/:id/identity`，返回**定义/会话级**的解析结果（不含分支变量覆盖，覆盖在前端叠加）：

```json
{ "assistant": { "name": string|null, "avatar": string|null },
  "user":      { "name": string|null, "avatar": string|null } }
```

- `assistant.name`：在会话 effective 节点树里，被启用 ref 引用的 `char` 定义中，按下序选"身份定义"取其 `name`；无则 `null`。
  1. `name == template.name` 的那个；
  2. 否则树序中第一个 `char`；
  3. 否则 `null`。
- `assistant.avatar`：直接取 `session.avatar`（无则 `null`）。
- `user.name` / `user.avatar`：对 `persona` 定义用同样的"身份定义"选取规则（实际几乎总是树序第一个 persona，因 persona 名不会等于 `template.name`），取其 `name` 与 `meta.avatar`；无则 `null`。

边界：无模板的自由会话 → 无 char/persona 定义 → `name`/`avatar` 皆 `null` → 前端兜底。

## 分支变量覆盖（前端，零成本）

`ChatView` 已随分支拉取 `sessionState.values`（含 `$avatar`）。在此基础上叠加覆盖、避免重拉 identity：

- 助手有效头像 = `values['$avatar']`（非空）否则 `identity.assistant.avatar`。
- 助手有效名 = `values['$assistant_name']`（非空）否则 `identity.assistant.name` 否则 i18n。
- 覆盖只作用于助手；用户 persona 只取定义解析结果。

即：身份端点给"定义/会话级"静态结果（每会话拉一次）；`$avatar` / `$assistant_name` 覆盖用既有 state 数据在前端应用，随分支自动更新。

## 前端改动

- **`DefinitionEditor`**：当 `definition.type === 'persona'` 时显示一个 `AssetPicker`（圆形预览），读写 `meta.avatar`。其他类型不显示头像选择器。
- **`ChatView`**：加载会话时 `getSessionIdentity(id)`；结合 `sessionState.values` 的 `$avatar` / `$assistant_name` 覆盖算出有效身份，向下传给消息列表与头部。
- **身份向下传递**：`ChatView` → `MessageList` → `MessageItem` 传 `identity = { assistant: {name, avatar}, user: {name, avatar} }`；`MessageItem` 按 `message.role` 取对应名/头像。
- **`MessageItem`**：渲染真实头像（`<img>`，无则回退现有纯色圆圈占位），名称用解析出的名（兜底 i18n）。bubble 与 flat 两种模式都接通。
- **头部（`ChatView`）**：显示**角色名 + 头像**（替换通用标题）。`session.name`（聊天标题）保留为首页列表标题。

## 会话标题重命名（#4 瘦身）

- 新增 `PATCH /api/sessions/:id`，body `{ name?: string, avatar?: string|null }`，返回更新后的 `Session`。需要存储层支持更新 `session.name` / `session.avatar`。
- UI：首页列表 `⋯` 菜单新增 **Rename**（就地编辑 `session.name`），与既有 Duplicate/Export/Delete 并列。聊天头部展示的是**角色名**（身份），与列表标题区分。
- 该端点也使"改助手头像 = 改 `session.avatar`"成为可能（后续可挂到 ⋯ 菜单/头部，本批不强制）。

## 测试（TDD，逐层）

- **core**：`system_variables()` 含 `$assistant_name`；身份解析——name-match 优先、树序回退、无定义回退；persona 取 `meta.avatar`。
- **web**：`GET /sessions/:id/identity`（导入卡 → 正确角色名、`session.avatar`、persona 名/头像；无模板 → null）；`PATCH /sessions/:id` 改名/改头像 round-trip。
- **ui**：`DefinitionEditor` 仅对 persona 显示头像选择器、写 `meta.avatar`；`MessageItem` 按 role 渲染名/头像 + 回退；`ChatView` 头部显示角色名；`$avatar` / `$assistant_name` 覆盖优先于定义解析。

## 涉及文件（预估）

- `shirita-core`：`state.rs`（新增 `$assistant_name` 系统变量）+ 身份解析逻辑（新模块或 assembly）+ 单测。
- `shirita-web`：`routes/sessions.rs`（identity GET、PATCH）+ storage 改 name/avatar 方法 + 集成测试。
- `shirita-ui`：`api/client.ts`（getSessionIdentity、patchSession）、`api/types.ts`（Identity 类型）、`DefinitionEditor.vue`、`ChatView.vue`、`MessageList.vue`、`MessageItem.vue`、`HomeView.vue` / `ChatCard.vue`（Rename）、相应 `*.test.ts`。

## 兼容性

- 无 DB 迁移（persona 头像入 `meta`；`$assistant_name` 是代码内系统变量声明，旧快照按 schema 增长回填初值）。
- 旧会话/无身份定义：走兜底（i18n 名 + 纯色圆圈 / `session.avatar`），不破坏现状。
