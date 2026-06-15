# Shirita M4 — 消息树与写时复制设计 (Message Tree & Copy-on-Write Design)

> 状态：经与用户逐点 brainstorm 确认（分支模型、编辑语义、COW 呈现、存储位置、范围裁剪）。
> 本文档**细化路线图 §5 的 M4 草案**（消息树：分支/分叉/隐藏 + 写时复制）。
> 上游：`docs/superpowers/specs/2026-06-12-shirita-roadmap-design.md`、`2026-06-13-m3-frontend-design.md`、已完成并合并到 `main` 的 M0–M3。
> 关键决策另存于会话记忆 `shirita-project`。

---

## 1. 范围与定位

M4 在「已能流式对话 + 定义/模板管理」的 M3 之上，加厚两件相关但可分离的能力：

- **子系统 A — 消息树**：regenerate 生成兄弟分支、左右 swipe 切分支、就地编辑、逐条隐藏、从某节点 fork 出新会话。
- **子系统 B — 写时复制（COW）**：对话内编辑定义/模板节点写入「本会话局部覆盖」，全局库不动；可「同步到全局」或「还原为全局」。

很大一部分地基在 M0–M3 已就位（见 §3），本里程碑主要补**语义、接口与前端**，几乎不动 schema。

**非目标**（明确推迟）：删除分支/子树（用户拍板移到 M4 之后）；变量引擎本身（M5，本里程碑 fork 只搬运 `snapshot_state`，不实现变量求值）；自动总结（M6）。

---

## 2. 设计决策摘要（brainstorm 结论）

| 决策点 | 结论 |
|---|---|
| 分支模型 | **完整消息树 + active path**：任意消息可有兄弟分支；会话记一个「激活叶子」，沿其到 root 即当前分支；分支点显示 `‹ n/m ›` swipe。 |
| 编辑语义 | **SillyTavern 同款 = 就地覆盖** `raw_content`，不产生分支。探索另一走向交给 regenerate / fork。 |
| regenerate | 在选定 assistant 的同 `parent_id` 下新增 assistant 兄弟，成为激活叶子。 |
| 隐藏 | 逐条 `is_hidden` 开关，**不级联**，组装时排除。 |
| fork | 线性深拷 `root → 指定节点` 到新会话，状态隔离。 |
| 激活叶子存储 | `chat_sessions` **独立列** `active_leaf_id`（迁移 0011），非塞进 `override_config`。 |
| COW 呈现 | **Book 页带对话上下文时分两个 section：局部（上）/ 全局（下）**；不做抽屉、不做作用域开关。 |
| 局部 section 内容 | 默认显示对话在用的 template 树 + 定义编辑（COW）；顶部一条「本对话已改」chip 条，**仅在确有局部覆盖时出现**。 |
| 局部覆盖范围 | 模板节点树 + 定义两者都纳入局部。 |
| 局部覆盖存储 | **差异 Patch**（只存被改字段）+ 组装**按字段合并**；非冲突的全局升级可平滑渗透（≈ 现有 `effective_*` 实现，明确为契约）。 |
| 生成取消 | inline SSE，客户端断开即取消、不落半成品、`active_leaf` 不前移；另加服务端按会话 `AbortHandle`，新生成先中止旧的。 |
| swipe 下沉 | MVP 取 `created_at` 最新子；未来缓存 `last_visited_child_id`（**非本期**）。 |
| 删除分支 | 推迟到 M4 之后。 |

---

## 3. 已有地基（M0–M3 既存，复用）

- `messages` 表已含 `parent_id` / `is_hidden` / `snapshot_state`（树结构、隐藏、状态快照齐备）。
- `chat_sessions` 已含 `override_config`（JSON）/ `current_state`（JSON）/ `template_id` / `mounted_definitions`。
- 组装 `send_message`（`conversation.rs`）**已**：① 过滤 `!is_hidden`；② 读取 `override_config.local_definitions` 优先于全局定义；③ 注释「会话自有节点优先（fork 后），否则引用模板」——即节点树已支持 `owner_kind=session` 的会话自有副本。
- `duplicate_session` 已实现「复制会话自有 fork 节点树」的逻辑，可被 fork / COW-materialize 复用。
- 迁移按编号推进，最新 `0010_assets.sql`，本里程碑新增 **`0011`**。

