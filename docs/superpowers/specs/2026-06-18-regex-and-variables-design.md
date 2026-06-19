# Shirita — Regex 完善、管理 UI 与协议指令定义化（设计 Spec）

> 状态：经 brainstorm 与用户逐条确认 + 修订，待 review。
> 范围：**#1** Regex 管理 UI（紧凑 + 作用域可视 + 失效标记）、**#2** Regex 应用补全（user_input + prompt 侧，Display 侧改读侧计算）、**#3** 变量 `<state_update>` 协议**定义化 + 自动注入**，并顺带把已落地的 **HTML-card patch 指令**统一到同一"协议定义"机制。
> **不在本 spec**：卡/预设分离（#4）已拆为独立设计（见末节）。
> 上游：M5 变量状态（`state.rs`）、Prompt 树 v2 / 世界书、ST 角色卡导入（`charcard.rs`）、同会话 HTML-card patch（commit `b7bd93f`）。

---

## 0. 背景与现状（代码实证）

- `regex_rule` 是一种 `Definition`，`meta = { pattern, replacement, disabled, scope, targets }`：
  - `scope: "display" | "both" | "prompt"`（WHERE：改显示 / 两者 / 改发送）。
  - `targets: ("ai_output" | "user_input")[]`（WHICH：作用在哪一侧消息；空数组 = 广义，向后兼容旧规则）。
- `assembly::apply_regex_rules` **目前只实现了 `ai_output × display`**，产物写入助手消息的 `display_content`（`conversation.rs` 落库时算，`messages.rs::edit_message` 手动编辑时重算）。`raw_content` 永远是原文。
- **Regex 引擎**：现用 Rust `regex` crate。为线性时间放弃了 **lookaround `(?=)(?!)(?<=)(?<!)` 与反向引用 `\1`**——而 ST 的 `regex_scripts` 大量依赖这些，导入后会有一批静默失效。
- **作用域（hybrid，`assemble_request`）**：orphan（不被任何节点引用）= 全局；被本会话 effective 树引用 = 局部（如 ST 卡导入）。二者互斥。
- **变量**：`state.rs` 只**解析** `<state_update action="…" key="…" value="…"/>`（动作 SET/ADD/SUB/TOGGLE/APPEND/REMOVE）折叠进 `snapshot_state`；引擎从不向模型注入协议说明。
- **HTML-card patch（已落地 b7bd93f）**：指令文本目前**硬编码**在 `html_patch::INSTRUCTION` 常量，`assemble_request` 在对话含卡时直接 push。
- 三项均**不依赖 #4**。

---

## 1. Regex 引擎：切换到 `fancy-regex`（#1/#2 共用基础）

为吃下 ST 兼容,把 **`regex_rule` 的编译/应用/校验**从 `regex` 换到 **`fancy-regex`**（在 `regex` 之上以回溯补回 lookaround + 反向引用；无 fancy 特性时仍走线性快路径）。

- 影响面：`assembly::apply_regex_rules*`（应用）、`assembly::is_valid_regex`（校验，`definitions.rs` 创作期用）。
- **不动**引擎内部正则：`state::parse_state_updates`/`strip_state_tags`、`assembly::render_vars`（`{{var}}`）仍用 `regex` crate（无需 fancy 特性）。
- ReDoS：规则系卡/用户编写、跑在**有限长度**聊天文本上；用 `fancy-regex` 的 `backtrack_limit` 兜底，本地单人 RP 工具可接受。
- 依赖：`shirita-core/Cargo.toml` 增 `fancy-regex`。
- API 适配：`fancy_regex::Regex::new` 返回 `Result`；`replace_all` 签名与 `regex` 略异（plan 阶段处理）。
- **失效可见**：仍可能有规则编译失败（其它语法错误）。校验函数返回的错误经 §3-UI 暴露（见 §3 的 `pattern_error`）。

---

## 2. Regex 应用补全（#2）

补齐 `targets`（ai_output / user_input）×`scope`（display / prompt / both）矩阵。**只作用于聊天消息（user/assistant），不碰 world-info/prompt 段；`raw_content` 永不变。**

| 组合 | 含义 | 现状 |
|---|---|---|
| ai_output × display | 改助手消息**显示** | 有（迁读侧） |
| user_input × display | 改用户消息**显示** | 🆕 |
| ai_output × prompt | 发给模型的**历史助手消息**被改写 | 🆕（临时） |
| user_input × prompt | 发给模型的**用户消息**被改写 | 🆕（临时） |

`scope=="both"` = 显示与 prompt 都改；`"display"` 仅显示；`"prompt"` 仅发送。

### 2.1 Core：参数化应用函数
```rust
pub enum RegexTarget { AiOutput, UserInput }
pub enum RegexPhase  { Display, Prompt }
pub fn apply_regex_rules_for(
    text: &str, rules: &[Definition], target: RegexTarget, phase: RegexPhase,
) -> Option<String>;
```
过滤：跳过 `disabled`；`Display` 要 `scope∈{display,both}`，`Prompt` 要 `scope∈{prompt,both}`；`target` 须在 `targets` 内（空/缺省 = 广义）。用 `fancy-regex` 编译，非法 pattern 仅 warn 跳过。

