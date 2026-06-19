# Shirita — Regex 完善、管理 UI 与变量协议自动注入（设计 Spec）

> 状态：经 brainstorm 与用户逐条确认，待 review。
> 范围：**#1** Regex 管理 UI（紧凑化 + 作用域可视）、**#2** Regex 应用补全（user_input + prompt 侧）、**#3** 变量 `<state_update>` 协议自动注入。
> **不在本 spec**：卡/预设分离（#4）已拆为独立设计（见末节）。
> 上游：M5 变量状态（`state.rs`）、Prompt 树 v2 / 世界书、ST 角色卡导入（`charcard.rs`）、同会话 HTML-card patch（commit `b7bd93f`，本 spec 的注入机制与之对齐）。

---

## 0. 背景与现状（代码实证）

- `regex_rule` 是一种 `Definition`，`meta = { pattern, replacement, disabled, scope, targets }`：
  - `scope: "display" | "both" | "prompt"`（WHERE：改显示 / 两者 / 改发送）。
  - `targets: ("ai_output" | "user_input")[]`（WHICH：作用在哪一侧消息；空数组 = 广义，向后兼容旧规则）。
- `assembly::apply_regex_rules` **目前只实现了 `ai_output × display`**：`scope=="prompt"` 跳过、`targets` 不含 `ai_output` 跳过、`disabled` 跳过。产物写入助手消息的 `display_content`（`conversation.rs` 落库时计算；`messages.rs::edit_message` 手动编辑时重算）。`raw_content` 永远是模型原文。
- **作用域（hybrid model，`assemble_request`）**：
  - **全局规则** = orphan（不被任何节点引用，在设置里建）→ 对每个会话生效。
  - **局部规则** = 被本会话 effective 树引用（如 ST 卡导入的 `regex_scripts`）→ 仅该会话生效。
  - 二者互斥（被树引用的不是 orphan），拼接无需去重。
- **变量**：`state.rs` 只**解析** `<state_update action="…" key="…" value="…"/>`（动作 SET/ADD/SUB/TOGGLE/APPEND/REMOVE）并折叠进 `snapshot_state`；**引擎从不向模型注入协议说明**——需用户自行在 prompt 里写。这是"缺少清晰指示"的根因。
- 三项改动彼此独立，且都**不依赖 #4**：regex 的"全局 vs 卡内"作用域、应用逻辑、变量注入，无论将来卡/预设是否分离都成立。

---

## 1. Regex 管理 UI（前端为主 + 一个轻量后端查询）

### 1.1 问题
`SettingsView.vue` 用 `listDefinitions()` 拉全部 `regex_rule` 并 `v-for` 渲染 `RegexRuleEditor`（每条一张带边框卡片）。即便 `RegexRuleEditor` 本身可折叠，N 条规则仍是 N 张卡片，且**无作用域区分、无置顶、无搜索/过滤**。导入多张 ST 卡后，全局与各卡的 regex 混成一堆，"看起来全是全局、一团乱"。语义其实是分作用域的（见 §0），问题纯在**展示组织与密度**。

### 1.2 设计
把"一摞卡片"改成 **紧凑 master-detail 单行列表**（行高接近 definition 列表）：

- **排序**：全局规则（orphan）**置顶**（背景色 A）；局部规则在下（背景色 B），每行带**来源模板/卡名**小标签。平铺，不做可折叠分组。
- **手风琴编辑**：点行内联展开编辑（pattern / replacement / 作用对象 / 范围），**同时只展开一行**。
- 每行右侧两个 mini-badge：**作用对象**（AI / 用户）·**范围**（显示 / 两侧 / prompt）。
- 停用规则**置灰**仍显示；列表顶部 **「隐藏停用」开关 + 搜索框**。
- 「新建全局规则」按钮保留（创建的就是 orphan）。

### 1.3 后端：作用域查询
前端要分全局/局部并显示来源卡名，需每条 regex 的作用域信息。新增只读端点：

```
GET /api/regex-rules/scopes
→ [ { "id": "<def_id>", "scope": "global" | "template", "template_names": ["女仆", ...] } ]
```

实现：用已有的 `Storage::referenced_definition_ids()` 判定 orphan；对被引用的，反查引用它的模板名（遍历各模板节点的 `definition_id`）。无 schema 变更。

### 1.4 前端改动
- `SettingsView.vue`：拉 `scopes` 与 `regexRules` 合并 → 计算 `{rule, scope, templateNames}`；按 (global 优先, 名称) 排序；渲染新列表；加搜索/隐藏停用状态。
- `RegexRuleEditor.vue`：保持手风琴；改为更紧凑的单行头（toggle + 名称 + 来源标签 + 两个 badge + 展开箭头）；展开区不变（pattern/replacement/范围/作用对象）。背景色与置灰由父级传入 props（`scope`、`enabled`）。
- 「同时只展开一行」：把 `expanded` 状态上提到 `SettingsView`（记录当前展开的 rule id），或父级用 `:open` 受控。
- i18n：新增「隐藏停用 / 搜索 / 全局 / 来自卡 {name}」等 4 语言键，过 `parity.test.ts`。