**因此本里程碑的 schema 改动仅一处**：给 `chat_sessions` 加 `active_leaf_id`。

---

## 4. 子系统 A：消息树

### 4.1 概念

- 一个会话的消息是以 `parent_id` 连接的**树**（`root` 的 `parent_id = NULL`）。
- 会话记 `active_leaf_id` = 当前展示分支最末端消息 id。**当前分支 = 从 `active_leaf` 沿 `parent_id` 回溯到 root 的线性路径（active path）**。
- 某节点的**兄弟** = 同 `parent_id` 的所有消息；在该分支点显示 `‹ 序号/总数 ›`，左右切即换兄弟。
- 切兄弟后 `active_leaf` 重设为「沿被选兄弟向下的最深叶子」；下行每层选择规则（MVP）：取 `created_at` 最新的子，直到叶子。
  - *未来迭代（非本期）*：在每个分支节点缓存 `last_visited_child_id`，让下沉路径贴合用户「上次看的那条」的记忆直觉；MVP 先用 `created_at`，心中有数即可。

### 4.2 后端接口（`shirita-web`）

| 方法 & 路径 | 作用 |
|---|---|
| `GET /api/sessions/{id}/messages` | 已有，**不改**：返回该会话**全部**消息（含 `parent_id` 等树字段）。`active_leaf_id` 属于会话对象（`Session`），前端从已加载的会话读取，无需改本端点。 |
| `POST /api/sessions/{id}/messages` (SSE) | 已有。改：在当前 `active_leaf` 下追加 `user`→`assistant`；完成后 `active_leaf = 新 assistant`。 |
| `POST /api/sessions/{id}/messages/{msgId}/regenerate` (SSE) | 新。`msgId` 指 assistant 消息；在其 `parent_id` 下生成**新 assistant 兄弟**；`active_leaf = 新兄弟`。 |
| `PUT /api/sessions/{id}/messages/{msgId}` | 新。body `{ content?: string, is_hidden?: bool }`。`content` → 就地覆盖 `raw_content` 并**重算 `display_content`**（重跑 `apply_regex_rules`）；`is_hidden` → 切隐藏。 |
| `PUT /api/sessions/{id}/active-leaf` | 新。body `{ message_id }`。校验属本会话；按 §4.1 规则把 `active_leaf` 设为沿该消息的最深叶子。 |
| `POST /api/sessions/{id}/fork` | 新。body `{ message_id }`。见 §4.4。返回新 `Session`。 |

> 核心层新增纯函数 `active_path(messages: &[Message], active_leaf_id) -> Vec<&Message>`，供组装与前端逻辑共用语义（前端用 TS 重写同规则）。

### 4.3 组装 / 上下文（关键改动）

`send_message` 现在以 `list_messages`（全部消息，按时间）为历史。**改为只取 active path**：

1. 取会话 `active_leaf_id` 与全部消息 → `active_path()` 得到 root→leaf 线性历史；
2. 仍 `filter(!is_hidden)`；
3. 该历史用于拼接上下文。

即「上下文 = 当前分支可见消息」。发送新消息时父节点取 `active_leaf`，组装含刚写入的 user 消息。

### 4.4 fork 语义

`POST /fork {message_id}`：

1. 取 active path 中 `root → message_id` 的**线性切片**（只拷当前分支到该节点，不复制其它分支）。
2. 新建会话，**重映射 `parent_id`** 写入这些消息（保留 `is_hidden` / `role` / 内容）；新 `active_leaf` = 拷贝后的叶子。
3. 会话级承袭：`template_id`、会话自有节点树（若有，复用 `duplicate_session` 的拷贝逻辑）、`mounted_definitions`、`override_config`（**含 `local_definitions`**）。
4. `current_state` = 该节点的 `snapshot_state`（状态隔离；变量求值在 M5，本里程碑仅搬运快照值）。
5. 命名：`"{原名} (fork)"`。

