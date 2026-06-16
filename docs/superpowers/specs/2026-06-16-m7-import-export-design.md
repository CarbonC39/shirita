# M7 — 导入导出 + 定义名包裹 设计

> 状态：已定稿（brainstorm 通过）。本里程碑仅后端为主 + 少量前端入口。
> 上游路线图：`docs/superpowers/specs/2026-06-12-shirita-roadmap-design.md`（M7 迁移导入）。

## 1. 目标与范围

让 Shirita 能与 SillyTavern 生态互通，并支持自有格式的 round-trip：

- **导入**（单一入口，按内容 sniff 来源）：
  1. ST **角色卡**：PNG（内嵌卡 JSON + 整图作头像）/ 纯 JSON，含内嵌 `character_book`。
  2. ST **世界书** JSON。
  3. Shirita **原创** JSON：① 单定义 ② 模板 bundle（启用部分）。
- **导出**（只产 Shirita 原创 JSON，**不产 ST 格式、不写 PNG**）：
  1. 单定义。
  2. 模板"启用部分"（启用节点 + 被引用定义）。
- **定义名包裹**：定义可选「用自身名字作 XML 标签包裹 content」，组装时生效。

**完成标志**：能导入现有 SillyTavern 角色卡（PNG/JSON）；导出的原创 JSON 能再导入还原。

### 明确不在本轮范围
- 对话呈现（消息头像 + 发言者名字、Markdown 渲染）——下轮单独立项。
- 群聊编排（多 char 自动接话/轮次）——独立大 feature，后续立项。
- **导出成 ST 格式 / 写 PNG**——不做。`def_to_charcard` / `defs_to_worldinfo` / `tree_to_preset` 保留在 core 但本轮不接端点。

## 2. 现状（已有，复用）

- `definitions` 表：`id TEXT PRIMARY KEY, type, name, content, meta TEXT`。**`name` 无唯一约束**——同名定义靠 UUID 共存，存储层无需改动。
- `charcard_to_defs(card: &Value) -> (Definition, Vec<Definition>)`：ST V2/V3 卡 → char 定义（ST 扩展字段存 `meta.st.*`）+ 内嵌 `character_book` → world 定义。**当前不处理头像**。
- `worldinfo_to_defs(&Value) -> Vec<Definition>`：ST 世界书 → world 定义。
- 导入端点现状：`POST /api/import/charcard`（`Json`）、`POST /api/import/worldinfo`（`Json`），内部 `persist()` 逐条 `create_definition`，**无冲突策略、不存头像**。
- 资源：`POST /api/assets`（multipart）写 `assets_dir/{uuid}.{ext}` + `Asset::new(name, stored)` + `create_asset`。`Asset { id, name, path }`。
- 头像机制：`$avatar` 是**会话状态变量**（`state.rs` system variable），值为 asset 路径；它是会话级的，**不是定义字段**。本轮导入产出的是定义，故头像存进定义 `meta.avatar`（定义自带引用），会话层如何采用属后续。
- 组装：`assemble_from_nodes` 中 **Folder 节点**用自身 `tag` 包裹子节点 `<{tag}>…</{tag}>`；**Ref 节点裸输出**定义 content（见 `resolve()`）。
- 模板：`Template { id, name, meta, … }` + prompt 节点树（`PromptNode { id, owner_kind, owner_id, parent_id, kind(Folder|Ref|History), definition_id, tag, enabled, sort_order, … }`）。

## 3. core：`pngcard` 模块（只读，手写 chunk 层，纯函数）

PNG = 8 字节签名 + 一串 `len(4) | type(4) | data | crc(4)` chunk。SillyTavern 把卡 JSON 以 base64 存在 `tEXt` chunk（keyword `chara` = V2、`ccv3` = V3）。

```rust
/// 从 PNG 字节提取内嵌角色卡 JSON：扫 tEXt chunk，keyword 优先 `ccv3` 再 `chara`，
/// base64 解码其 value 为 UTF-8 JSON。无匹配 / 非 PNG / 解码失败 → Err。
pub fn read_card_json(png: &[u8]) -> Result<serde_json::Value>;
```

- 校验前 8 字节为 PNG 签名；顺序遍历 chunk；`tEXt` data = `keyword\0text`。
- 只读，不解码像素、不校验 CRC（容忍轻微不规范文件）。
- 新依赖：`base64`（仅此一个；不需要 `crc32fast`/占位图，因为不写 PNG）。
- 纯函数，单测覆盖：合法卡片段还原、keyword 优先级、非 PNG 报错、无 tEXt 报错、base64 损坏报错。

