# Shirita — Prompt 树 v2 与世界书激活（设计 Spec）

> 状态：经 brainstorm 与用户逐条确认，待 review。
> 上游：`2026-06-13-m3-frontend-design.md`（M3 前端主体）、已完成的 M0–M2 与已落地的 M3 前端可视对齐（commit `0c68232`）。
> 本 spec 覆盖一个**完整子系统**：把 prompt 模板节点树从"装饰"升级为真正驱动组装的结构，并引入 SillyTavern 式的**世界书触发**（常驻 / 关键词 / 随机）、**历史消息魔法节点**、**可扩展类型**、**会话引用 + 写时复制**，以及为 **ST 配置导出/导入**预留的结构。

---

## 1. 背景与动机

M3 前端已铺好节点树 UI，但发现一个关键缺口：**后端组装从未使用节点树**。`conversation.rs` 仍调用 M2 的 `assemble_system_prompt(mounted, …)`，按 `session.mounted_definitions` 扁平列表分组包 XML —— 模板树对实际生成毫无影响。

本 spec 解决该缺口，并据用户反馈把节点树升级为有触发能力的"prompt 管理器 + 世界书"：

- 节点树真正驱动 system prompt 的**顺序与结构**。
- 条目（定义引用）支持**常驻 / 关键词 / 随机**激活（世界书）。
- 一个特殊的**历史消息节点**标记会话历史在 prompt 中的位置。
- 类型（角色/用户/世界设定/prompt）**可扩展**，新增可删、默认不可删。
- 会话**默认引用模板**，改动时**写时复制（局部覆盖）**。
- 数据结构对齐 SillyTavern，便于**导出/导入配置文件**。

## 2. 范围

**本 spec 内：**
- 类型注册表（`def_types`）+ 4 个默认类型 + 用户自定义类型。
- 节点模型（folder / ref / history）与放置规则。
- 触发模型（`definition.meta.trigger`）+ 激活算法（扫描深度、递归扫描开关）。
- 组装重写：`assemble_from_nodes`（遍历 → 触发求值 → 容器封包 → 历史切分 → 消息数组）。
- 会话引用 + 写时复制语义（`override_config`）。
- 前端节点树 v2（放置规则、添加流、历史行、展开内联触发编辑、拖动、删除）。
- 设置内的扫描参数（深度 + 递归开关）。
- 一批快速视觉修正（导航灰度 / 副标题 / 色板 / 自动取模型）。
- 为 ST 导出/导入预留的结构约束（适配器实现单列计划）。

**非目标（延后）：**
- ST 的 **@depth 注入**（把条目插入历史指定深度）—— 现仅 before/after。
- 每条目的"排除递归 / 整词匹配 / secondary keys" 精细控制（先做全局开关）。
- **对话内编辑面板**（`/chat/:id/options` 改名/换头像/局部覆盖管理）—— 数据模型已预留，UI 后续。
- 实际的 **ST 导出/导入适配器实现**（结构在此对齐，代码单列 Plan 5）。
- 消息编辑、头像上传、Custom CSS 注入（M3 既有 stub）。
- 分支/分叉持久化（属 M4）。

## 3. 类型系统（可扩展）

新增表 **`def_types`**：

| 列 | 说明 |
|----|------|
| `id` TEXT PK | 稳定英文 id（导出可移植），如 `char`/`persona`/`world` |
| `label` TEXT | 显示名（可 i18n），如 Character / User / World |
| `sort` INT | 排序 |
| `builtin` BOOL | 内置=true 不可删；自定义=false 可删 |
| `created_at` TEXT | |

**4 个默认类型：**

| id | label | 角色 | 封包 | 存储 |
|----|-------|------|------|------|
| `char` | Character（角色） | 容器 | `<char>…</char>` | `def_types` 行（builtin） |
| `persona` | User（用户） | 容器 | `<persona>…</persona>` | `def_types` 行（builtin） |
| `world` | World（世界设定） | 容器 | `<world>…</world>` | `def_types` 行（builtin） |
| `prompt` | Prompt | 特殊：根级、**裸文本**、无封包 | — | **代码常量**（不入表） |