### 4.5 前端（`shirita-ui`）

- `MessageItem`：在每个分支点渲染 `‹ n/m ›`（调 `PUT active-leaf`）；操作区加 **编辑（就地）/ 隐藏开关 / regenerate / fork**。
- `MessageList` / `ChatView`：依据「全部消息 + `active_leaf`」用 TS 版 `activePath()` 只渲染当前分支；现有 `handleRegenerate`（目前是「重发最后一条 user」的占位）替换为调用 regenerate 接口。
- `stores/chat`：保存 `active_leaf`；swipe / regenerate / 编辑 / 隐藏 / fork 后局部刷新。
- 通信层补上述端点；SSE 复用既有解析。

### 4.6 并发与取消（生成生命周期）

生成（send / regenerate）跑在 SSE 响应 future 内（`async_stream`，**非 detached spawn**），且 **assistant 消息只在 provider 流完整结束后才落库**。由此已有两条既成性质：

- **客户端断开即取消**：用户切分支 / 发起新生成 / 离开页面时前端关闭旧 `EventSource` → 该 future 被 drop → 生成在下一个 `.await` 处停止；**被中止的生成不写入任何半成品消息**（落库在流结束之后）。
- **`active_leaf` 只在成功落库后转移**：被中止的生成不移动 `active_leaf`。

在此之上加一层**服务端保护**（Plan 1 纳入），不依赖前端关连接的时序：

- `AppState` 持一个按 `session_id` 的「进行中生成」登记（`AbortHandle` / `tokio_util::sync::CancellationToken` / 或一把每会话生成锁）。
- 新的 send / regenerate 开始前，**先中止该会话上一个在途生成**，避免两个生成竞争写兄弟 / 抢移 `active_leaf`。
- 前端规约：发起新生成 / 切分支前必须关闭旧 `EventSource`（与服务端保护互为双保险）。

---

## 5. 子系统 B：写时复制（COW）

### 5.1 概念

- 对话内对**定义**的修改 → 以**差异 Patch** 写进 `override_config.local_definitions[defId]`：**只存被改的字段**（`content` / `trigger` / `scan` / `name` 等各自独立的键），全局库不动。
  - 组装**按字段合并**：`effective = global ⊕ patch`，patch 里没有的字段回落全局。这是**现有实现**——`effective_trigger` / `effective_scan` / `effective_def_content` 已逐字段读取覆盖、缺则取全局；本 spec 把它**明确规定**为契约（不可改成「整份快照替换」）。
  - 收益：未冲突的全局升级（改了你**没**覆盖的字段）能平滑渗透到所有会话，不会因「整份冻结」而失联；`trigger` / `scan` 等 meta 子项各自独立覆盖、互不冻结。
- 对话内对**模板节点树**的修改 → 写时复制成**会话自有节点**（`owner_kind=session`）：首次局部编辑时把模板节点拷到会话（复用 `duplicate_session` 同款拷贝），之后改会话自有树。
- **同步到全局**（二次确认）：把本地版写回全局并清掉该局部覆盖。**还原为全局**：清掉局部覆盖（节点树则丢弃会话自有树、回落引用模板）。
- 「写时」= 没改之前局部不存任何副本，开始编辑才落地。

### 5.2 后端接口

| 方法 & 路径 | 作用 |
|---|---|
| `PUT /api/sessions/{id}/local-definitions/{defId}` | 写/更新 `override_config.local_definitions[defId]` 的**差异 Patch**（只含被改字段：`content`/`trigger`/`scan`/`name`）；组装按字段合并 over 全局（见 §5.1）。 |
| `DELETE /api/sessions/{id}/local-definitions/{defId}` | 还原为全局：删该局部覆盖。 |
| `POST /api/sessions/{id}/local-definitions/{defId}/promote` | 同步到全局：用局部版更新全局 `definition`，并清该局部覆盖。 |
| `POST /api/sessions/{id}/materialize-nodes` | 模板节点写时复制：若会话尚无自有节点，从其 `template_id` 拷一份会话自有树（首次局部编辑前调用）。之后沿用既有 `…/nodes`（`owner_kind=session`）增删改。 |

