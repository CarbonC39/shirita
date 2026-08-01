# Shirita — 卡/预设分离：Pack（包）、Template（预设框架）与作用域绑定（设计 Spec）

> 状态：经 brainstorm 与用户逐条 co-design，待 review。
> 本 spec 落地 `2026-06-18-regex-and-variables-design.md` §7 明确拆出的 **「#4 卡/预设分离」**。当时预判两方向：X 彻底分离（`card_id`+`preset_id`，ST 对齐）/ Z 模板可组合。**本设计 = X 的精炼版**：会话 = 1 个 Template（预设框架）+ N 个 Pack（内容包），packs 经一个魔法 `<<content>>` 节点流入框架。
> 上游：Prompt 树 v2 / 世界书（`2026-06-13-prompt-tree-worldbook-design.md`，已落地）、regex/变量一等化（`2026-06-18-…`，已落地）、聊天身份（`2026-06-17-chat-identity-design.md`，已落地 `identity.rs`）、ST 角色卡/世界书导入（`charcard.rs`/`worldinfo.rs`，已落地）。
> **不在本 spec**：HTML 的渲染/授权深设计（三角色 Theme/Card/Inline、shadow 沙箱、Card 绑变量、预览、编辑器）—— 另起专项 spec（见 §13）。多角色「谁在说话」的消息路由 —— 推迟。

---

## 1. 背景与动机

### 1.1 现状的三层割裂
- **Definition**（原子）：全局扁平库里的单个 `{type, name, content, meta}`——「一个一个」散落。
- **Template**（一整套）：一棵 `prompt_nodes` 树（folder=类型容器渲染 `<tag>`、ref=引用定义、history=魔法占位）。ST 导入时把「角色卡」与「prompt 预设」**塞进同一棵树**。
- **html / regex / 变量**：散在三处且像「ST 卡的附属」——regex 是 `regex_rule` 定义 + 树引用决定作用域；变量是 `template.meta.variables` + 会话 `local_variables`；html 是 `html_patch` protocol + 渲染侧 `HtmlCardFrame`。

### 1.2 核心痛点（用户语）
1. **缺中间层**：template「一整套」、definition「一个一个」，没有「文件夹」能把**一组相关定义 + 它们专属的 html/regex/变量**收成一个有凝聚力、可整体管理/挂载/导出的单位。
2. **卡/预设纠缠**（#4 根因）：单一 template 同时承载「这张角色卡」与「这套 prompt 顺序」，**「只换 prompt、留着这张卡」做不到**。
3. **html/regex/变量不够一等**：作用域要么全局、要么靠隐式的树引用，冲突无明确语义；html 尤其黑箱。
4. **folder 太尴尬**：现仅作「类型容器」用，没发挥「占位符 / 选择枢纽」的潜力。

### 1.3 设计立场
- 引用集合（共享原子），不做封装拷贝——契合现有 reference + COW 架构。
- 组合式机器、**无强制角色**；角色只是默认约定，不是结构铁律。开放类型注册表已支撑这点。
- **面向预设词作者的编写体验**：作者分别编辑「预设框架」与「内容包」，各自绑自己的 html/regex/变量，导出即分享。
- 尽量兼容 SillyTavern，但不纵容其死板（卡=上帝对象、固定字段、世界书外挂）。

---

## 2. 范围

**本 spec 内：**
- 新实体 **Pack**（内容包）：内容树（owner=pack）+ 可选 identity + 绑定的 regex/变量/html。
- 新魔法节点 **`<<content>>`**（`NodeKind::Content`）：挂载的 packs 在此按类型流入框架。
- **Folder 升级**：`select` 选择策略（all/one）+ `tag` 可空（纯组织/选择 vs 包 tag）。
- **会话挂载模型**：会话 = `template_id` + 有序 `pack_ids` + history。
- **作用域绑定**：regex/变量/html 的 owner 扩展到 Pack；regex 确定性有序管道；变量 schema 合并；html 绑定（**仅绑定与作用域，渲染另议**）。
- **身份精炼**：assistant/user 身份来自挂载的 char/persona Pack 的 identity（精炼现有 `identity.rs`）。
- **组装改写**：`assemble_from_nodes` 在 `<<content>>` 处注入 pack 内容（跨包按类型分组封包）；世界书触发求值覆盖 pack 内容。
- **ST 导入映射**：角色卡 → Pack；preset → Template；内嵌 lorebook → Pack 内 world 定义；内嵌 regex → Pack 绑定 regex。
- **迁移与向后兼容**：现有模板树不破坏（pinned 内容仍渲染）；默认模板补 `<<content>>`。

