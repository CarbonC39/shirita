# Shirita M3 — 前端主体设计 (Frontend Design Spec)

> 状态：UX 经可视化 brainstorm 与用户逐屏确认；**后端数据模型为本 spec 的提议，待 review**（用户明确把后端设计交由本方案提出）。
> 本文档**取代路线图 §5 的 M3 草案**（原"三栏 shadcn 抽屉布局"）。设计方向已由用户重定为：**极简、消息应用（通讯录）隐喻、单列居中、移动优先可读**。
> 上游：`docs/superpowers/specs/2026-06-12-shirita-roadmap-design.md`、`tdd.md`、已完成的 M0–M2 实现。
> 可视化 mockup 留档：`.superpowers/brainstorm/`（gitignore，不入库）；关键决策另存于会话记忆 `shirita-m3-design`。

---

## 1. 范围与定位

M3 铺设前端主体（Vue 3），覆盖当前已有后端能力（聊天 / 会话 / 定义 / 组装 / 资源 / regex）的完整 UI，并据此**提议一组后端数据模型升级**（模板节点树、节点引用定义、写时复制局部覆盖、设置存储、provider 列表）。

- 前端仅视图层；业务逻辑、组装、token 计算仍在 `shirita-core`。
- 首发平台 Web；通信层预留 Tauri-IPC 适配位（M8）。
- 分支/分叉的**持久化**属 M4、变量面板属 M5、自动总结属 M6 —— M3 只铺设其 UI 占位或最小可用版本（见 §10）。

---

## 2. 设计语言

- **极简**：单列居中，桌面两侧留白，移动端收窄留白即可（同一套结构）。无多余装饰。
- **隐喻**：通讯录 / 消息应用（聊天列表像联系人，新建对话像"添加联系人"）。
- **无弹窗原则**：除"确认类"对话框外，一律用**页面跳转 + 内联展开**。唯一例外：编辑框的**全屏编辑覆盖层**（用户明确要）。
- **图标**：统一用 Lucide（`lucide-vue-next`），不用 emoji。
- **i18n**：从一开始多语言；**默认英文**，中文后续加（见记忆 `shirita-frontend-i18n`）。所有文案可翻译；**避免固定宽度内联标签**（文本输入用上方堆叠标签；开关 / 分段 / 下拉才左右排）。
- **外壳**：顶部居中三图标（chat / book / settings），图标**正下方**一条约 170px 短分隔线（非通栏）；左上角 logo 图标（无文字），进入子页时其旁出现面包屑；当前页用颜色加深表示（无圆点）。
- **色板（用户提议 2026-06-13，可调）**：`#F8F7F6` 暖白→背景/surface；`#459797` 青绿→**主色**（主按钮/当前导航/发送键/新建气泡/启用态）；`#F2A7A4` 珊瑚→暖强调（用户气泡/高亮）；`#8ED2EB` 天蓝→冷强调（助手头像底/info）；`#9F8391` 灰紫→次级文字/边框/非活跃。正文主色仍保留近黑以保可读性，青绿只做交互主色。落地为 Tailwind 设计 token / CSS 变量。

---

## 3. 技术栈

- **Vue 3 + Vite + TypeScript**
- **Tailwind CSS**（设计 token：中性灰阶 + `#111` 主强调；圆角 8–16；细线 `#e4e4e7/#ececec`）
- **Pinia**（仅 UI / 当前会话 / 流式缓存 / 设置缓存，不做业务持久化）
- **vue-router**
- **lucide-vue-next**
- 组件自建为主（headless 行为可借鉴 shadcn-vue/Radix 原语，但**自定义样式**，不照搬 shadcn 外观）。
- 通信：`fetch` + SSE（流式）封装为 `api` 客户端，预留 Tauri-IPC 实现。

> 与路线图差异：原 M3 写"三栏 + shadcn-vue + 右栏上下文面板"。本 spec 改为单列居中、自建极简组件；右栏 token 面板并入设置/对话选项后续处理。

---

## 4. 屏幕清单（逐屏）

### 4.1 外壳 Shell（全局）
顶部：左 logo（顶层 section 仅 logo；子页 logo 旁加面包屑）、中三图标导航、其下短分隔线。内容区单列居中（桌面 `max-width` 约 480–600）。

### 4.2 首页 / 聊天列表
居中卡片列；每张卡片：左头像、右上名称 + 时间、右下最后一条消息截断、右侧固定槽位的绿色未读点（已读留空槽以对齐时间）。右下角悬浮**新建按钮 = 实心消息气泡**（尾巴在右下，含加号），落在居中列右下角。点卡片 → 对话详情；点气泡 → 新建第 1 步。

