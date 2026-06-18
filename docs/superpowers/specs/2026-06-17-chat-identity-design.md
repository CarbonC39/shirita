# 聊天身份（头像 + 显示名）与会话标题重命名 设计 spec

**日期:** 2026-06-17 · **状态:** 待实现（设计已与用户敲定）

来源：前端 review 的 #5–#8（头像与显示名不一致 / 无法展示）+ #4 的瘦身版（会话标题重命名）。

## 背景 / 问题

当前"身份"散落在多处、无单一真相，导致聊天 UI 无法正确展示头像与名称：

- 角色的规范名其实在 **模板/`char` 定义** 里（ST 导入把 `template.name` 与主 `char` 定义名都设为角色名）。
- `session.name` 只是用户在新建流程里输入的**聊天标题**，可与角色不同。
- `session.avatar` 仅用于首页列表缩略图（本批已接通新建流程写入）。
- `$avatar` 是分支快照状态变量（模型可经 `<state_update>` 改）。

结果：聊天头部显示通用 "Chat"（#5）；气泡永远是纯色圆圈 + 写死的 "Assistant"/"You"（#6）；头部头像只读 `$avatar` 不读 `session.avatar`（#7）；用户 persona 没有头像/名称概念进入 UI（#8）。

## 目标

1. 助手与用户在聊天中显示**真实的名称 + 头像**（头部、气泡、flat 抬头一致）。
2. 头像取自既有的**素材库**（`AssetPicker`）。
3. 用户可重命名会话标题（#4 瘦身：仅标题，不做完整聚焦页）。

## 非目标（推迟）

- 完整的 `/chat/:id/options` 聚焦页。
- 把助手名做成变量（`$name`）——本设计选择**基于定义**。
- Provider 原生 reasoning 字段捕获；Composer 附件（已各自有计划/记录）。

## 核心决策：基于定义的身份模型

| | 显示名 | 头像 |
|---|---|---|
| **助手** | `char` 定义的 `name` | `char` 定义的 `meta.avatar` |
| **用户** | `persona` 定义的 `name` | `persona` 定义的 `meta.avatar` |

不同机制各取所需：助手/用户身份都是**作者编写的静态身份**，编辑入口在 Book（定义编辑器）；`$avatar` 仍作为助手头像的**分支级动态覆盖**保留。

## 数据模型

- 在 `char` / `persona` **定义的 `meta`** 上增加 `avatar` 字段：素材相对路径字符串（与 `session.avatar` / `$avatar` 同形）。`meta` 是自由 JSON，**无需迁移**。
- 其他类型（world/prompt 等）不引入头像。

## 身份解析规则（后端）

新增只读端点 `GET /api/sessions/:id/identity`，返回：

```json
{ "assistant": { "name": string|null, "avatar": string|null },
  "user":      { "name": string|null, "avatar": string|null } }
```

对每一侧（`char` → 助手；`persona` → 用户），在该会话的 **effective 节点树**里筛出该类型的、被启用 ref 引用的定义，按以下顺序选"身份定义"：

1. `name == template.name` 的那个；
2. 否则，树序中该类型的**第一个**；
3. 否则无（返回 `null`）。

返回字段：

- `name`：身份定义的 `name`；无身份定义则 `null`（前端用 i18n 兜底 `chat.assistant` / `chat.you`）。
- `avatar`：身份定义的 `meta.avatar`；助手侧若无则回退 `session.avatar`；再无则 `null`。（**不**在此处理 `$avatar`，见下。）

边界：

- 无模板的自由会话：无 `char`/`persona` 定义 → 两侧 `name`/`avatar` 皆 `null` → 前端兜底。
- persona 名基本不会等于 `template.name`，故实际走"树序第一个 persona"——符合预期。

## `$avatar` 动态覆盖（前端）

`$avatar` 是分支快照状态，`ChatView` 已随分支拉取 `sessionState.values['$avatar']`。为避免每次切分支都重拉 identity：

- **助手有效头像** = `$avatar`（若该分支已设）**否则** `identity.assistant.avatar`。
- 该覆盖只作用于助手（`$avatar` 是助手外观变量）；用户 persona 头像只取定义。

即：身份端点给出"定义级"静态身份（每会话拉一次）；`$avatar` 覆盖在前端用既有的 state 数据应用。

## 前端改动

- **`DefinitionEditor`**：当 `definition.type ∈ {char, persona}` 时，显示一个 `AssetPicker`，读写 `meta.avatar`（圆形预览）。其他类型不显示。
- **`ChatView`**：加载会话时 `getSessionIdentity(id)`，得到 `{assistant, user}`；连同 `$avatar` 覆盖计算出有效身份，向下传给消息列表与头部。
- **身份向下传递**：`ChatView` → `MessageList` → `MessageItem` 传 `identity = { assistant: {name, avatar}, user: {name, avatar} }`；`MessageItem` 按 `message.role` 取对应名/头像。
- **`MessageItem`**：渲染真实头像（`<img>`，无则回退现有纯色圆圈占位），名称用解析出的名（兜底 i18n）。bubble 与 flat 两种模式都接通。
- **头部（`ChatView`）**：显示**角色名 + 头像**（替换通用标题）。`session.name`（聊天标题）保留为首页列表标题。

## 会话标题重命名（#4 瘦身）

- 新增 `PATCH /api/sessions/:id`，body `{ name?: string, avatar?: string|null }`，返回更新后的 `Session`。需要存储层支持更新 `session.name`（及 `avatar`，便于将来）。
- UI：首页列表 `⋯` 菜单新增 **Rename**（就地编辑 `session.name`），与既有 Duplicate/Export/Delete 并列。聊天头部展示的是**角色名**（身份），与列表标题区分。

## 测试（TDD，逐层）

- **core**：身份解析纯函数 / 服务——name-match 优先、树序回退、无定义回退、助手头像回退 `session.avatar`。
- **web**：`GET /sessions/:id/identity`（导入卡 → 正确角色名/头像；无模板 → null）；`PATCH /sessions/:id` 改名 round-trip。
- **ui**：`DefinitionEditor` 对 char/persona 显示头像选择器、写 `meta.avatar`；`MessageItem` 按 role 渲染名/头像 + 回退；`ChatView` 头部显示角色名；`$avatar` 覆盖优先于定义头像。

## 涉及文件（预估）

- `shirita-core`：身份解析逻辑（assembly/或新模块）+ 单测。
- `shirita-web`：`routes/sessions.rs`（identity GET、PATCH）+ storage 改名方法 + 集成测试。
- `shirita-ui`：`api/client.ts`（getSessionIdentity、patchSession）、`api/types.ts`（Identity 类型）、`DefinitionEditor.vue`、`ChatView.vue`、`MessageList.vue`、`MessageItem.vue`、`HomeView.vue` / `ChatCard.vue`（Rename）、相应 `*.test.ts`。

## 兼容性

- 无 DB 迁移（头像入 `meta`）。
- 旧会话/无头像定义：走兜底（i18n 名 + 纯色圆圈 / `session.avatar`），不破坏现状。