### 2.2 Display 侧 = **读侧即时计算**（采纳用户建议）
- 展示文本在**读侧**算,不落库：规则一改,下次加载历史 UI **瞬间全局刷新**;每次加载记录有限 + Rust 正则微秒级,无卡顿。
- 落点：`messages.rs` 的 `list_messages`（及任何供前端展示消息的端点）——对每条消息,取 `base = stored display_content ?? raw_content`,按 role 选 target 套 `(target, Display)` 规则,得 `served_display` 返回前端（覆写响应里的 `display_content` 字段；前端 `display_content ?? raw_content` 不变）。
- **写侧 `display_content` 只保留与规则无关的变换**：剥 `<state_update>` 标签、HTML-card 重建。无外部依赖、不过期,留存即可;读侧在其上叠加 regex。
- **守卫**：若 `base` 是整篇 HTML 文档（卡,`html_patch::is_html_document`），跳过 Display 正则（卡不是 RP 散文,避免被规则改坏）。
- `edit_message` 不再算 regex（读侧负责）;只存 raw + 规则无关的 display。

### 2.3 Prompt 侧（请求期、临时、不落库）
在 `assemble_request` 构造 `context` 之后、交给组装之前,对 context 副本按 role 套 `(role→target, Prompt)` 规则:
```text
Assistant→AiOutput, User→UserInput;
m.content = apply_regex_rules_for(m.content, rules, target, Prompt).unwrap_or(m.content)
```
- 世界书扫描窗口 `recent` **沿用原文**（prompt-regex 仅改最终发送内容,不影响触发判定）。
- 存储零改动。

### 2.4 共享：effective regex rules
抽出 `fn effective_regex_rules(storage, session) -> Vec<Definition>`（全局 orphan + 本会话 effective 树引用的 scoped），供 §2.2 读侧 与 §2.3 请求期 共用（`assemble_request` 现有逻辑移入）。

### 2.5 前端
`RegexRuleEditor` 的「范围」补 **prompt-only**,成 显示/两侧/prompt 三选一;`metaToRule`/`scopeFlagsToMeta` 支持 `scope:"prompt"`。作用对象勾选已存在。

### 2.6 测试
- Core：`apply_regex_rules_for` 四组合 + 广义 targets 空 + disabled + scope 矩阵 + 一个 lookaround pattern（验证 fancy-regex 生效）。
- 读侧：`list_messages` 对 user/assistant 分别套 Display 规则;HTML 卡消息跳过;改规则后再读结果变化;`raw_content`/stored display 不变。
- Prompt 侧：发给 provider 的 user/assistant 被改写而 `raw_content` 不变;display-only 规则不改 prompt;世界书触发按原文。

---

## 3. Regex 管理 UI（#1，前端 + 轻量后端查询）

### 3.1 问题
`SettingsView` 把全部 `regex_rule` 平铺成一摞带边框卡片,无作用域区分/置顶/搜索,导入多卡后混成一堆。

### 3.2 设计：紧凑 master-detail 列表
- **全局规则置顶**（背景色 A）;**局部规则**在下（背景色 B）,每行带**来源模板/卡名**标签。平铺,不折叠分组。
- **手风琴**:点行内联展开编辑,同时只展开一行。
- 每行右侧 mini-badge:**作用对象**（AI/用户）·**范围**（显示/两侧/prompt）。
- 停用规则**置灰**仍显示;顶部「隐藏停用」开关 + 搜索框。
- **失效标记**:`pattern_error` 非空的规则标红 + tooltip 显示错误（含 fancy-regex 仍编译失败的情况）。
- 「新建全局规则」保留（创建即 orphan）。

### 3.3 后端：作用域 + 校验查询
```
GET /api/regex-rules/scopes
→ [ { "id", "scope": "global"|"template", "template_names": [...],
      "pattern_error": null | "<fancy-regex 编译错误>" } ]
```
实现：`referenced_definition_ids()` 判 orphan;被引用的反查模板名;`pattern_error` = `fancy_regex::Regex::new(pattern).err()`。无 schema 变更。

### 3.4 前端改动
- `SettingsView`：拉 `scopes` + `regexRules` 合并,按 (global 优先, 名称) 排序,渲染新列表 + 搜索/隐藏停用状态;展开态上提（受控,单开）。
- `RegexRuleEditor`：紧凑单行头（toggle + 名 + 来源标签 + 两 badge + 失效标记 + 展开箭头）;展开区不变;`scope`/`enabled`/`pattern_error` 由父级传入。
- i18n：「隐藏停用 / 搜索 / 全局 / 来自 {name} / 规则失效:{err}」等键,过 `parity.test.ts`。