## 4. core：原创格式 codec（`portable` 模块）

Shirita 原创导入导出 JSON 的编解码（纯函数，core）。两种信封：

```jsonc
// 单定义
{ "format": "shirita.definition", "version": 1,
  "definition": { "type": "...", "name": "...", "content": "...", "meta": {…} } }

// 模板「启用部分」
{ "format": "shirita.template", "version": 1,
  "template": { "name": "...", "meta": {…} },
  "nodes": [ { "local_id": "n1", "parent_local_id": null, "kind": "folder|ref|history",
               "tag": "...", "def_local_id": "d1", "enabled": true, "sort_order": 0 } ],
  "definitions": [ { "local_id": "d1", "type": "...", "name": "...", "content": "...", "meta": {…} } ] }
```

- 节点/定义间引用用 **`local_id`**（导出时临时分配，与真实 UUID 解耦）；导入时重映射为新 UUID。
- 导出模板「启用部分」：**排除 `enabled=false` 的节点连同其子树**；`definitions` 只含被保留 ref 节点 `definition_id` 实际引用的定义（去重）。
- 函数（签名示意，纯数据变换，不碰存储）：
  - `export_definition(&Definition) -> Value`
  - `export_template(&Template, nodes: &[PromptNode], defs: &HashMap<String,Definition>) -> Value`（内部做 enabled 过滤 + local_id 分配）
  - `parse_portable(&Value) -> PortableDoc`（枚举：`Definition(Definition)` | `Template{template, nodes, defs}`，节点用 local_id 形态）
- 单测：导出→解析 round-trip、enabled 过滤正确、引用完整性（ref 的 def_local_id 必在 definitions 内）。

## 5. core：定义名包裹

- 定义新增可选元字段 `meta.wrap_in_tag: bool`（默认缺省视为 false）。**无迁移**（meta 为自由 JSON）。
- 纯函数 `sanitize_tag(name: &str) -> String`（放 `assembly` 或 `keyword`/util 模块）：
  - `trim` → 连续空白折叠为单个 `_` → 移除 XML 致命字符 `< > & " ' /` → 保留 Unicode 字母/数字/中日韩/`_`/`-`。
  - 结果为空（名字全是被剔字符）→ 兜底返回该定义的 `def_type`。
- 组装集成：`assemble_from_nodes` 的 `resolve()` 在返回定义渲染内容前，若该定义 `meta.wrap_in_tag` 为真，包成：
  ```
  <{sanitize_tag(name)}>
  {content}
  </{sanitize_tag(name)}>
  ```
  与 Folder `tag` 共存时自然嵌套。
- ST 导入不设此字段；原创导出 `meta` 原样带出（含 `wrap_in_tag`）。
- 单测：`sanitize_tag` 各类输入（空格、中文、标点、全特殊字符兜底）；开关开启时组装输出含包裹、关闭时裸输出。

## 6. 导入端点（web）

单一入口，**改为 multipart**（单 `file` 字段，兼容 PNG 二进制与 JSON 文本）：

`POST /api/import?on_conflict=skip|overwrite|duplicate`（默认 `skip`）

**来源 sniff（按序）：**
1. 字节以 PNG 签名开头 → `pngcard::read_card_json` → ST 角色卡 → `charcard_to_defs`；并把**整张 PNG 存为 asset**，char 定义 `meta.avatar = <asset.path>`。
2. 否则按 JSON 解析：
   - `format == "shirita.template"` → 原创模板 bundle（§7 还原）。
   - `format == "shirita.definition"` → 原创单定义。
   - 有 `spec` 含 `chara_card` 或存在 `data.name`/顶层 `name`+`description` → ST 角色卡 JSON → `charcard_to_defs`（无头像）。
   - 存在 `entries`（世界书结构）→ ST 世界书 → `worldinfo_to_defs`。
   - 都不匹配 → `400`。

**冲突策略**（统一应用）：
- 定义判重键 = **name + def_type**。`skip`：不写、计入 skipped；`overwrite`：`update_definition` 已存在记录（保留其 id）；`duplicate`：以新 UUID `create_definition`。
- 模板判重键 = **name**。语义同上；`overwrite` = 删除同名模板及其节点后整体重建。