### 4.3 新建对话 · 第 1 步（基础信息，"像添加联系人"）
整页跳转。面包屑 `Chat / New`。**只有头像 + 名称**：
- 头像：圆形占位，右下角浮一枚相机图标（不用 `+`）。点击 → 在**下方内联展开头像库**（已有头像横排 + 末尾"上传新图"虚线圈）。背后维护一个**头像库**。
- 名称：与输入框合并（combobox 风格的单行；i18n 友好实现见 §2）。
- 主按钮自适应：名称空 = **Skip**，已填 = **Next**（无箭头）。两者都进第 2 步。

### 4.4 新建对话 · 第 2 步（Prompt 模板节点树）—— 完成即创建对话
面包屑 `Chat / New / Prompt`。顶部模板选择器 + 四基础操作；下方节点树（详见 §5）。底部 **Create conversation**（**本页完成才真正创建会话**）。

### 4.5 对话详情
消息应用式，居中列。两种可切换渲染：
- **Bubble**：助手左（头像 + 浅气泡）、用户右（深气泡）。
- **Flat（非气泡）**：全宽行 + 头像/名称抬头 + 细分隔线（适合长篇 RP）。
- **无时间戳**。
- 每条 AI 回合操作：`‹ n/m ›` swipe（切换重试回合）、regenerate、**fork（分支）**、copy、edit（hover 出现）。
- 流式：最后一条助手消息带光标。
- 底部固定 composer：`+`（附件，留口子）+ 输入框 + 深色圆形发送（上箭头）。
- 顶部右侧 `⋯` = 单对话选项（§4.9）。

### 4.6 书本（定义 / 模板编辑器）
book 图标进入，顶层 section（左仅 logo）。**无整页卡片，分 section**：
- 顶部：模板 combobox + 四基础操作（import / export / duplicate / delete；New 收进下拉首项）+ 低调 **Saved**（自动保存，无"创建"按钮）。
- 中部：选中模板的**节点树（卡片包裹）**，与第 2 步同一组件；节点展开 = 内联可编辑文本框（右上角全屏按钮）。
- 底部 **Definition** section：一个 combobox **合并"搜索已有 / 命名新建"**（首项 New；展开后的下拉面板顶部才放搜索框，字段本身不放搜索图标）+ 四基础操作；下接 Type 选择 + 内容编辑（全屏按钮）+ Save。
- **作用域语义**：在书本里编辑 = 改**全局**定义；对话内编辑 = 默认**局部覆盖**，并显示「覆盖全局 / 重置为全局」（§6 写时复制）。

### 4.7 设置
gear 图标进入。无卡片，section + 细线：
- **Provider**（仅对话补全，不含绘图）：Source 下拉（**SillyTavern 式完整列表**：OpenAI / Anthropic / Google / OpenRouter / Mistral / DeepSeek / Groq / xAI / Cohere / Together / Perplexity / Custom…，选了自动填 Base URL）、Base URL、API key（显隐）+ **Test connection**（带状态）、Model、Stream 开关。
- **Generation**（全局）：Temperature / Top P / Frequency penalty / Presence penalty 滑块 + Max response tokens。
- **Appearance**：Message style（Bubble/Flat）、Theme（Light/Dark/System）、**Custom CSS** 编辑框（深色等宽 + 全屏）。
- **Regex**：规则列表（开关 + 名称；展开 = Find/Replace + "Apply to" 作用域 `AI output / User input / Display only`）+ import/export + Add rule。映射 M2 `regex_rule`。
- **Language**（English / 中文）、**About**（版本、Export/Import all data）。

### 4.8 头像库（文字描述，未单独 mock）
独立选择面板 / 内联展开：历史头像网格 + 上传新图（走 assets 上传）。用于会话头像与（后续）角色定义头像。

### 4.9 单对话选项（文字描述，未单独 mock）
从对话详情 `⋯` 进入的聚焦页：改名 / 换头像 / 编辑本对话挂载的模板与定义（含局部覆盖管理）/ 导出 / 删除。

---

## 5. 核心组件：Prompt 模板节点树（PromptTree）

第 2 步与书本共用一份组件。模型："Godot node" 式的有序树。

- **节点种类**
  - **folder / type 节点**（如 `char`、`world`）：纯容器，无自身文本；渲染为 **XML 标签** `<char>…children…</char>`（标签名 = 节点名）。可嵌套；UI 不显示标签徽标（弱化技术感），只用首行缩进 + 卡片容器。
  - **ref / prompt 节点**：引用一条**库定义**（`definition_id`），渲染该定义内容（经局部覆盖 + `{{var}}`）。展开 = 内联编辑其内容（全屏按钮）。