### 1.5 测试
- 后端：一条 orphan + 一条被模板引用 → `scopes` 分别返回 `global` / `template` + 正确卡名（`scoped_regex_rule_does_not_leak` 已覆盖应用面，这里覆盖查询面）。
- 前端：列表把全局排在局部前；停用项带置灰类；搜索过滤；同时只展开一行。

---

## 2. Regex 应用补全（后端为主）

补齐 `targets`（ai_output / user_input）×`scope`（display / prompt / both）矩阵。**只作用于聊天消息（user / assistant），不碰 world-info / prompt 段。`raw_content` 永不变。**

| 组合 | 含义 | 现状 |
|---|---|---|
| ai_output × display | 改助手消息**显示** | ✅ 已实现 |
| user_input × display | 改用户消息**显示** | 🆕 新增 |
| ai_output × prompt | 发给模型的**历史助手消息**被改写 | 🆕 新增（临时） |
| user_input × prompt | 发给模型的**用户消息**被改写 | 🆕 新增（临时） |

`scope == "both"` = 显示与 prompt 都改；`"display"` 仅显示；`"prompt"` 仅发送。

### 2.1 Core：参数化应用函数
将 `apply_regex_rules` 重构为按 (target, phase) 过滤：

```rust
pub enum RegexTarget { AiOutput, UserInput }
pub enum RegexPhase  { Display, Prompt }

/// 按 target+phase 过滤适用规则并依挂载顺序替换；无适用规则返回 None。
pub fn apply_regex_rules_for(
    text: &str, rules: &[Definition], target: RegexTarget, phase: RegexPhase,
) -> Option<String>;
```

过滤逻辑：跳过 `disabled`；`phase==Display` 要求 `scope ∈ {display, both}`，`phase==Prompt` 要求 `scope ∈ {prompt, both}`；`target` 须在 `targets` 内（`targets` 为空/缺省 = 广义，对两种 target 都适用——保持旧规则行为）。
保留旧 `apply_regex_rules` 作为 `apply_regex_rules_for(.., AiOutput, Display)` 的薄封装，或就地替换调用点。

### 2.2 Display 侧
- **助手消息**（已有）：`conversation.rs` 落库时用 `(AiOutput, Display)` 计算 `display_content`（经由 `resolve_display`，须与 HTML-patch 重建共存——见 §4 顺序）。
- **用户消息**（新增）：用户消息落库后用 `(UserInput, Display)` 计算其 `display_content`。`regex_rules` 在 `assemble_request` 返回后才有，故在拿到规则后**回填**用户消息的 `display_content`（一次 `update_message`），或将用户消息 display 计算下放到读侧（`messages` 列表/`edit_message`）。**选择：写侧回填**，与助手消息一致、读侧无需改。
- `messages.rs::edit_message`：按被编辑消息的 role 选 target 重算 display（目前固定 ai 路径）。

### 2.3 Prompt 侧（临时、不落库）
在 `assemble_request` 内、构造 `context: &[ChatMessage]` 之后、交给 `assemble_from_nodes` / `build_chat_messages` 之前，对 context 的**副本**按 role 应用 `(role→target, Prompt)` 规则：

```text
for m in context (copy):
    target = if m.role==Assistant {AiOutput} else if m.role==User {UserInput} else continue
    m.content = apply_regex_rules_for(m.content, &regex_rules, target, Prompt).unwrap_or(m.content)
```

- 世界书扫描窗口 `recent` 取自 context.content：**沿用原文扫描**（prompt-regex 仅改最终发给模型的消息，不改触发判定），避免触发行为被 regex 间接影响；在 spec 标注此决定。
- 存储层零改动；只影响本次请求的 `ChatRequest.messages`。

### 2.4 前端
`RegexRuleEditor` 的「范围」目前只暴露 显示/两侧（`scopeFlagsToMeta` 产出 `display|both`）。新增 **prompt-only** 选项，使范围成 **显示 / 两侧 / prompt** 三选一；`scopeFlagsToMeta` / `metaToRule` 相应支持 `scope:"prompt"`。作用对象（AI 输出 / 用户输入）勾选已存在。

### 2.5 测试
- Core：`apply_regex_rules_for` 的四组组合 + 广义 `targets` 空 + `disabled` + `scope` 过滤矩阵。
- conversation：用户消息得到 `(UserInput,Display)` 的 `display_content`；助手消息 display 与 HTML-patch 共存正确。
- assemble_request：prompt 侧规则改写了发给 provider 的 user/assistant 消息，而 `raw_content` 不变；`display`-only 规则不改 prompt；世界书触发用原文。