**返回**：
```json
{ "created": [{ "kind":"definition|template", "id":"...", "name":"..." }],
  "skipped": [...], "overwritten": [...] }
```

> 旧路由 `POST /api/import/charcard`、`/api/import/worldinfo` 保留为**兼容薄包装**：内部以固定来源转调统一入口的同一套落库 + 冲突逻辑，不再各自维护一份，避免重复。

## 7. 模板 bundle 还原（导入）

`format=shirita.template` 时：
1. 先按 `on_conflict` 处理 `definitions[]`，得到 `local_id -> 真实定义 id` 映射（skip 时映射到已存在同名同类定义的 id；duplicate 时映射到新建 id）。
2. 新建 `Template`（按模板冲突策略），得到 `template_id`。
3. 遍历 `nodes[]`：为每个分配新 UUID，建 `local_id -> node_id` 映射；`parent_id` 由 `parent_local_id` 经映射重指；ref 节点 `definition_id` 由 `def_local_id` 经第 1 步映射重指；`owner_kind=Template, owner_id=template_id`。
4. 保持 `sort_order`/`tag`/`enabled` 原样。

引用缺失（如 def_local_id 不在映射内）→ 跳过该 ref 节点并记日志，不整体失败。

## 8. 导出端点（web）

只产原创 JSON，附 `Content-Disposition: attachment`：

- `GET /api/definitions/{id}/export` → `portable::export_definition`（单定义信封）。
- `GET /api/templates/{id}/export` → 取模板 + 其 nodes + 被引用 defs，`portable::export_template`（启用部分信封）。

## 9. 前端

- **导入入口**（定义库 / Book 页）：文件选择 `accept=".png,.json"` → 选 `on_conflict`（skip/overwrite/duplicate）→ 上传 → 展示结果摘要（created/skipped/overwritten 计数与名称）。复用既有确认对话框样式，不新增弹窗类型。
- **导出按钮**：定义行 → 下载单定义 JSON；模板 → 下载启用部分 JSON。
- **定义编辑器**：新增 `wrap_in_tag` 开关（写入定义 `meta`）。
- 约束：Lucide 图标、无 emoji、禁 `v-html`、默认英文文案（i18n 友好）。

## 10. 测试策略

- **core**
  - `pngcard`：合法卡片段读出 JSON、keyword 优先级、非 PNG/无 tEXt/坏 base64 报错。
  - `portable`：单定义与模板 round-trip、enabled 过滤、local_id 引用完整性。
  - `sanitize_tag`：空格/中文/标点/全特殊字符兜底；包裹开关在组装中的效果。
- **web**（集成，复用 `EchoProvider` 测试 harness）
  - PNG 导入 → 定义 + 头像 asset（`meta.avatar` 指向已落库 asset）。
  - ST JSON 角色卡 / 世界书导入。
  - 原创单定义、模板 bundle 导入（节点树 + 定义重映射还原）。
  - `on_conflict` 三态（skip/overwrite/duplicate）对定义与模板。
  - 导出 → 再导入 round-trip 还原。

## 11. 切片（计划拆 4 个 Plan）

1. **core 解析与编解码**：`pngcard::read_card_json` + `portable`（export/parse）+ `sanitize_tag` 与组装包裹集成（含全部 core 单测）。
2. **导入端点**：统一 `POST /api/import`（multipart + sniff + 头像存 asset + 冲突策略），定义级来源（ST 卡 PNG/JSON、ST 世界书、原创单定义）。
3. **模板 bundle 导入 + 导出端点**：模板还原（§7）、`GET …/definitions/{id}/export`、`GET …/templates/{id}/export`。
4. **前端**：导入入口（文件 + on_conflict + 摘要）、导出按钮、定义编辑器 `wrap_in_tag` 开关。

## 12. 关键设计决策小结

- 导入吃 ST（PNG+JSON）+ 原创；导出只产原创 JSON（不产 ST、不写 PNG）→ `pngcard` 只读、依赖最小。
- 原创格式用 `local_id` 解耦真实 UUID，保证模板 bundle 跨实例 round-trip 干净。
- 同名定义存储层本就允许；导入冲突策略只是便利，`duplicate` 可显式保留同名。
- 头像存定义 `meta.avatar`（定义自带），区别于会话级 `$avatar`。
- 定义名包裹为定义级 `meta.wrap_in_tag` + `sanitize_tag`，组装时生效，无迁移。