- **每节点**：启用勾选框、拖动手柄（重排 / 改变嵌套）、展开 chevron。
- **上下文 `+`（每个容器与根各一个）**：内联展开面板，顺序 = **搜索框 → 该类型已有定义的部分列表（点击=加入，购物车式，可多选，新增项落在下方）→ New → Other type**。即节点是从定义库**复用**而来。
- **组装**：遍历节点（按 `sort_order`，跳过 disabled）；folder → `<tag>` + 递归子节点 + `</tag>`；ref → 渲染定义内容。**这是 M2「按类型分组包 XML」的泛化**——从自动分组升级为用户显式树。

---

## 6. 后端数据模型变更（提议，待 review）

> 用户未预设后端，以下为本 spec 提议。目标：模板=树、节点引用定义、写时复制、设置持久化。尽量复用 M0–M2 既有表。

### 6.1 新增 / 变更表
- **`templates(id, name, meta JSON, created_at, updated_at)`** — 模板主记录。
- **`prompt_nodes(id, owner_kind TEXT, owner_id TEXT, parent_id NULLABLE, sort_order INT, kind TEXT, tag TEXT NULL, definition_id TEXT NULL, enabled BOOL, created_at)`**
  - `owner_kind ∈ {template, session}`，`owner_id` 指向 templates.id 或 chat_sessions.id —— **一张表同时承载模板主树与会话副本树**。
  - `kind ∈ {folder, ref}`：folder 用 `tag`（无 definition_id）；ref 用 `definition_id`（无 tag）。
- **`chat_sessions`**：
  - 新增 `template_id`（来源模板，nullable）。
  - 沿用 `override_config` JSON 存**局部覆盖**：`local_definitions: { <definition_id>: <overridden_content/meta> }`（写时复制）。
  - **退役 M2 的 `mounted_definitions`**（扁平有序 ID 列表）→ 由会话自己的 `prompt_nodes` 取代（迁移：把旧 mounted 列表转成一棵扁平 ref 节点树）。
- **`settings(key TEXT PRIMARY KEY, value JSON)`** — 全局设置 KV：provider source / base_url / api_key / model / stream / generation params / custom_css / theme / message_style / language。
  - API key 服务端存储（自托管单实例场景；后续可考虑静态加密 / 仍允许 env 覆盖）。
- **`avatars(id, path, created_at)`**（可选）—— 头像库索引（仅存 assets 相对路径，复用 M2 资源服务）。或简化为直接列 assets 子目录。
- **`definitions`**：结构不变（`type/name/content/meta`）。`regex_rule` 的 `meta` 扩展：`{ pattern, replacement, enabled, name, scope: { ai_output, user_input, display_only } }`。

### 6.2 会话创建与覆盖语义
- 创建会话（可带 `template_id`）→ **深拷模板的 `prompt_nodes` 为会话副本**（结构 per-session，互不影响）；定义内容仍**引用全局**，除非局部覆盖。
- 对话内编辑某 ref 节点内容 → 写 `override_config.local_definitions[def_id]`（不动全局）。
- 「重置为全局」= 删除该局部覆盖；「覆盖全局」= 把局部内容写回 `definitions`（二次确认）。
- 书本内编辑 = 直接改 `definitions` / 模板 `prompt_nodes`（全局）。

### 6.3 组装流水线（升级 M2 `assemble_system_prompt`）
- 入参由"mounted 定义列表"改为"会话的 `prompt_nodes` 树 + 局部覆盖 + `current_state`"。
- 遍历树：folder→XML 标签包裹；ref→`定义内容(局部覆盖优先)` 经 `render_vars`。保持 `regex_rule`/`tool` 不进 system 包裹的既有行为，但 regex 作用域改由 `meta.scope` 决定（§6.1）。

> 注：分支/分叉（messages 树、swipe 兄弟节点、fork 深拷）落地在 **M4**；M3 复用既有 `messages.parent_id` 字段，UI 先按 §10 的最小版本接。

---

## 7. API 端点（新增 / 变更）

- **Templates**：`GET/POST /api/templates`、`GET/PUT/DELETE /api/templates/{id}`、`POST /api/templates/{id}/duplicate`、`/import`、`/export`。
- **Template nodes**：`GET /api/templates/{id}/nodes`、`POST`（加节点）、`PUT /api/nodes/{nid}`（改名/启用/内容）、`DELETE`、`PUT /api/templates/{id}/nodes/reorder`（重排/改父）。
- **Sessions**：`POST /api/sessions`（可带 `template_id`，服务端深拷树）、`GET/PUT/DELETE`；会话节点树：`GET /api/sessions/{id}/nodes` + 同上节点编辑端点；局部覆盖：`PUT /api/sessions/{id}/overrides/{defId}`（设/改）、`DELETE`（重置为全局）、`POST /api/sessions/{id}/overrides/{defId}/promote`（覆盖全局）。
- **Definitions**：沿用 CRUD + `GET /api/definitions?type=char&q=…`（按类型搜索，供 `+` 选择器）、`/duplicate`、`/import`、`/export`。
- **Avatars**：`GET /api/avatars`、`POST /api/avatars`（复用 assets 上传）。
- **Settings**：`GET /api/settings`、`PUT /api/settings`。
- **Provider**：`POST /api/provider/test`（用当前 source/base_url/key/model 探活）。
- **Regex**：经 definitions（`type=regex_rule`）CRUD + import/export；服务端在 display 流水线按 scope 应用。
- 既有 `POST /api/sessions/{id}/messages`（SSE）、health、ping、assets 静态服务**不变**。