**非目标（推迟，见 §13）：**
- HTML 渲染/授权深设计（三角色、shadow 沙箱、Card 绑变量、预览、编辑器）—— 专项 spec。
- 多角色「谁在说话」的消息路由 / 一个 Pack 内两个角色。
- ST 的 @depth 注入、整词匹配等（沿用现有推迟项）。

---

## 3. 核心模型

**三种对象，其中两种是「作用域容器」（都能绑 html/regex/变量）：**

| 对象 | 是什么 | 角色 | 现状 |
|---|---|---|---|
| **Definition**（原子） | `{type, name, content, meta}`，type 来自开放注册表 | 最小可复用单元 | 不变 |
| **Pack**（内容容器） | 一棵 owner=pack 的节点树（Ref/Folder）+ 可选 identity + 绑定 html/regex/变量 | 可分享的**内容**：Alice、奇幻世界、家规、用户 persona | **新增** |
| **Template**（框架容器） | 一棵含 `<<content>>`/`<<history>>` 的节点树 + 绑定 html/regex/变量 | 可分享的**预设**：预设词作者的产物 | 已存在，扩展 |

**节点树（统一，一个递归概念）。** 节点 = `{ tag?, select, enabled, kind, definition_id?, sort_order, owner_kind, parent_id, meta }`：

| kind | 子节点来源 | 就是 | 现状 |
|---|---|---|---|
| `ref` | — | 引用一个定义，渲染其内容 | 已有 |
| `folder` | authored（你放进去） | 组织 + 选择枢纽，**可选**包 `<tag>` | 已有，加 `select` |
| `history` | runtime（聊天消息） | `<<history>>` 魔法占位 | 已有 |
| `content` | runtime（挂载的 packs） | `<<content>>` 魔法挂载点 | **新增** |

> **去魔法化**：`history`/`content` 不是另一种「形状」，只是**子节点由 runtime 填充**的节点。`content` 内部按类型分出的 `<char>…</char>`、`<world>…</world>` 本身就是自动生成的 folder。渲染逻辑一个递归函数即可。

**会话 = Template + N 个 Pack + history。** 组装时：渲染框架 → 在 `<<content>>` 注入各 pack 的 typed 定义（跨包按类型分组封包）→ history 注入消息 → 合并模板与所有挂载 pack 的 regex/变量 schema/html 绑定。

**为何不套娃**：只有 Template 树含 `content`/`history` 这两种 runtime 节点，**Pack 树没有** → pack 装不进 pack，物理上杜绝递归。一层注入（template 的 content ← packs），零套娃。

### 3.1 一张图

```
Definition（原子，不变）           {type, name, content}

Pack「Alice」  (owner=pack)        identity{name:"Alice", avatar} · 绑: $好感度 · alice-regex · 问候html
├─ Ref → "Alice角色卡"  (char)
├─ Ref → "Alice问候"    (first_message)
└─ Folder「心情」 select=one tag=∅   ← folder 当枢纽
   ├─ Ref → "开心版" (char) ✓
   └─ Ref → "生气版" (char)

Template「RP预设」 (owner=template)  绑: cleanup-regex · theme-html
├─ Ref → "主系统提示" (prompt)
├─ Folder「文风」 select=one tag=∅    ← 六选一
│  ├─ Ref → "细腻文风" ✓ / Ref → "简洁文风" / Ref → "古风文风"
├─ <<content>>   ← 新节点：packs 灌进来
├─ Ref → "Jailbreak" (prompt)
└─ <<history>>   ← 已有

会话 = Template「RP预设」 + [Pack「Alice」] + history
→ <<content>> 处长出: <char>{Alice角色卡}+{开心版}</char>  <first_message>{Alice问候}</first_message>
→ regex = cleanup + alice（按确定性顺序）；变量 = $好感度；html 绑定 = theme + 问候
```

---

## 4. 节点树升级（Folder 枢纽化 + Content 节点）