### 3.5 测试
- 后端：orphan→`global`、被引用→`template`+卡名;非法 pattern→`pattern_error` 非空。
- 前端：全局排局部前;停用置灰;失效标红;搜索过滤;单开。

---

## 4. 协议指令统一为 builtin「协议定义」（#3 + HTML-patch 迁移）

固定不变的协议说明不该硬编码在引擎里——做成**定义（数据）**,可见/可编辑/可导出,且仍由引擎按条件**自动注入**。统一 `<state_update>` 与 HTML-patch 两套指令到同一机制。

### 4.1 模型
- 新增**保留类型 `protocol`**（加入 `def_type::RESERVED`,与 `regex_rule`/`first_message`/`tool` 同列;代码常量,不入 def_types 表、不作树容器）。
- `protocol` 定义的 `meta.kind`：`"state_update" | "html_patch"`,标识引擎按哪个触发条件注入、是否追加动态内容。
- **种子**：新增 `seed::ensure_builtin_definitions`（幂等,固定已知 id,缺失才建）,在启动（`main.rs`,与 `ensure_default_template` 并列）种两条 builtin `protocol` 定义:
  - `state_update`：content = 状态协议说明（`<state_update action=… key=… value=…/>` + 6 动作语义）。
  - `html_patch`：content = 现 `html_patch::INSTRUCTION` 文本（该常量降级为种子默认值,不再在 `assemble_request` 直接使用）。

### 4.2 引擎注入（`assemble_request`）
取所有 `protocol` 定义,逐条按 `meta.kind` 评估触发,命中则把内容（+ 动态附加）作为 **AfterHistory** system 段 push（两段可并存,Anthropic 适配器合并入顶层 `system`）:
- `state_update`：触发 = 会话 effective schema 非空。动态附加 = **当前变量清单**（逐条 `- name (type) = 当前值`,引擎从 schema + branch_state 生成,不入定义）。
- `html_patch`：触发 = 对话含 HTML 卡（`is_html_document` 或含 patch 块,即现有逻辑）。无动态附加。

### 4.3 改动点
- `def_type::RESERVED` 加 `"protocol"`;`html_patch.rs` 移除 `assemble_request` 里对 `INSTRUCTION` 的直接 push（改走定义查询）;`INSTRUCTION` 作为种子默认文本保留（或移入 seeder）。
- 新增动态变量清单构造 `build_state_variables_block(schema, state) -> String`。
- `assemble_request`：一处统一的 protocol 注入循环替代原 HTML-patch push。

### 4.4 测试
- 种子幂等:重复调用不产生重复 protocol 定义。
- `state_update`:有变量会话→system 段含协议 + 变量清单;无变量→不注入。
- `html_patch`:对话含卡→注入（迁移后行为与 b7bd93f 一致,沿用既有测试断言）。
- 两者可并存。

---

## 5. 跨特性交互与注意
- **Display 计算链**（读侧）：`served_display = regex_display( strip+HTML重建后的 base )`,HTML 卡 base 跳过 regex（§2.2 守卫）。写侧 `display_content` 不再含 regex。
- **注入段并存**：`state_update` + `html_patch` 两 protocol 段 + 未来其它,皆 AfterHistory,合并入 `system`,彼此无依赖。
- **向后兼容**：`targets` 空 = 广义;`scope` 缺省 `display`;迁移后旧会话的 HTML-patch 行为不变（种子提供同文本）。
- **性能**：读侧每条消息一次正则串;请求期对 context 一遍;规则数 + 历史长度有限,可接受。

---

## 6. 实现顺序与提交（各独立审/合）
1. **fancy-regex 引擎切换**（§1，小、隔离;含 lookaround 测试）。
2. **协议定义化 #3 + HTML-patch 迁移**（§4：保留类型 + 种子 + 统一注入循环）。
3. **#2 Regex 应用补全**（§2：core 参数化 + 读侧 Display + 请求期 Prompt + 共享 effective rules + 前端范围三态）。
4. **#1 Regex 管理 UI**（§3：scopes/校验端点 + SettingsView/RegexRuleEditor 重构 + i18n）。

每步：`cargo test --workspace` + `vue-tsc` + `vitest` 绿,独立提交（不 push,除非另行要求）。

---

## 7. 不在本 spec：卡/预设分离（#4，已拆出）
**根因**：单一 `template` 把 ST 的"角色卡"与"prompt 预设"塞进同一节点树,"只换 prompt、留着这张卡"无法实现;当前无给已有会话换模板的接口（`patch_session` 仅改名/头像;`tree_to_preset` 只是导出序列化器,模型无独立 preset 实体）。
**候选方向**（留待专门设计）：**X** 彻底分离（`card_id`+`preset_id`,ST 对齐,改动最大）/ **Z** 模板可组合（有序 template 列表合并,中等改动）。
**绑定生命周期**（供 #4 参考）：prompt/world/scoped-regex/变量 schema 实时跟随 effective 树;first_message 与变量初始值在会话创建时一次性 seed（不回溯改写）;materialize 后整棵树冻结为会话自有副本。