- 即 `def_types` 表只存**容器类型**（3 个内置 + 用户自定义）；`prompt`、`regex_rule`、`tool` 是**代码层特殊常量**，不入表。
- `regex_rule` / `tool` 为**保留**类型：不进节点树（仅 Settings）。
- **可新建类型**："New type…" 写一行 `builtin=false` 的容器类型；其封包 = `<id>`。**内置不可删，自定义可删**（删除前要求其下无定义引用，或二次确认级联）。
- `definitions.type`（已是 TEXT）的校验集合 = `{prompt, regex_rule, tool} ∪ def_types.id`。
- **退役 Rust `DefinitionType` 枚举** → 改为 `String` 别名 + 校验/分类辅助函数（`is_container_type` / `is_reserved` / `wrap_tag`）。`item` 默认不再 seed（无现存数据依赖）。
- API：`GET /api/types`（列容器类型）、`POST /api/types`（新建）、`DELETE /api/types/{id}`（仅非内置）。

## 4. 节点模型与放置规则

沿用 `prompt_nodes` 表，`kind` 扩展为三种：

- **folder**：`kind=folder`，`tag` = 某容器类型 id，无 `definition_id`。渲染为 `<tag>…children…</tag>`。
- **ref**：`kind=ref`，有 `definition_id`，无 `tag`。渲染引用定义的内容。
- **history**：`kind=history`，无定义、无 tag。**每棵树恰好一个**，自动创建，**可移动**，**可通过勾选框停用**，**不可删除/复制**。

**放置规则（前端约束 + 后端校验）：**
- **根**只能放：容器 folder、`prompt` 类型 ref、history 节点 —— 其余不允许。
- 类型为 `T` 的**容器**只能放 `definition.type === T` 的 ref。
- **扁平**：一层（root → 容器 → ref）。**无 folder 嵌套**。
- 每种类型在根下**至多一个容器**（再次"添加容器"则聚焦已有）。

**迁移 `0008_prompt_nodes_history.sql`**：SQLite 无法原地改 CHECK，重建 `prompt_nodes` 表把 `kind` 约束放宽为 `('folder','ref','history')`，拷贝数据，重建索引。

## 5. 触发模型（世界书，挂在 `definition.meta`）

按用户决定，触发归属**定义**（同一定义在任何引用处行为一致）。`definition.meta.trigger`：

```jsonc
meta.trigger = {
  mode: "constant" | "keyword" | "random",  // 默认 constant
  keys: [],            // keyword 模式的主关键词；↔ ST "key"
  secondaryKeys: [],   // 预留；↔ ST "keysecondary"（本期不用）
  probability: 100,    // random 模式 0–100；↔ ST "probability"+"useProbability"
  order: 100           // 插入序，默认随树位；↔ ST "order"
}
```

- **constant**：始终包含（启用前提下）。
- **keyword**：`keys` 任一命中扫描窗口才包含。
- **random**：每次生成按 `probability` 掷骰决定。
- 字段命名刻意贴合 ST 世界书条目，便于导出（见 §11）。
- 触发属定义 → 在任意 ref 的展开编辑器里改触发 = 改全局（与内容一致；对话内则走局部覆盖，§7）。

## 6. 激活与扫描（SillyTavern 对齐）

设置级（全局，存 `settings`）：
- **Scan depth**（`worldinfo_scan_depth`，默认 4）：扫描最近多少条消息找关键词。
- **Recursive scan**（`worldinfo_recursive`，默认 on）：**必须可关闭**。