> 局部节点的增删改**复用 M3 既有节点接口**，只是 `owner_kind=session`、`owner_id=session_id`；本里程碑只补「materialize（首写拷贝）」与定义局部覆盖的 set/clear/promote。

### 5.3 前端：Book 页分 section

- `BookView` 读 `AppShell` 的 `activeChatId`：
  - **有活动对话**：渲染 **局部（本会话，置顶）+ 全局（置底）** 两个 section。
  - **无活动对话**：只渲染 **全局**（= 当前 M3 行为，零变化）。
- **局部 section**：
  - 模板选择器默认选中并显示该对话在用的 template；其下是该会话**有效节点树**（会话自有树优先，否则模板）。在此编辑节点 → 触发 `materialize-nodes` 后改会话自有树。
  - 复用 `DefinitionEditor`；在局部 scope 编辑某定义 → 调 `PUT local-definitions/{defId}`。
  - 顶部一条 **「本对话已改」chip 条**：列出 `override_config.local_definitions` 的各项（+ 局部节点树标记）；**为空则整行不显示**。每个 chip 进入该项局部编辑，并挂「同步到全局」（二次确认）/「还原为全局」。
  - 被本地改过的 ref 节点在树里带小角标，与 chip 条呼应。
- **全局 section**：现有 Book 行为，直接改库。

---

## 6. 实现拆分（两个独立 plan）

> 一份 spec、两个实现计划，各走 TDD。

- **Plan 1 — 消息树 (A)**
  - 后端：迁移 0011 加 `active_leaf_id`；`active_path()` 核心函数；组装改走 active path；接口 regenerate / 编辑(就地)+隐藏 / active-leaf / fork；编辑后重算 `display_content`。
  - 前端：active path 渲染、swipe、编辑/隐藏/regenerate/fork UI、chat store 调整。
- **Plan 2 — 写时复制 (B)**
  - 后端：local-definitions set/clear/promote、materialize-nodes。
  - 前端：Book 局部/全局双 section、局部 scope 编辑接线、「本对话已改」chip 条、同步/还原。

Plan 1 先行；A 与 B 解耦，Plan 2 可在其后接续。

---

## 7. 测试策略（TDD，测试先行）

- **核心 (`shirita-core`)**：`active_path` 选路（多分支、隐藏穿插、单链、空会话）；组装只纳入 active path 且排除 `is_hidden`；fork 切片 + parent_id 重映射 + `override_config`/`snapshot_state` 承袭；**local_definitions 差异 Patch 按字段合并**——覆盖某字段后改全局的**另一**字段，effective 同时反映（propagation），且覆盖字段仍以局部为准。
- **Web (`shirita-web`)**：各新端点状态码与副作用（regenerate 产生兄弟且切 active_leaf；编辑重算 display_content；active-leaf 取最深叶子；fork 返回隔离的新会话；promote 改全局并清覆盖；materialize 幂等）；**并发取消**：同会话发起新生成会中止前一个在途生成、被中止者不落库且 `active_leaf` 不前移。
- **前端 (`vitest`)**：`MessageItem` swipe 序号/切换、就地编辑、隐藏；`activePath` TS 函数；Book 局部/全局双 section 的出现条件与 chip 条空态隐藏。

---

## 8. 完成标志

- 分支：regenerate 出兄弟、`‹ n/m ›` 可来回切、当前分支正确进上下文；隐藏逐条生效；fork 出状态隔离的新会话。
- 编辑：就地改 `raw_content` 并重算 `display_content`，不产生分支。
- COW：对话内改定义/模板节点只动本会话，全局库不被污染；可同步到全局 / 还原；「本对话已改」一眼可见、无改动时不堆叠。
- `cargo test --workspace` 与 `vitest` 全绿、零警告。