---

## 3. 变量 `<state_update>` 协议自动注入（后端）

### 3.1 触发
当会话的 effective 变量 schema **非空**时（`session_schema` 解析出 ≥1 个 VarDecl），在 `assemble_request` 注入一段 **AfterHistory** system 段——与 HTML-card patch 注入同一机制（`plan.segments.push(PromptSegment{ placement: AfterHistory, .. })`），两段可并存。Anthropic 适配器会把所有 System 段并入顶层 `system`。

### 3.2 内容（英文，面向模型）
新增构造函数（放 `state.rs` 或新 `var_protocol` 模块）：

```rust
/// schema + 当前值 → 注入用的协议指令。schema 空时返回 None。
pub fn build_state_protocol_instruction(schema: &[VarDecl], state: &Value) -> Option<String>;
```

产出包含两部分：
1. **当前变量清单**：逐条 `- <name> (<type>) = <current value>`（值取自 `state` / branch_state，缺省用 schema initial）。
2. **协议说明**：`<state_update action="…" key="…" value="…"/>` 自闭合标签，列 6 个动作及语义：
   - `SET key value` / `ADD`(数值加) / `SUB`(数值减) / `TOGGLE`(布尔翻转，无 value) / `APPEND`(列表/文本追加) / `REMOVE`(列表移除)。
   - 强调：标签内联在正文里即可，引擎会折叠进状态并从显示中剥离。

### 3.3 接入
`assemble_request` 已有 `state`（branch_state）与可解析的 schema（`session_schema`/`resolve_schema`）。在 push HTML-patch 段附近，若 `build_state_protocol_instruction(&schema, state)` 为 `Some`，push 一段 AfterHistory。

### 3.4 测试
- 单元：schema 空 → None；schema 有 hp:number=95、alarmed:bool=false → 指令含变量清单与 6 动作。
- 集成（conversation）：声明变量的会话，发送请求里 System 段含 `<state_update`；无变量会话不注入。

---

## 4. 跨特性交互与注意

- **display_content 计算顺序**（助手消息）：HTML-card 重建优先（`resolve_display` 已是「先 patch 重建，否则 regex/strip」）。补 #2 后，"否则"分支用 `(AiOutput, Display)`。即 patch 重建 > regex 显示替换，不冲突。
- **注入段并存**：HTML-patch 指令、变量协议指令都可能作为 AfterHistory 段同时存在；合并相邻 System 后进入 `system`。顺序：变量协议、patch 指令各自独立，无依赖。
- **向后兼容**：`targets` 空 = 广义（旧规则照旧广播）；`scope` 缺省 `display`。
- **性能**：display 侧每条消息一次正则串；prompt 侧每次请求对 context 副本一遍。规则数与历史长度都有限，可接受。

---

## 5. 实现顺序与提交

各项独立、可分别审/合：

1. **#3 变量协议注入**（最小、纯后端）。
2. **#2 Regex 应用补全**（core 参数化 + conversation/assemble 接入 + 前端范围三态）。
3. **#1 Regex 管理 UI**（scopes 端点 + SettingsView/RegexRuleEditor 重构 + i18n）。

每步：`cargo test --workspace` + `vue-tsc` + `vitest` 绿，再独立提交（不 push，除非另行要求）。

---

## 6. 不在本 spec：卡/预设分离（#4，已拆出）

**根因**：单一 `template` 把 ST 的"角色卡"（人设/开场白/世界书/卡上变量/regex）与"prompt 预设"（组装结构）塞进同一节点树，导致"只换 prompt、留着这张卡"无法实现，"换 template"也语义含糊（无损换 prompt vs 重开换卡分不开）。当前**没有**给已有会话换模板的接口（`patch_session` 仅改名/头像；`tree_to_preset` 只是导出序列化器，模型里无独立 preset 实体）。

**两个候选方向**（留待专门设计定夺）：
- **X · 彻底分离**：会话绑定 `card_id` + `preset_id`，effective 树 = 卡树 ⊕ 预设树。ST 对齐、长远干净；改动最大（数据模型 + 迁移 + 组装合并 + 导入区分 + UI + 播种归属）。
- **Z · 模板可组合**：会话引用一组有序 template 合并；约定其一为卡、其一为预设。复用现有结构、改动中等；合并/播种归属/列表 UI 需定义。

**绑定生命周期（供 #4 设计参考）**：prompt/world/scoped-regex/变量 schema 实时跟随 effective 树；first_message 与变量初始值在会话创建时一次性 seed（不回溯改写）；materialize 后整棵树冻结为会话自有副本。