**激活算法（每次生成时，对一棵树的全部启用 ref）：**
1. 组装**扫描文本** = 最近 `scan_depth` 条消息（含本次 user 输入），全部小写。
2. 第一轮：`constant` 恒激活；`keyword` 若 `keys` 任一为扫描文本子串则激活；`random` 按概率掷骰。
3. **递归**（开启时，限 ~3 轮）：把"已激活条目的内容"追加进扫描文本，重新扫描 keyword 条目；直到无新增或到达轮数上限。关闭时只扫聊天、单轮。
4. 匹配为**大小写不敏感子串**（整词匹配、per-entry 排除递归留作后续）。
5. 启用勾选框为硬开关：未勾选 ⇒ 永不包含，无视触发。

## 7. 会话 ↔ 模板：引用 + 写时复制

**改变现状**：已落地代码在 `create_session` 时**急切深拷**模板树。改为用户要的引用模型：

- 会话存 `template_id`，**不拷贝**节点树。组装时有效结构 = **模板当前树**（模板后续改动自动反映到已存在会话 —— 引用的意义）。
- **对话内内容/触发编辑** → 存入 `session.override_config`：
  ```jsonc
  override_config = {
    local_definitions: { "<def_id>": { content?: string, trigger?: {…} } }
  }
  ```
  组装时按 `def_id` 覆盖（写时复制，逐定义）。**重置为全局** = 删除该覆盖；**覆盖全局** = 把覆盖写回 `definitions`（二次确认）。
- **对话内结构编辑**（启停 / 增删 / 重排）：首次结构改动时会话**惰性 fork** —— 把模板树拷为 `owner_kind=session` 的自有树，此后用自有树；fork 前纯引用。
- **书本内编辑** = 直接改全局 `definitions` / 模板 `prompt_nodes`。
- 无模板创建的会话：拥有一棵最小自有树（仅 history 节点）。
- 对话内编辑面板（`/chat/:id/options`）本期不做；数据模型已按"引用而非拷贝"预留，避免返工。
- **退役 `session.mounted_definitions`**：组装不再读它（保留列做兼容，后续清理）。

## 8. 组装流水线（核心）

新函数（core）：
```rust
fn assemble_from_nodes(
    nodes: &[PromptNode],            // 有效树（模板或会话 fork）
    definitions: &HashMap<String, Definition>,
    overrides: &serde_json::Value,   // override_config.local_definitions
    state: &serde_json::Value,       // {{var}}
    recent_msgs: &[String],          // 关键词扫描窗口
    scan: &ScanConfig,               // depth, recursive
) -> AssembledPrompt;

struct AssembledPrompt { before: String, after: String }  // 历史前 / 历史后
```

**步骤：**
1. 先算**激活集**（§6）。
2. 按 `sort_order` 遍历根级节点，遇 **history 节点切分** before/after：
   - **folder**：收集其"启用且激活"的子 ref，渲染内容（局部覆盖优先 → `{{var}}`），用 `<tag>\n…\n</tag>` 封包；**空容器省略**。
   - **根级 prompt ref**：渲染**裸文本**（无封包）。
   - **history（启用）**：标记切分点；其后节点进 `after`。
   - **history（停用）**：不切分；before/after 合并为单 system（单轮/无历史模式）。
3. `regex_rule`/`tool` 永不进封包（保持现状）。

**`send_message`（conversation.rs）改写：**
- 取会话有效树（自有 or 模板）+ 定义 + `override_config` + 最近消息。
- `let p = assemble_from_nodes(...)`：
  - `messages = []`
  - 若 `p.before` 非空 → `push(system, p.before)`
  - push 过滤隐藏后的**真实历史消息**（原样、按序）+ 本次 user
  - 若 history 启用且 `p.after` 非空 → `push(system, p.after)`（after-history 块；角色后续可配置）
- token 计数、SSE 流式不变。

**Worked example：**

树（按序）：
```
char (容器)
  ├ Neo        (char, constant)
  └ Trinity    (char, keyword: ["trinity","she"])
world (容器)
  ├ Zion       (world, keyword: ["zion"])
  └ The Matrix (world, constant)
Main Instructions (prompt, constant)   ← 根级 prompt
▸ Chat history (启用)                   ← history
Jailbreak    (prompt, constant)         ← 历史后
```
本次 user：「Tell me about Zion.」→ 激活：Neo✓ Trinity✗ Zion✓ Matrix✓ Main✓ Jailbreak✓。

