# ST 角色卡 → 设定集 导入(数据层)设计

> 来源:一次关于「支持复杂 SillyTavern(ST/酒馆)角色卡」的 brainstorming。本 spec 只覆盖**数据层 + 导入翻译**这一 slice;HTML 消息渲染、ST 预设导入、富 regex 引擎(prompt 侧应用)各为后续独立 slice。

## 1. 核心立场(brainstorm 结论)

- **Shirita 有自己的原生理想模型,不是「兼容 ST」**:用户管理的是**设定集**——角色卡与世界书本就揉在一起,全部是 `Definition`,靠现有的 **2 层文件夹树(`Template`)**组织。「角色」不是特权实体,只是设定集里的一个(或几个)定义。**一切皆定义。**
- **现有文件夹/定义设计就是目标形态**:不引入新的「文件夹」概念、不加「Character」实体、不做二层以上嵌套。沿用刚加固的 2 层节点树约束。
- **酒馆格式语义上无法直接 1:1 导入** → 导入 = 把一张卡**熔进**设定集的**单向、有损**翻译。不追求回出口(round-trip),不保留 ST 槽位结构。
- **卡与预设是两个独立 ST 文件,本就分开导**,且**多数卡不带预设**。故本 spec **只做「角色卡 → 设定集」**;ST 预设导入是另一个 slice。
- **能力边界 = regex + HTML 渲染**:不做 MVU/变量框架桥接,不支持 `tavern_helper`(JS 运行时)。这些字段原样存 `meta`,不丢但不解释。

完成判据:`cargo test` 全绿(含下列新增单测),`examples/怪谈社.json` 与 `examples/密教模拟器.json` 能导入成一个可直接新建会话开聊的设定集(首消息为其 HTML/文本开场白,备选作 swipes;各自的 regex 只作用于本设定集会话)。

## 2. 用真实数据收敛范围

`examples/` 两张 `chara_card_v3` 的**非空**字段:

- 密教模拟器:`description`(大 JSON)、`first_mes`(`<start>`)、`alternate_greetings`×2、`character_book`×65、`extensions.regex_scripts`×2
- 怪谈社:`first_mes`(整页 HTML)、`alternate_greetings`×2、`character_book`×22、`extensions.regex_scripts`×5

`personality / scenario / mes_example / system_prompt / post_history_instructions / extensions.depth_prompt.prompt` 在两张卡里**全为空**。结论:**只为四样东西造机制**——`description`、`first_mes(+alt)`、`character_book`、`regex_scripts`。其余空字段:有内容则拼进 char 正文,否则忽略;`extensions` 整体留一份在 char 定义的 `meta.st_raw` 里(调试用,不解释)。

## 3. 概念模型(不加新概念)

| 概念 | 是什么 | 本 spec 改动 |
|---|---|---|
| **Definition** | 唯一内容单元。type:char / persona / world / **first_message(新 RESERVED 类型)** / regex_rule / … | 加一个保留类型 |
| **Template** | 一棵 2 层 prompt 节点树。**它本身就是「设定集/分组」** | 导入卡时建一个 |
| **PromptNode** | Folder(tag) / Ref(→definition) / History。2 层不变 | 新增「非渲染 ref」语义 |
| **Session** | 一次会话 = 选一个 template + 消息树 | 创建时按 first_message 定义 seed 开场白 |

**设定集 = 一个 Template + 它节点树引用的那批 Definition。** 没有「库内文件夹」「Character 实体」这类新东西。

## 4. 数据模型改动

### 4.1 新保留类型 `first_message`

- `def_type::RESERVED` 由 `["prompt","regex_rule","tool"]` → `["prompt","regex_rule","tool","first_message"]`。
- 一个 `first_message` 定义:`content` = 主开场白(`first_mes`);`meta.alternate_greetings` = `[String]`(备选开场白)。
- 它**可被 ref 节点引用**(保留类型不做容器 tag,但能被引用,与 regex_rule 同),用于挂进设定集 template 并被会话 seed 取用。
- 创作期校验:`definitions.rs` 已有 `validate_type`——`is_reserved("first_message")` 返回 true,自动放行;无需额外分支。

### 4.2 「非渲染 ref」语义

`assemble_from_nodes`(`assembly.rs`)遍历树产出 prompt 段时,**遇到指向以下类型定义的 ref 一律跳过(不产 prompt 段),无论它在根还是在 folder 内**:`regex_rule`、`first_message`。它们由各自子系统消费:

- `regex_rule` ref → 收集进「本设定集生效的 regex 规则」(见 4.3)。
- `first_message` ref → 由会话创建时的 seeder 取用(见 4.4)。