### 4.1 Folder 的 `select` 与可空 `tag`
- `meta.select: "all" | "one"`（默认 `all`）。`all` = 渲染所有 enabled 子节点（普通分组）；`one` = 同组只有一个 active（启用一个自动停用兄弟，单选枢纽——「六个文风/心情选一个」）。`n` 暂不做，留口。
- `tag` 已是 `Option<String>`：`Some("char")` 渲染 `<char>…</char>`；`None` 仅组织/选择、子节点裸渲染（六个文风不要 `<style>` 外壳）。前端加 wrap-in-tag 开关。
- **零 schema 变更**：`select` 入 `prompt_nodes.meta`，`enabled` 既有。
- **Folder 可嵌套吗？** 维持现有「视觉扁平」哲学：允许一层选择 folder，不鼓励深嵌；放置规则沿用现有约束（§ 现有 prompt-tree-v2 §4），content 节点取代「类型容器」承接动态内容（见 4.3）。

### 4.2 `NodeKind::Content`（`<<content>>` 魔法挂载点）
- 每棵 **Template** 树恰好一个 `content` 节点，自动创建，**可移动**（决定内容在 prompt 中的位置）、**可勾选停用**、**不可删除/复制**——与 `history` 同规格。
- 渲染：组装时收集会话挂载的全部 packs，把它们的 enabled+activated ref 按 `definition.type` **跨包聚合**，每类型包一层 `<type>…</type>`（类型顺序按 `def_types.sort`），整体作为该位置的段输出。空类型省略。
- **Pack 树没有 content 节点**（防套娃，§3）。
- 迁移：`prompt_nodes.kind` CHECK 放宽到 `('folder','ref','history','content')`。

### 4.3 Template 树里还允许「pinned 内容 ref」吗？
**决策（可在 review 否决）**：允许，向后兼容 + 进阶用途。
- 现有模板树里 author 放置的 `char`/`world` 类型容器与 ref（含 ST 旧导入）**继续按现状渲染**为固定内容——不破坏 M0–M8 数据。
- **惯用做法**：随会话变化的 char/world 内容走 **Pack → `<<content>>`**；模板保持内容无关（content-agnostic 框架）。
- 即：固定内容可 pin 在模板，动态内容走 packs。两条路并存，文档引导走 packs。后续可提供「把模板里的容器提升为 Pack」的便捷操作（非本 spec）。

---

## 5. Pack（内容包）

### 5.1 模型
- 新表 `packs`：`{ id, name, identity_json, meta, created_at, updated_at }`。
- **内容** = `prompt_nodes` 树，`owner_kind = 'pack'`，`owner_id = pack.id`。复用全部既有节点机制（Ref/Folder、reorder、COW）。
- Pack 树**只含 authored 节点**（ref/folder），**无 content/history**。
- `OwnerKind` 增加 `Pack`（`prompt_nodes.owner_kind` 现已是 TEXT，`from_db` 接受 `"pack"`）。

### 5.2 identity（精炼现有 `identity.rs`）
- `packs.identity_json` = `{ display_name?: string, avatar?: string }`（可空）。带 identity 的是「角色包/persona 包」；纯 world/规则包无 identity。
- assistant 身份 = 会话挂载的**第一个带 identity 且含 `char` 内容的 pack** 的 identity；`$avatar`/`$assistant_name` 分支变量覆盖照旧（与 chat-identity spec 优先级一致）。
- user 身份 = 挂载的 **persona pack** 的 identity。**persona 本期一并 Pack 化**：persona pack = identity{name,avatar} + persona 类型内容；「选用户 persona」= 挂载一个 persona pack。向后兼容：无 persona pack 的旧会话回退现有「persona 定义 name + `meta.avatar`」解析。
- `identity.rs::resolve_identity` 增 pack-aware 路径：**有挂载 pack → 用 pack identity**；无 pack（旧会话）→ 沿用现「按 template.name 匹配 char 定义」启发式。`session.avatar` 仍是 assistant 头像的会话级真相（identity.avatar 缺省时回退它）。
- **新建会话流程接通**：M3 的 Step1「头像 + 名」= 建/选一个角色 Pack 的 identity；Step2 选 Template。多角色 = 挂多个角色 pack（一包一身份）。

### 5.3 绑定生命周期（沿用 #4 note）
- prompt / world / scoped-regex / 变量 schema / html 绑定 = **实时跟随会话 effective 结构**（挂载的 packs + template）。
- **first_message 与变量初始值** = 会话创建时**一次性 seed**（不回溯改写）。first_message 取挂载角色 pack 的 `first_message` 定义。
- materialize（对话内结构编辑触发 COW）后整棵会话 effective 树冻结为自有副本——既有语义不变。