输出 messages：
```jsonc
[
  { "role":"system", "content":
    "<char>\n{Neo}\n</char>\n<world>\n{Zion}\n{The Matrix}\n</world>\n{Main Instructions}" },
  ...真实历史消息(user/assistant) + 本次 user...,
  { "role":"system", "content": "{Jailbreak}" }
]
```
**固定规则**：容器封包、根 prompt 裸文本；历史原子不重排，仅整体定位；before→前置 system、after→后置 system；`{{var}}` 与 regex display 清洗照旧。

## 9. 前端节点树 v2

- **行**：`[启用勾选(圆角方块, 青绿)] [类型图标(按色板着色)] [名称] [hover: 删除] [展开 chevron]`。容器加粗；ref 常规；停用=灰（不删除线）。
- **添加流**（取代含糊的单一 "Add node"）：
  - **根 "+"** → `Add prompt`（内联选择器：搜索 prompt 定义 / + New prompt）· `Add container`（内联类型列表 `Character · User · World · + New type…`）。
  - **容器 "+"** → 该类型定义的内联选择器（搜索 / + New `<type>`），购物车式可多加。
- **展开 ref** → 内联编辑：内容 textarea（+ 全屏）**与触发控件**（Constant/Keyword/Random 分段；Keyword→关键词 chips；Random→概率滑块）。
- **history 行**：独立 "Chat history" 行，专属图标、勾选框、拖动手柄；无编辑器、无删除。
- **拖动**：同层重排 + 重定位 history；非法落点拒绝（char ref 不能落根）；持久化经 `reorder` + `parent_id`。
- **删除**：每节点删除；非空容器二次确认（级联）；history 例外。
- **类型动态**：类型 chips / Add container 列表从 `GET /api/types` 加载。

## 10. 一批快速视觉修正（无需设计，随本期一并做）

- **导航三图标**：回到**灰度差异**（激活更深、非激活更浅），统一笔画，去掉加粗。
- **副标题**：所有 section 副标题更醒目（更深色/更重）。
- **色板**：真正用起来 —— 节点类型按色板着色（char→sky、persona→coral、world→mauve、prompt→中性）、头像与强调色引入 sky/coral/mauve。
- **取模型**：去掉 "Fetch models" 按钮，source/base-url/key 就绪后**自动**调 `/models` 填下拉。

## 11. 导出 / 导入兼容（驱动结构选择）

数据结构选型确保可与 SillyTavern 往返：

- **世界书条目**（任意带触发的容器 ref）↔ **ST World-Info / lorebook 条目**：`meta.trigger` 字段刻意贴合（`keys`↔key、`secondaryKeys`↔keysecondary、`probability`↔probability/useProbability、`order`↔order）；启用↔ST `disable`（取反）；`name`↔comment；`content`↔content。位置/@depth 现由树位推导，显式留后续。
- **角色定义** ↔ **ST Character Card V2/V3**（`{name, description, …, character_book}`），角色关联的世界条目导出为内嵌 `character_book`。
- **模板** ↔ 类 ST preset 的 JSON（prompt 顺序 + 树）。
- **约束**：定义、触发、树均为纯 JSON，**不泄露仅 DB 字段**；类型以稳定英文 id 引用 → 导出可移植。
- 适配器（character card / world info / preset 的导入导出）为**单列 Plan 5**，但结构在此对齐，不留死角。

## 12. 数据模型与 API 变更汇总

**迁移：**
- `0007_def_types.sql`：建 `def_types` 表 + seed 4 默认（char/persona/world 容器、外加内置标记；prompt 作为代码常量不入表或入表标记特殊 —— 实现时择一，spec 取"容器类型入表、prompt/regex_rule/tool 代码常量"）。
- `0008_prompt_nodes_history.sql`：重建表放宽 `kind` CHECK 至 `('folder','ref','history')`。