---

## 8. 前端结构

- **路由**：`/`（列表）、`/chat/:id`、`/new`（第1步）、`/new/prompt`（第2步）、`/book`、`/settings`、`/chat/:id/options`、头像库（内联/路由二选一）。
- **组件**：`AppShell`（导航/面包屑）、`ChatCard`、`MessageList`（bubble/flat 变体）、`MessageItem`（swipe/fork/actions）、`Composer`、`PromptTree` + `NodeRow` + `NodePicker`、`DefinitionEditor`、`TemplatePicker`、`AvatarPicker`、设置原语 `Slider`/`Segmented`/`Switch`/`Combobox`/`Select`、`RegexRuleEditor`、`FullscreenEditor`。
- **Store（Pinia）**：`session`（当前会话 + 消息 + 流式缓存）、`settings`、`library`（templates/definitions 缓存）、`ui`（message style / theme）。
- **通信层**：`api` 客户端（fetch + SSE），统一错误处理；接口形态预留 Tauri-IPC 实现（M8 切换实现而非调用方）。

---

## 9. 安全

- **严禁 `v-html`**；消息内容走安全渲染（Markdown 用安全渲染器 + 净化；动态 HTML 卡片用无逻辑模板引擎）。
- **Custom CSS**：注入到应用作用域 `<style>`；属用户自填（自托管），可接受，但需限定作用范围、避免破坏导航；文档注明风险。
- **Regex display 变换**在 core 内生成 `display_content`，前端只渲染结果。
- 变量修改只走 M5 指令集沙箱（M3 不引入）。
- Bearer 鉴权链沿用。

---

## 10. 范围之外 / 延后

- **分支/分叉持久化**（messages 树、swipe 多回合历史、fork 深拷）= **M4**。M3 接最小版本：regenerate 可先做"替换当前回合"，`‹ n/m ›` 多回合与 fork 的完整持久化随 M4 落地（UI 已设计好）。
- **变量面板** = M5；**自动总结 / 预算指示** = M6。
- **按模板 / 按对话覆盖 generation + regex**：用户确认"有必要"，**延后再议**（与局部覆盖模型一致）。
- **Dark theme 细化**、**移动端精修**（仅收窄留白，用户自行处理）、**全屏覆盖层细节**（太基础，不单独设计）。
- **AI 绘图等非对话补全 provider**：不做。

---

## 11. 待确认决策（review 时定）

1. **会话树 = 创建时深拷模板**（结构 per-session）vs **始终引用模板**（仅内容局部覆盖）。本 spec 取**深拷结构 + 引用内容**。
2. **`prompt_nodes` 单表（owner_kind）** vs 模板树/会话树分表。本 spec 取单表。
3. **API key 存储**：settings 表明文（自托管）vs env-only vs 加密。本 spec 取 settings 表（允许 env 覆盖）。
4. **退役 `mounted_definitions`** 的迁移是否需要保留旧端点兼容期。
5. 头像库：独立路由页 vs 内联展开（第 1 步已用内联；库管理页是否需要）。

---

## 12. 完成标志 & TDD

- **TDD**：后端新表/端点/组装升级先写 core 单测 + web 集成测试（沿用 M0–M2 的 temp-sqlite 基座）；前端关键交互（流式渲染、节点树增删改、局部覆盖、设置读写）配组件/端到端测试。
- **完成标志**：浏览器内可完成——首页浏览会话、两步新建（含模板树挂载）、流式对话（bubble/flat 切换、regenerate/copy/edit）、书本里增删改定义与模板树、设置里配 provider/参数/CSS/regex 并 Test connection 通过；组装真实引用模板树；对话内局部覆盖不污染全局。

---

## 13. 下一步

本 spec review 通过后 → `writing-plans` 为 M3 出实现计划（建议按垂直切片再分子切片：①外壳+列表+对话详情读路径 → ②新建两步+模板树+组装升级 → ③书本编辑 → ④设置/regex/provider）。