---

## 6. Template（预设框架）

- 模型不变（`templates` 表 + owner=template 节点树），新增**自动 `<<content>>` 节点**（创建模板时与 history 并列种入，content 在 history 之前）。
- 内容 = `prompt` 类型 ref（系统提示、jailbreak）+ 选择 folder（文风等）+ `<<content>>` + `<<history>>` +（向后兼容的 pinned 容器）。
- 绑定：模板级 regex/变量/html（作者写的「这套预设的清洗规则 / 主题 / 系统变量」）经树引用（regex/html 定义 ref）或 `template.meta.variables`（变量 schema，现状）归属本模板。

---

## 7. 作用域与绑定（html / regex / 变量 一等化）

统一原则：**html/regex/变量都是一等对象，owner = 某个 Pack 或 Template，只在该容器进入会话时激活。** 不再有「全局一锅粥」。

### 7.1 Regex —— 确定性有序管道
- 沿用 `regex_rule` 定义 + `meta{pattern,replacement,scope,targets}`（fancy-regex，现状）。
- **作用域来源扩展**：现有 `effective_regex_rules` = 全局 orphan + 会话 effective 树引用。**新增：挂载 packs 树引用的 regex 定义**。
- **冲突 = 顺序**：把「一锅炒」改成确定性管道——**模板树序 → 各 pack 按挂载顺序 → 包内树序**，逐条 find/replace 串行；每条 `enabled`/`scope`/`targets` 既有。顺序在 §3 regex 管理 UI 里可见、可开关。作用域已限定（包没挂就不生效）→ 冲突面本就小。

### 7.2 变量 —— schema 合并
- schema 来源：系统变量 ∪ `template.meta.variables` ∪ **各挂载 pack 的 `pack.meta.variables`** ∪ 会话 `override_config.local_variables`（后者覆盖）。
- 初值在会话创建时 seed（§5.3）；旧快照对新增变量按 schema 回填初值（现有 `effective_state` 兜底语义不变）。

### 7.3 HTML —— 本 spec 只做「一等 + 可绑定」
- html 定义/资源的 owner 可为 Pack 或 Template（与 regex 同模式：树引用归属）。
- 现有 `html_patch` protocol（教模型如何吐 HTML 卡的指令）保持「protocol 定义 + 条件注入」机制；其归属可绑到 Pack/Template（如某角色包专属的卡协议）。
- **渲染/沙箱/Card 绑变量/预览/编辑器深设计不在本 spec**（§13）。本 spec 只保证 html 不再是全局黑箱，而是有 owner、有作用域、有编辑入口的对象。

### 7.4 冲突解决总则
- **regex**：确定性顺序（§7.1），每条可开关，作用域隔离。
- **html**：**默认 shadow DOM 隔离**——每张卡自己的 shadow root，包之间 CSS/JS 不串味（**实现细节属渲染 spec**，本 spec 仅确立隔离原则）。**Theme** 是唯一有意全局的层，单一胜出（会话覆盖 > 模板）。

---

## 8. 组装流水线（在现有 `assemble_from_nodes` 上增量）

现有 `assemble_from_nodes`（assembly.rs:416）遍历 owner 树、按 history 切分 before/after、容器封包、世界书触发求值——**保留**。本 spec 的增量：

1. **输入扩展**：除会话 effective 树（模板/会话 fork）外，传入**挂载 packs 的有效树集合**。
2. **`content` 节点处理**：遍历遇到 `content` 节点（启用）时：
   - 收集所有挂载 pack 树的 enabled ref；
   - 对这些 ref 跑**世界书触发求值**（constant/keyword/random，沿用 `keyword.rs` 的 Aho-Corasick；扫描窗口同现状）；
   - 把 activated ref 按 `definition.type` **跨包聚合**，每类型 `<type>\n…\n</type>` 封包（类型序 = `def_types.sort`；空类型省略）；
   - 作为该 placement 的段输出。
   - `content` 节点停用 → 跳过，packs 内容不进 prompt。
3. **regex / 变量 / 身份**：按 §7、§5.2 合并 packs 来源。
4. `build_chat_messages` 序列化（段→消息、同角色合并）—— 不变。