**存储层（Storage trait + sqlite）：**
- `def_types` CRUD；`list_container_types()`。
- 节点：现有 list/create/update/delete/reorder/get/copy 保留；新增**会话节点端点**所需的按 owner 读写（已具备 owner_kind 维度）。
- 会话创建：**不再深拷**模板树（仅存 template_id）；提供惰性 fork（首次结构改动时 copy_nodes(template→session)）。
- 模板创建：自动建一个 history 节点。

**组装：** `assemble_from_nodes`（§8）替代 `assemble_system_prompt`；保留 `render_vars` / `apply_regex_rules` / `effective_content`。

**API（新增/变更）：**
- `GET/POST /api/types`、`DELETE /api/types/{id}`。
- `GET /api/sessions/{id}/nodes` + 会话节点的 create/update/delete/reorder（镜像模板端点，供后续对话内编辑）。
- `POST /api/sessions`：带 `template_id` 时**仅引用**（不深拷）。
- 既有 overrides 端点（set/reset/promote）沿用；扩展为可覆盖 `trigger`。
- 既有 templates/nodes/definitions/settings/provider 端点沿用。

## 13. 前端结构变更

- 重建：`PromptTree` / `NodeRow` / `NodePicker`（放置规则、添加流、拖动、删除、history 行、动态类型）。
- 新增：`TriggerEditor`（mode 分段 + 关键词 chips + 概率滑块）。
- 调整：`DefinitionEditor` 类型 chips 动态化；`library` store 加类型缓存 + 自动取模型；Settings 加 Scan depth / Recursive 开关。
- 客户端：`listTypes/createType/deleteType`、会话节点函数、`updateDefinition` 带 trigger。

## 14. 安全

- 仍**严禁 `v-html`**；内容安全渲染。
- 关键词扫描、触发求值全在 core，前端只渲染结果。
- 导出文件为纯数据；导入需校验/净化（适配器计划内处理）。
- Bearer 鉴权链不变。

## 15. 测试 / TDD

- **Core 单测**：`def_types` 分类与封包；触发求值（constant/keyword/random、递归开/关、scan depth）；`assemble_from_nodes`（容器封包、根裸 prompt、历史切分 before/after、history 停用合并、空容器省略、局部覆盖优先、`{{var}}`）；会话引用 vs fork。
- **Web 集成测试**：types CRUD；会话引用模板组装；overrides 影响组装；session nodes 端点。
- **前端组件测试**：节点树放置约束、添加流、拖动重排、删除、history 行不可删可停用、TriggerEditor 三模式。

## 16. 计划series（writing-plans 时细分）

1. **后端核心**：`def_types`（迁移+存储+API）+ history 迁移 + `assemble_from_nodes`（触发/递归扫描/历史切分）+ `conversation.rs` 改引用会话树 + 会话节点端点 + 模板/会话自动 history 节点 + 改 create_session 为引用。
2. **前端树 v2**：放置规则、Add prompt / Add container / New type、history 行、删除、拖动、动态类型。
3. **前端触发**：`TriggerEditor` + 定义/展开节点接线 + Settings 扫描参数。
4. **快速视觉修正**：导航灰度 / 副标题 / 色板 / 自动取模型（独立，可先行）。
5. **ST 导出/导入适配器**：character card V2/V3、world info、preset 的导入导出。

## 17. 待确认 / 风险

- `DefinitionType` 枚举 → String 的重构波及面（assembly、定义模型、序列化）—— 已确认要做。
- after-history 块默认 `system` 角色；是否需 per-node 角色配置 —— 留后续。
- 惰性 fork vs 细粒度结构覆盖：本期取惰性 fork（更简单），对话内结构编辑 UI 落地时再评估。
- @depth 注入、整词匹配、per-entry 递归排除：明确延后。