(`char/persona/world/prompt` 等仍正常渲染成段,行为不变。)folder 若其子 ref 全为非渲染类型,则该 folder 不产段(避免空 `<tag></tag>`);导入时(§5.1)这类非渲染 ref 直接挂根以规避该情形。

### 4.3 regex 作用域:从全局 → 按 template 引用(brainstorm 选项 A)

**现状**:`conversation.rs::assemble_request`(`:134-143`)用 `list_definitions().filter(def_type=="regex_rule")` 取**全局所有** regex_rule,导致多设定集互相污染。

**改为**:assembly 遍历**本会话 template 节点树**时,把遇到的 `regex_rule` ref 指向的定义收集为本次生效规则集(随 `assemble_from_nodes` 的同一次遍历产出,返回给 `assemble_request`)。只应用本设定集引用的 regex。

附带在 regex 应用处(`apply_regex_rules`)honor 两个 meta 开关:

- `meta.disabled == true` → 跳过该规则。
- `meta.scope`:`"display"`(默认)只作用于 `display_content`;`"prompt"`/`"both"` 的 prompt 侧应用**本 spec 不实现**(留给富 regex 引擎 slice),但 meta 照常存储;本 spec 只对 `scope ∈ {display, both, 缺省}` 的规则做现有的 display 替换。

### 4.4 regex_rule meta 丰富化(只存,渲染留后)

ST `regex_scripts[i]` → `regex_rule` 定义映射:

| ST 字段 | regex_rule | 本 spec 是否生效 |
|---|---|---|
| `scriptName` | `name` | 是 |
| `findRegex` | `meta.pattern` | 是(display 替换) |
| `replaceString` | `meta.replacement` | 是 |
| `disabled` | `meta.disabled` | 是(跳过) |
| `markdownOnly`(仅显示) / `promptOnly`(仅提示) | `meta.scope` = `display` / `prompt` / `both` | display 生效;prompt 留后 |
| `placement`([1]用户输入/[2]AI输出) | `meta.targets` = `["user_input"]`/`["ai_output"]` | 只存;当前仅 AI 输出 display 生效 |
| `minDepth`/`maxDepth` | `meta.min_depth`/`max_depth` | 只存 |
| `trimStrings`/`substituteRegex`/`runOnEdit` | `meta.st_raw` 透传 | 只存 |

### 4.5 会话创建 seed 首消息

`routes/sessions.rs::create_session`:设置 `template_id` 并 seed `current_state` 后,新增:

1. 取该 session 生效的 template 节点(引用模板节点,或会话自身已物化节点)。
2. 找指向 `first_message` 定义的 ref;取其 `content`(主)+ `meta.alternate_greetings`(备选),按 `render_vars` 渲染变量。
3. **seed 成「根级 assistant 消息 + swipes」**:为主开场白 + 每条备选各建一条 `role=assistant`、`parent_id=None` 的消息,互为兄弟(= swipes);`active_leaf_id` 指向主开场白那条。`display_content` 经本设定集 display 侧 regex 求得(与生成消息一致),`raw_content` = 渲染后的开场白原文。
4. 无 `first_message` ref → 不 seed,保持现状(空会话)。

> 设计点:消息树需支持「根级 assistant 消息(其前无 user)」。`active_path`/分支逻辑须对此成立——纳入测试(见 §7)。

## 5. 导入翻译:卡 → 设定集

### 5.1 core 适配器(`adapters/charcard.rs` 重写)

新签名,产出整个设定集的逻辑结构(只造内存对象 + mint uuid,不落库):

```rust
pub struct LoreSet {
    pub template: Template,
    pub definitions: Vec<Definition>,   // char + world×N + first_message + regex×M
    pub nodes: Vec<PromptNode>,         // 2 层:folder + ref + history
}
pub fn charcard_to_loreset(card: &serde_json::Value) -> LoreSet;
```

翻译规则:

- `data`(v2/v3)优先,缺失回退顶层(v1)。
- **char 定义**:`name` = 卡名;`content` = `description`;若 `description` 空而 `personality`/`scenario` 非空,则把它们以简单标注拼进 content(「凑合」);仍全空则 content 留空。`meta.st_raw` = 原始 `extensions`(透传)。
- **world 定义 ×N**:`character_book` 走现有 `worldinfo_to_defs`(已实现,不动)。
- **first_message 定义**:`content` = `first_mes`;`meta.alternate_greetings` = `alternate_greetings`。`first_mes` 与 alt 全空 → 不产此定义。
- **regex_rule 定义 ×M**:`extensions.regex_scripts[*]` 按 §4.4 映射;空数组 → 无。
- **Template**:`name` = 卡名;2 层节点树(均合法 2 层:folder 挂根、ref 挂根或挂同 owner 根 folder):
  - `folder "char"` → ref char(渲染)
  - `folder "world"` → refs world×N(渲染,仅当非空)
  - **根级** refs regex×M(非渲染,挂根避免空折叠;仅当非空)
  - **根级** ref first_message(非渲染开场白;仅当存在)
  - `history` 节点(开场白之后即正常历史)