**触发求值的归并**：模板 pinned 容器（§4.3）与 packs 内容**共享同一轮激活算法**（同一扫描缓冲、同一递归预算），避免两套逻辑。

---

## 9. 数据模型与迁移汇总

**迁移：**
- `00NN_packs.sql`：建 `packs` 表（`id` PK / `name` / `identity_json` TEXT / `meta` TEXT / `created_at` / `updated_at`）。
- `00NN_prompt_nodes_pack_content.sql`：重建 `prompt_nodes`，`kind` CHECK 放宽至 `('folder','ref','history','content')`；`owner_kind` 接受 `'pack'`（若有 CHECK 一并放宽）。拷贝数据、重建索引（SQLite 无法原地改 CHECK，沿用现有重建套路）。
- `00NN_session_packs.sql`：建 `session_packs`（`session_id` FK / `pack_id` FK / `sort_order`；复合唯一 (session_id,pack_id)）—— 会话挂载的有序包列表。
- 默认模板补 content 节点：`seed::ensure_default_template` 在 history 前种 `content` 节点；**数据迁移**给现有模板补一个 content 节点（置于 history 之前；无 history 的置末）。

**存储层（Storage trait + sqlite）：**
- `packs` CRUD + `list_packs`；pack 节点走既有按 owner 读写（owner_kind=pack）。
- `session_packs`：`set_session_packs(session_id, [pack_id])`（有序，整体替换）+ `list_session_packs`。
- identity：`packs.identity_json` 读写。

**API（新增/变更）：**
- `GET/POST /api/packs`、`GET/PUT/DELETE /api/packs/{id}`、pack 节点端点（镜像模板节点端点，owner=pack）。
- `PUT /api/sessions/{id}/packs`（设挂载包有序列表）、`GET /api/sessions/{id}/packs`。
- `POST /api/sessions`：可带 `template_id` + `pack_ids`（创建时 seed first_message/变量初值）。
- 既有 `/sessions/{id}/identity` 内部改为 pack-aware（响应形状不变）。
- 既有 types/templates/nodes/definitions/regex/settings 端点沿用。

---

## 10. 向后兼容

- **现有模板树不破坏**：pinned 容器/ref 继续渲染（§4.3）；只新增 content 节点（迁移补）。
- **旧会话**（仅 template_id、无挂载 pack）：identity 走旧启发式（§5.2）；组装 content 节点为空段（无挂载 pack）→ 输出等价于「仅模板 pinned 内容」，行为不退化。
- **`mounted_definitions`**：现已被 prompt 树取代（旧列保留兼容，组装不读）。本 spec 不依赖它。
- **ST 旧导入**（曾以 template+char 定义导入的「卡」）：保持为 template，仍工作；新导入走 Pack（§11）。

---

## 11. ST 导入映射

- **角色卡（V2/V3）** → **1 个 Pack**：`name`→pack.name + identity.display_name；卡图→identity.avatar；`description/personality/scenario`→`char` 定义；`first_mes`/`alternate_greetings`→`first_message` 定义（多问候入 `select=one` folder）；内嵌 `character_book`→pack 内 `world` 定义（带 `meta.trigger`，沿用 `worldinfo.rs`）；内嵌 regex_scripts→pack 绑定 `regex_rule` 定义。
- **独立世界书（lorebook）** → **1 个纯 world Pack**（无 identity）。
- **ST preset（prompt manager 顺序）** → **1 个 Template**（prompt 顺序 + `<<content>>` + `<<history>>`）。
- 现有 `charcard.rs`/`worldinfo.rs` 适配器改导入目标为 Pack（而非 template+定义平铺）；导出对称（Pack→卡，Template→preset）。**适配器改造列入独立 Plan**。

---

## 12. 前端影响（概述，细节随 plan）

- **Book**：拆成「**Pack 编辑器**」（identity + 内容树 + 绑定的 regex/变量/html 列表）与「**Template 编辑器**」（框架树 + content/history + 绑定）。模板/包各自 picker + 新建/导入/复制/删除。
- **PromptTree 组件**：加 folder 的 `select`（all/one 切换）+ wrap-in-tag 开关；渲染 `content` 行（专属图标、可停用、不可删，类 history 行）。复用于 template 与 pack 两种 owner。
- **新建会话**：Step1 = 建/选角色 Pack（identity）；Step2 = 选 Template；可选挂额外 packs（world/规则）。
- **会话内**：可增删挂载 packs（`PUT …/packs`）；现有 COW/局部覆盖语义不变。
- i18n：Pack/挂载/选择策略/content 等新键过 `parity.test.ts`。

---

## 13. 不在本 spec（后续专项）

- **HTML 渲染/授权专项 spec**：三角色（Theme 作用域 CSS / Card 渲染变量的确定性视图 / Inline 模型现吐 + shadow 隔离）、沙箱、Card↔变量绑定、预览、编辑器 UX。本 spec 只交付 html 的「一等 + 可绑定 + 作用域」骨架。
- 多角色「谁在说话」消息路由；一个 Pack 内多角色身份。
- 模板可组合（#4 方向 Z）—— 本设计取方向 X，Z 不做。
- ST @depth 注入、整词匹配、per-entry 递归排除（沿用现有推迟）。

---

## 14. 安全
- 仍**严禁 `v-html`** / 直接执行 LLM 生成的 JS；html 渲染沙箱（shadow/iframe）属渲染 spec。
- 触发求值、regex、身份解析全在 core；前端只渲染结果。
- 导入需校验/净化（适配器内处理）；Bearer 鉴权链不变。

---

## 15. 测试 / TDD
- **Core 单测**：folder `select=one`（启停互斥、组装只取 active）；`content` 节点组装（跨包按类型聚合封包、空类型省略、停用跳过、与 pinned 容器共享触发求值）；regex 确定性管道顺序（模板→包→包内）；变量 schema 合并（含 pack 来源 + 初值 seed）；pack-aware `resolve_identity`（有 pack 用 identity、无 pack 回退旧启发式）。
- **Web 集成**：packs CRUD + pack 节点；`PUT/GET /sessions/{id}/packs`；建会话带 pack_ids → seed first_message/变量；identity 端点 pack-aware；旧会话（无 pack）组装不退化。
- **前端组件**：PromptTree 的 select/tag 开关、content 行不可删可停用；Pack 编辑器 identity + 绑定列表；新建会话两步接 Pack/Template。
- 每步 `cargo test --workspace` + `vue-tsc` + `vitest` 绿。

---

## 16. 计划 series（writing-plans 时细分）
1. **后端模型与存储**：`packs` 表 + `session_packs` + `prompt_nodes`(content/pack) 迁移；Storage CRUD；OwnerKind::Pack / NodeKind::Content；默认模板补 content + 数据迁移。
2. **组装与作用域**：`assemble_from_nodes` 接 content 节点（跨包聚合 + 触发）；regex 有序管道接 pack 来源；变量 schema 合并 pack；身份 pack-aware。
3. **Web API**：packs / pack-nodes / session-packs 端点；建会话 seed；identity 内部改造。
4. **前端**：Book 拆 Pack/Template 编辑器；PromptTree select/tag/content；新建会话两步；i18n。
5. **ST 导入/导出适配器改造**：卡↔Pack、preset↔Template、lorebook↔world Pack。

---

## 17. 已决 / 待确认 / 风险

**已决（review 确认）：**
- **命名** = **Pack（包）** + **Template（预设框架）**（泛化 ST 的「卡/预设」）。
- **§4.3 并存** = 采纳：模板 pin 容器 + packs 走 `<<content>>` 两条路并存（迁移轻、灵活）。
- **persona Pack 化** = 本期落地：persona 是一种 pack（identity + persona 内容），旧 persona 定义解析作向后兼容回退。

**待确认 / 风险：**
- **触发求值归并**：pinned 容器与 packs 共享一轮激活——确认无语义冲突（同一扫描缓冲、递归预算）。
- 迁移给现有模板补 content 节点的位置（history 前）—— 对个别自定义树是否需更智能定位。

---

## 18. 参考
- `2026-06-18-regex-and-variables-design.md` §7「#4 卡/预设分离」（本 spec 的直接上游与方向 X/Z 来源）。
- `2026-06-13-prompt-tree-worldbook-design.md`（节点树 v2、世界书触发、def_types、组装流水线）。
- `2026-06-17-chat-identity-design.md`（身份解析优先级，本 spec §5.2 精炼）。
- 已落地源：`assembly.rs`（`assemble_from_nodes`）、`identity.rs`、`keyword.rs`、`adapters/{charcard,worldinfo}.rs`。