### 5.2 web 路由(`routes/import_export.rs`)

- `import_charcard` / `import`(PNG 内嵌或 JSON)→ 调 `charcard_to_loreset`,再:
  - 定义按现有 `persist_defs` 的 `name+def_type` 去重 + `on_conflict` 落库;
  - template + nodes 经 `create_template`/`create_node` 落库(参考既有 `import_template_bundle` 的 def_map 重映射:ref 节点的 `definition_id` 要换成去重后实际入库的 id)。
- summary 增加 `template` 计数项(已有 item 机制)。
- PNG 解析(`save_png_asset` / 读 `chara` tEXt)沿用现有路径。

## 6. 影响面与文件清单

- `shirita-core/src/models/def_type.rs`:`RESERVED` 加 `first_message`。
- `shirita-core/src/adapters/charcard.rs`:重写为 `charcard_to_loreset` + `LoreSet`;删除 `def_to_charcard`(回出口不在路线图)及其测试,或保留但标注 deprecated(取删除,YAGNI)。
- `shirita-core/src/adapters/worldinfo.rs`:不动(`worldinfo_to_defs` 复用)。`defs_to_worldinfo` 同属回出口,一并删。
- `shirita-core/src/assembly.rs`:`assemble_from_nodes` 跳过 `regex_rule`/`first_message` ref 并收集 regex 规则集;`apply_regex_rules` honor `disabled`/`scope`。
- `shirita-core/src/conversation.rs`:`assemble_request` 不再全局 filter regex,改用 assembly 收集到的设定集 regex 集。
- `shirita-web/src/routes/sessions.rs`:`create_session` 增 first_message seeding。
- `shirita-web/src/routes/import_export.rs`:charcard 导入改造为落「定义 + template + nodes」。
- `shirita-core/src/storage/*`:若 seeding 需批量建消息,复用现有 `create_message`;预计无 schema 变更。

## 7. 测试策略

- **core 适配**(`adapters/charcard.rs`):`examples/` 两卡(或精简夹具)→ `charcard_to_loreset` 产出:char/world×N/first_message/regex×M 数目正确;template 2 层结构正确;ref `definition_id` 自洽。
- **非渲染 ref**(`assembly.rs`):含 regex_rule + first_message ref 的树,装配产出的 prompt 段**不含**这俩;regex 规则集只含树里引用的。
- **regex 作用域**(`conversation.rs`):两个设定集各带不同 regex,会话 A 只应用 A 的;`disabled` 规则被跳过。
- **首消息 seed**(`routes/sessions.rs` 集成测):建带 first_message 的设定集 → 新建会话 → 消息树有 1 主 + N 备选的根级 assistant 兄弟;`active_leaf` 指主;`active_path` 对「根级 assistant」成立。
- **导入端到端**(web 集成测):POST 怪谈社 JSON → 200;库里出现 char + 22 world + 5 regex + 1 first_message + 1 template;据此新建会话首消息 = 其 HTML 开场白原文。
- **去重/冲突**:重复导入同卡,`on_conflict=skip` 不产重复定义。
- 既有测试回归:`worldinfo` 不变;`template_assembly_test` 等仍绿(注意 regex 全局→引用的行为变更需同步调整相关单测)。

## 8. 取舍与不做(本 slice)

- **不做** HTML 渲染本身(前端 sandbox)——下一个 slice;本 spec 只保证 `display_content` 能装 HTML 文本。
- **不做** ST 预设导入——独立 slice(卡多数不带预设,且 ST 中是独立文件)。
- **不做** prompt 侧 regex 应用、`depth_prompt` 深度注入、`mes_example`/`system_prompt`/`post_history` 的装配机制(真实数据里皆空)——meta 存着,留后。
- **不做** MVU/变量框架桥接、`tavern_helper`——明确不支持,原样存 `meta.st_raw`。
- **不做** 回出口(Shirita→ST):删 `def_to_charcard`/`defs_to_worldinfo`。导入是单向有损翻译。

## 9. 后续 slice(本 spec 之外,顺序参考)

1. **富 regex 引擎**:prompt 侧应用、`placement`/`targets`、`min/max_depth`、`{{macro}}` 替换(`substituteRegex`)。
2. **HTML 消息渲染**:`MessageItem.vue` 把 `display_content` 在 sandbox iframe 渲染;依赖 slice 1 的 display 产物。
3. **ST 预设导入**:独立文件 → 一棵以 prompt 结构为主的 template。
