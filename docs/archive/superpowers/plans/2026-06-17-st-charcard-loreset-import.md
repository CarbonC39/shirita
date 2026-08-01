# ST 角色卡 → 设定集 导入(数据层)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把一张 SillyTavern 角色卡(v2/v3)单向、无损地翻译成一个 Shirita 设定集——一批 `Definition` + 一棵引用它们的 2 层 `Template`,新建会话时 seed 出开场白。

**Architecture:** 万物皆定义:每个非空 ST 字段各成一个独立定义 + Ref 节点,挂到设定集 Template,落点(before/after-history)由 `sort_order` 相对 `history` 节点决定。regex 从「全局」改为「按 template 引用」收集。首消息靠新增 `Message.is_anchor`(进 prompt、不进 UI)避免 assistant-first 的 API 400。

**Tech Stack:** Rust(shirita-core / shirita-web)、sqlx 0.8 + SQLite(versioned migrations `migrations/*.sql`)、axum、Vue3(shirita-ui)。

## Global Constraints

- **2 层节点树**:folder/history 挂根(parent=None);ref 的 parent 为 None 或同 owner 根 folder。导入产出的树必须合法 2 层。
- **一切皆定义**:不新增 Character 实体、不新增「库内文件夹」概念。设定集 = Template + 它引用的 Definitions。
- **单向有损翻译**:不做回出口(删除 `def_to_charcard`/`defs_to_worldinfo`)。
- **本 slice 不做**:HTML 渲染本身、ST 预设导入、prompt 侧 regex 应用、depth_prompt 深度注入、MVU、tavern_helper(原样存 `meta.st_raw`)。
- **TDD + 频繁提交**:每个 Task 一个独立可测交付物;红→绿→commit。
- 提交信息末尾加 `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`。

参考设计:`docs/superpowers/specs/2026-06-17-st-charcard-loreset-import-design.md`。

---

## File Structure

- `shirita-core/src/models/def_type.rs` — `RESERVED` 加 `first_message`。
- `shirita-core/src/models/message.rs` — `Message` 加 `is_anchor: bool`。
- `shirita-core/migrations/0013_message_is_anchor.sql` — 加列。
- `shirita-core/src/storage/sqlite.rs` — messages 读写映射 `is_anchor`。
- `shirita-core/src/assembly.rs` — 非渲染 ref 跳过 + `AssembledPlan.regex_rules` 收集 + `apply_regex_rules` honor `disabled`/`scope`。
- `shirita-core/src/conversation.rs` — `assemble_request` 用 `plan.regex_rules` 取代全局 filter。
- `shirita-core/src/adapters/charcard.rs` — 重写为 `charcard_to_loreset` + `LoreSet`;删回出口。
- `shirita-core/src/adapters/worldinfo.rs` — 删 `defs_to_worldinfo` 及其测试(`worldinfo_to_defs` 保留)。
- `shirita-web/src/routes/import_export.rs` — charcard 导入落「定义 + template + nodes」。
- `shirita-web/src/routes/sessions.rs` — `create_session` seed anchor + 开场白。
- `shirita-ui/src/components/MessageList.vue` + `src/api/types.ts` — `is_anchor` 消息不渲染。

---

## Task 1: `first_message` 保留类型

**Files:**
- Modify: `shirita-core/src/models/def_type.rs:6`
- Test: `shirita-core/src/models/def_type.rs`(内联 `mod tests`)

**Interfaces:**
- Produces: `is_reserved("first_message") == true`(供 charcard 适配与装配判别非渲染类型)。

- [ ] **Step 1: 写失败测试**

在 `def_type.rs` 的 `mod tests` 里加:

```rust
    #[test]
    fn first_message_is_reserved() {
        assert!(is_reserved("first_message"));
        assert!(!is_prompt("first_message"));
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p shirita-core first_message_is_reserved`
Expected: FAIL(`first_message` 不在 RESERVED)。

- [ ] **Step 3: 实现**

把 `def_type.rs:6` 改为:

```rust
pub const RESERVED: [&str; 4] = ["prompt", "regex_rule", "tool", "first_message"];
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p shirita-core first_message_is_reserved`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add shirita-core/src/models/def_type.rs
git commit -m "feat(core): add first_message reserved def_type

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 2: `Message.is_anchor` 字段 + 迁移 + 存储

**Files:**
- Modify: `shirita-core/src/models/message.rs:33`(struct)、`:60`(new)
- Create: `shirita-core/migrations/0013_message_is_anchor.sql`
- Modify: `shirita-core/src/storage/sqlite.rs`(`row_to_message` :90、`create_message` INSERT :261、`list_messages`/`get_message` SELECT :290/:301)
- Test: `shirita-core/src/storage/sqlite.rs`(内联 `mod tests`)

**Interfaces:**
- Produces: `Message.is_anchor: bool`(默认 false)。语义:进 prompt(`is_hidden=false`)、不进 UI。

- [ ] **Step 1: 写失败测试**

在 `sqlite.rs` 的 `mod tests` 里加(`Sess`/`Msg` 别名同模块已用):

```rust
    #[tokio::test]
    async fn message_is_anchor_roundtrips() {
        let store = temp_storage().await;
        let s = Sess::new("anchor");
        store.create_session(&s).await.unwrap();
        let mut m = Msg::new(&s.id, None, Role::User, "<start>");
        m.is_anchor = true;
        store.create_message(&m).await.unwrap();
        let got = store.get_message(&m.id).await.unwrap().unwrap();
        assert!(got.is_anchor);
        // 普通消息默认 false
        let m2 = Msg::new(&s.id, None, Role::User, "hi");
        store.create_message(&m2).await.unwrap();
        assert!(!store.get_message(&m2.id).await.unwrap().unwrap().is_anchor);
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p shirita-core message_is_anchor_roundtrips`
Expected: FAIL(编译错误:`Message` 无 `is_anchor`)。

- [ ] **Step 3: 加字段**

`message.rs` struct 在 `pub is_hidden: bool,` 后加:

```rust
    /// 合成锚定 user 轮:进 prompt、不进 UI(区别于 is_hidden:出 UI、不进 prompt)。
    pub is_anchor: bool,
```

`Message::new` 在 `is_hidden: false,` 后加:

```rust
            is_anchor: false,
```

- [ ] **Step 4: 加迁移**

新建 `shirita-core/migrations/0013_message_is_anchor.sql`:

```sql
ALTER TABLE messages ADD COLUMN is_anchor INTEGER NOT NULL DEFAULT 0;
```

- [ ] **Step 5: 存储读写映射**

`row_to_message`(`:90`)在读 `is_hidden` 后加,并补进返回的 `Message{}`:

```rust
    let is_anchor: i64 = row.try_get("is_anchor")?;
```
返回结构体里 `is_hidden: is_hidden != 0,` 后加 `is_anchor: is_anchor != 0,`。

`create_message` 的 INSERT(`:261`):列表加 `is_anchor`、占位符加一个 `?`、并在 `.bind(message.is_hidden as i64)` 后加 `.bind(message.is_anchor as i64)`:

```rust
            "INSERT INTO messages \
             (id, session_id, parent_id, role, raw_content, display_content, is_hidden, is_anchor, snapshot_state, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
```

`list_messages`(`:290`)与 `get_message`(`:301`)两处 SELECT 列表把 `is_hidden,` 改为 `is_hidden, is_anchor,`。

- [ ] **Step 6: 修编译(其它 Message 构造点)**

Run: `cargo build -p shirita-core && cargo build -p shirita-web`
Expected: 若有「Message 结构体字面量缺 `is_anchor`」的报错,给该字面量补 `is_anchor: false,`(用 `Msg::new`/`..` 展开的不受影响)。直到 0 报错。

- [ ] **Step 7: 跑测试确认通过**

Run: `cargo test -p shirita-core message_is_anchor_roundtrips`
Expected: PASS。

- [ ] **Step 8: 提交**

```bash
git add shirita-core/src/models/message.rs shirita-core/migrations/0013_message_is_anchor.sql shirita-core/src/storage/sqlite.rs
git commit -m "feat(core): add Message.is_anchor (in-prompt, out-of-UI)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 3: 装配——非渲染 ref + 收集 regex + apply honor

**Files:**
- Modify: `shirita-core/src/assembly.rs`(`AssembledPlan` :~205、`assemble_from_nodes` :288、`apply_regex_rules`)
- Test: `shirita-core/src/assembly.rs`(内联 `mod tests`)

**Interfaces:**
- Produces: `AssembledPlan.regex_rules: Vec<Definition>`(树里被 enabled ref 引用的 regex_rule 定义);`assemble_from_nodes` 不再为 `regex_rule`/`first_message` 类型 ref 产 prompt 段;`apply_regex_rules` 跳过 `meta.disabled==true` 与 `meta.scope=="prompt"` 的规则。

- [ ] **Step 1: 写失败测试**

在 `assembly.rs` 的 `mod tests` 加(沿用本模块已有的 `def`/`folder_node`/`history_node` helper;`new_ref` 经 `PromptNode::new_ref`):

```rust
    #[test]
    fn non_rendering_refs_skipped_and_regex_collected() {
        use std::collections::HashMap;
        let mut rx = def("regex_rule", "R", "");
        rx.meta = serde_json::json!({ "pattern": "X", "replacement": "Y" });
        let fm = def("first_message", "Hi", "hello there");
        let neo = def("char", "Neo", "Neo body");
        let charf = folder_node("t", 0, "char");
        let cref = PromptNode::new_ref(OwnerKind::Template, "t", Some(charf.id.clone()), 0, &neo.id);
        let rxref = PromptNode::new_ref(OwnerKind::Template, "t", None, 1, &rx.id);
        let fmref = PromptNode::new_ref(OwnerKind::Template, "t", None, 2, &fm.id);

        let mut defs = HashMap::new();
        for d in [&rx, &fm, &neo] { defs.insert(d.id.clone(), d.clone()); }
        let nodes = vec![charf, cref, rxref, fmref];
        let mut roll = || 0.0;
        let plan = assemble_from_nodes(&nodes, &defs, &serde_json::json!({}), &serde_json::json!({}), &[], &mut roll);

        // regex_rule / first_message 不进 prompt 段
        let joined: String = plan.segments.iter().map(|s| s.content.clone()).collect();
        assert!(joined.contains("Neo body"));
        assert!(!joined.contains("hello there"));
        assert!(!joined.contains("pattern"));
        // regex 被收集
        assert_eq!(plan.regex_rules.len(), 1);
        assert_eq!(plan.regex_rules[0].name, "R");
    }

    #[test]
    fn apply_regex_honors_disabled_and_scope() {
        let mut on = Definition::new("regex_rule", "on", "");
        on.meta = serde_json::json!({ "pattern": "a", "replacement": "b" });
        let mut off = Definition::new("regex_rule", "off", "");
        off.meta = serde_json::json!({ "pattern": "a", "replacement": "Z", "disabled": true });
        let mut prompt_only = Definition::new("regex_rule", "po", "");
        prompt_only.meta = serde_json::json!({ "pattern": "b", "replacement": "Q", "scope": "prompt" });
        // disabled 不生效、scope=prompt 不作用于 display:仅 on 生效 a->b
        let out = apply_regex_rules("aaa", &[on, off, prompt_only]).unwrap();
        assert_eq!(out, "bbb");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p shirita-core non_rendering_refs_skipped_and_regex_collected apply_regex_honors_disabled_and_scope`
Expected: FAIL(`AssembledPlan` 无 `regex_rules`;非渲染未跳过;honor 未实现)。

- [ ] **Step 3: 扩展 `AssembledPlan`**

`AssembledPlan` 加字段:

```rust
pub struct AssembledPlan {
    pub segments: Vec<PromptSegment>,
    pub history_enabled: bool,
    /// 本设定集生效的 regex(树里被 enabled ref 引用的 regex_rule 定义)。
    pub regex_rules: Vec<Definition>,
}
```

- [ ] **Step 4: 改 `assemble_from_nodes`**

在文件内(`assemble_from_nodes` 上方或 `maybe_wrap` 附近)加判别 helper:

```rust
/// regex_rule / first_message 类型的 ref 不渲染成 prompt 段(由各自子系统消费)。
fn is_non_rendering(def_type: &str) -> bool {
    matches!(def_type, "regex_rule" | "first_message")
}
```

在 entries 构建循环里,取到 `def` 后、`effective_scan` 前加跳过:

```rust
        if is_non_rendering(&def.def_type) {
            continue;
        }
```

在 `resolve` 闭包里,取到 `def` 后加(在 `active.contains` 判断前):

```rust
        if is_non_rendering(&def.def_type) {
            return None;
        }
```

在函数尾部 `AssembledPlan { segments, history_enabled }` 之前,收集 regex:

```rust
    let regex_rules: Vec<Definition> = nodes
        .iter()
        .filter(|n| n.enabled && n.kind == NodeKind::Ref)
        .filter_map(|n| n.definition_id.as_ref().and_then(|id| definitions.get(id)))
        .filter(|d| d.def_type == "regex_rule")
        .cloned()
        .collect();
    AssembledPlan { segments, history_enabled, regex_rules }
```

- [ ] **Step 5: 改 `apply_regex_rules`**

在 `for rule in rules {` 循环体最前面加:

```rust
        if rule.meta.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        let scope = rule.meta.get("scope").and_then(|v| v.as_str()).unwrap_or("display");
        if scope == "prompt" {
            continue; // prompt 侧应用留给富 regex 引擎 slice
        }
```

- [ ] **Step 6: 修同文件构造 `AssembledPlan` 的测试**

Run: `cargo test -p shirita-core --lib assembly 2>&1 | head -30`
Expected: 若 `build_messages_*` 等测试以 `AssembledPlan { segments, history_enabled }` 字面量构造,给它们补 `regex_rules: vec![],`。直到编译通过。

- [ ] **Step 7: 跑测试确认通过**

Run: `cargo test -p shirita-core --lib assembly`
Expected: PASS(含两个新测)。

- [ ] **Step 8: 提交**

```bash
git add shirita-core/src/assembly.rs
git commit -m "feat(core): non-rendering refs + scoped regex collection + disabled/scope honor

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 4: conversation 用设定集 regex 取代全局 filter

**Files:**
- Modify: `shirita-core/src/conversation.rs`(`assemble_request` :134-143)
- Test: `shirita-core/src/conversation.rs`(更新 `regex_rule_sets_display_content` :549)

**Interfaces:**
- Consumes: `AssembledPlan.regex_rules`(Task 3)。
- Produces: `assemble_request` 返回的 `regex_rules` 来自当前会话 template 树引用的 regex,而非全局所有。

- [ ] **Step 1: 改实现**

`assemble_request` 里删除全局 filter 块(`:134-143` 的 `let regex_rules: Vec<Definition> = storage.list_definitions()...filter(def_type=="regex_rule").collect();`),改为复用 `plan.regex_rules`。把末尾返回改为:

```rust
    let max_tokens = provider_max_tokens(storage).await;
    Ok((ChatRequest { model, messages: chat_messages, summary, max_tokens }, plan.regex_rules))
```

(若 `plan` 在 `build_chat_messages` 之后已被借用结束,直接 `plan.regex_rules` 取值即可;无则 `plan.regex_rules.clone()`。)

- [ ] **Step 2: 更新受影响测试**

`regex_rule_sets_display_content`(`:549`)原靠全局 regex。改为把 regex 经 template 引用:在 `storage.create_definition(&rule)` 后、`create_session` 前插入,并给 session 绑定该 template:

```rust
        // regex 现在按 template 引用生效:建一个引用该 regex 的 template。
        let tmpl = crate::models::template::Template::new("rx");
        storage.create_template(&tmpl).await.unwrap();
        let rxref = crate::models::prompt_node::PromptNode::new_ref(
            crate::models::prompt_node::OwnerKind::Template, &tmpl.id, None, 0, &rule.id);
        storage.create_node(&rxref).await.unwrap();
        let mut session = session;
        session.template_id = Some(tmpl.id.clone());
```

(把原 `let session = Session::new("t");` 保留;上面 `let mut session = session;` 紧接其赋值前移除重复绑定——确保 `session.template_id` 在 `create_session` 之前设置。)

- [ ] **Step 3: 跑测试确认通过**

Run: `cargo test -p shirita-core regex_rule_sets_display_content`
Expected: PASS(display = "hello")。

- [ ] **Step 4: 全 core 回归**

Run: `cargo test -p shirita-core`
Expected: PASS。若别的测试依赖「全局 regex」假设而红,按同法给它的 session 绑定引用 regex 的 template。

- [ ] **Step 5: 提交**

```bash
git add shirita-core/src/conversation.rs
git commit -m "feat(core): scope regex to session template tree, not global

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 5: `charcard_to_loreset` + `LoreSet`(删回出口)

**Files:**
- Modify(重写): `shirita-core/src/adapters/charcard.rs`
- Modify: `shirita-core/src/adapters/worldinfo.rs`(删 `defs_to_worldinfo` + 用它的测试 `exports_defs_to_standalone_map`/`worldinfo_roundtrips`)
- Modify: `shirita-core/src/lib.rs`(导出 `charcard_to_loreset`/`LoreSet`;移除 `def_to_charcard`/`defs_to_worldinfo` 导出)
- Test: `shirita-core/src/adapters/charcard.rs`(内联 `mod tests`)

**Interfaces:**
- Consumes: `worldinfo_to_defs`(保留)、`Template::new`、`PromptNode::new_folder`/`new_ref`、`Definition::new`、`first_message` 类型(Task 1)。
- Produces:
  ```rust
  pub struct LoreSet { pub template: Template, pub definitions: Vec<Definition>, pub nodes: Vec<PromptNode> }
  pub fn charcard_to_loreset(card: &serde_json::Value) -> LoreSet;
  ```

- [ ] **Step 1: 写失败测试**

替换 `charcard.rs` 的 `mod tests` 为:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn ty<'a>(s: &'a LoreSet, t: &str) -> Vec<&'a Definition> {
        s.definitions.iter().filter(|d| d.def_type == t).collect()
    }

    #[test]
    fn decomposes_every_nonempty_field() {
        let card = serde_json::json!({
            "spec": "chara_card_v3", "spec_version": "3.0",
            "data": {
                "name": "Neo", "description": "desc",
                "personality": "calm", "scenario": "the matrix",
                "mes_example": "<START>ex", "system_prompt": "be terse",
                "post_history_instructions": "stay terse",
                "first_mes": "wake up", "alternate_greetings": ["again", "third"],
                "character_book": { "entries": [ { "keys": ["zion"], "comment": "Zion", "content": "Last city" } ] },
                "extensions": { "regex_scripts": [
                    { "scriptName": "r1", "findRegex": "a", "replaceString": "b", "disabled": false, "markdownOnly": true }
                ] }
            }
        });
        let s = charcard_to_loreset(&card);
        // 每个非空字段各一个定义
        assert_eq!(ty(&s, "char").len(), 2);          // description + personality
        assert_eq!(ty(&s, "world").len(), 2);          // scenario(constant) + 1 book entry
        assert_eq!(ty(&s, "prompt").len(), 3);         // system_prompt + mes_example + post_history
        assert_eq!(ty(&s, "first_message").len(), 1);
        assert_eq!(ty(&s, "regex_rule").len(), 1);
        // first_message 带备选
        let fm = ty(&s, "first_message")[0];
        assert_eq!(fm.content, "wake up");
        assert_eq!(fm.meta["alternate_greetings"][1], "third");
        // scenario world 是 constant
        let scen = ty(&s, "world").iter().find(|d| d.name == "the matrix" || d.content == "the matrix").unwrap();
        assert_eq!(scen.meta["trigger"]["mode"], "constant");
        // 2 层:每个 ref 的 parent 要么 None 要么指向一个 folder
        let folder_ids: std::collections::HashSet<_> =
            s.nodes.iter().filter(|n| n.kind == crate::NodeKind::Folder).map(|n| n.id.clone()).collect();
        for n in s.nodes.iter().filter(|n| n.kind == crate::NodeKind::Ref) {
            assert!(n.parent_id.is_none() || folder_ids.contains(n.parent_id.as_ref().unwrap()));
        }
        // system_prompt 在 history 之前、post_history 在 history 之后(按 sort_order)
        let hist = s.nodes.iter().find(|n| n.kind == crate::NodeKind::History).unwrap();
        let sys_ref = s.nodes.iter().find(|n| n.parent_id.is_none() && n.kind == crate::NodeKind::Ref
            && s.definitions.iter().any(|d| Some(&d.id)==n.definition_id.as_ref() && d.content=="be terse")).unwrap();
        let post_ref = s.nodes.iter().find(|n| n.parent_id.is_none() && n.kind == crate::NodeKind::Ref
            && s.definitions.iter().any(|d| Some(&d.id)==n.definition_id.as_ref() && d.content=="stay terse")).unwrap();
        assert!(sys_ref.sort_order < hist.sort_order);
        assert!(post_ref.sort_order > hist.sort_order);
    }

    #[test]
    fn empty_fields_produce_no_defs() {
        let card = serde_json::json!({ "data": { "name": "Bare", "description": "only desc" } });
        let s = charcard_to_loreset(&card);
        assert_eq!(s.definitions.len(), 1); // 仅 char(description)
        assert_eq!(s.definitions[0].def_type, "char");
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p shirita-core --lib charcard`
Expected: FAIL(`charcard_to_loreset`/`LoreSet` 不存在)。

- [ ] **Step 3: 重写 `charcard.rs` 主体**

替换文件顶部(保留下方 `mod tests`):

```rust
//! SillyTavern Character Card v2/v3 → Shirita 设定集(Template + Definitions + Nodes)。
//! 单向有损翻译:每个非空字段各成一个定义 + ref 节点。

use crate::adapters::worldinfo::worldinfo_to_defs;
use crate::models::definition::Definition;
use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
use crate::models::template::Template;

pub struct LoreSet {
    pub template: Template,
    pub definitions: Vec<Definition>,
    pub nodes: Vec<PromptNode>,
}

fn nonempty<'a>(data: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    data.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

fn regex_rule_def(s: &serde_json::Value) -> Definition {
    let name = s.get("scriptName").and_then(|v| v.as_str()).unwrap_or("regex").to_string();
    let mut d = Definition::new("regex_rule", name, "");
    let scope = match (s.get("markdownOnly").and_then(|v| v.as_bool()).unwrap_or(false),
                       s.get("promptOnly").and_then(|v| v.as_bool()).unwrap_or(false)) {
        (true, false) => "display",
        (false, true) => "prompt",
        _ => "both",
    };
    let targets: Vec<&str> = s.get("placement").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_u64()).map(|n| match n { 1 => "user_input", _ => "ai_output" }).collect())
        .unwrap_or_default();
    d.meta = serde_json::json!({
        "pattern": s.get("findRegex").and_then(|v| v.as_str()).unwrap_or(""),
        "replacement": s.get("replaceString").and_then(|v| v.as_str()).unwrap_or(""),
        "disabled": s.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false),
        "scope": scope,
        "targets": targets,
        "min_depth": s.get("minDepth"),
        "max_depth": s.get("maxDepth"),
        "st_raw": s.clone(),
    });
    d
}

pub fn charcard_to_loreset(card: &serde_json::Value) -> LoreSet {
    let data = card.get("data").unwrap_or(card);
    let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("Imported character").to_string();

    let tmpl = Template::new(name.clone());
    // OwnerKind 非 Copy,构造函数按值取,故各处直接写 OwnerKind::Template(单元变体,零成本)。
    let mut defs: Vec<Definition> = Vec::new();
    let mut nodes: Vec<PromptNode> = Vec::new();
    let mut sort: i64 = 0;
    let mut next = |s: &mut i64| -> i64 { let v = *s; *s += 1; v };

    // --- before-history:system_prompt 最前 ---
    if let Some(sp) = nonempty(data, "system_prompt") {
        let d = Definition::new("prompt", format!("{name}·system"), sp);
        nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, next(&mut sort), &d.id));
        defs.push(d);
    }

    // --- char folder:description + personality ---
    let charf = PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, next(&mut sort), "char");
    let mut child_sort = 0;
    let desc = Definition::new("char", name.clone(), data.get("description").and_then(|v| v.as_str()).unwrap_or(""));
    nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, Some(charf.id.clone()), { let v = child_sort; child_sort += 1; v }, &desc.id));
    defs.push(desc);
    if let Some(p) = nonempty(data, "personality") {
        let d = Definition::new("char", format!("{name}·personality"), p);
        nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, Some(charf.id.clone()), { let v = child_sort; child_sort += 1; v }, &d.id));
        defs.push(d);
    }
    let _ = child_sort;
    nodes.push(charf);

    // --- world folder:scenario(constant) + character_book ---
    let worldf = PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, next(&mut sort), "world");
    let mut wsort = 0;
    if let Some(sc) = nonempty(data, "scenario") {
        let mut d = Definition::new("world", format!("{name}·scenario"), sc);
        d.meta = serde_json::json!({ "trigger": { "mode": "constant", "keys": [], "probability": 100, "order": 100 } });
        nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, Some(worldf.id.clone()), { let v = wsort; wsort += 1; v }, &d.id));
        defs.push(d);
    }
    if let Some(book) = data.get("character_book") {
        for bd in worldinfo_to_defs(book) {
            nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, Some(worldf.id.clone()), { let v = wsort; wsort += 1; v }, &bd.id));
            defs.push(bd);
        }
    }
    let _ = wsort;
    nodes.push(worldf);

    // --- before-history:mes_example ---
    if let Some(ex) = nonempty(data, "mes_example") {
        let d = Definition::new("prompt", format!("{name}·examples"), ex);
        nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, next(&mut sort), &d.id));
        defs.push(d);
    }

    // --- 非渲染根 ref:regex_scripts + first_message ---
    if let Some(scripts) = data.get("extensions").and_then(|e| e.get("regex_scripts")).and_then(|v| v.as_array()) {
        for s in scripts {
            let d = regex_rule_def(s);
            nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, next(&mut sort), &d.id));
            defs.push(d);
        }
    }
    let first = nonempty(data, "first_mes");
    let alts: Vec<String> = data.get("alternate_greetings").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    if first.is_some() || !alts.is_empty() {
        let mut d = Definition::new("first_message", format!("{name}·greeting"), first.unwrap_or(""));
        d.meta = serde_json::json!({ "alternate_greetings": alts });
        nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, next(&mut sort), &d.id));
        defs.push(d);
    }

    // --- history 节点 ---
    let mut hist = PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, next(&mut sort), "history");
    hist.kind = NodeKind::History;
    hist.tag = None;
    nodes.push(hist);

    // --- after-history:post_history_instructions ---
    if let Some(ph) = nonempty(data, "post_history_instructions") {
        let d = Definition::new("prompt", format!("{name}·post"), ph);
        nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, next(&mut sort), &d.id));
        defs.push(d);
    }

    // --- extensions 透传到 char 定义 meta(无法解释的部分不丢) ---
    if let Some(ext) = data.get("extensions") {
        if let Some(ch) = defs.iter_mut().find(|d| d.def_type == "char" && d.name == name) {
            ch.meta = serde_json::json!({ "st_raw": ext.clone() });
        }
    }

    LoreSet { template: tmpl, definitions: defs, nodes }
}
```

> 注:`prompt` 类型经现有 `resolve` 渲染为裸文本(无 `<tag>` 封包);before/after 由 sort_order 相对 history 决定,无需新机制。

- [ ] **Step 4: 删回出口**

- `charcard.rs`:确认已无 `def_to_charcard`(上面整体替换已移除)。
- `worldinfo.rs`:删 `pub fn defs_to_worldinfo(...)` 及其测试 `exports_defs_to_standalone_map`、`worldinfo_roundtrips`(后者用了 `defs_to_worldinfo`);若 `parse_trigger`/`TriggerMode` 仅被它使用导致 unused,保留(其它处仍用)。
- `lib.rs`:把 `charcard` 相关导出改为 `pub use adapters::charcard::{charcard_to_loreset, LoreSet};`,移除 `def_to_charcard`/`defs_to_worldinfo` 的 re-export(grep `def_to_charcard`/`defs_to_worldinfo` 清干净)。

- [ ] **Step 5: 编译并跑测试**

Run: `cargo test -p shirita-core --lib charcard worldinfo 2>&1 | tail -20`
Expected: PASS(新增两测 + 保留的 worldinfo 导入测);无未解析符号。

- [ ] **Step 6: 全 core 回归**

Run: `cargo build -p shirita-core && cargo test -p shirita-core`
Expected: PASS。`shirita-web` 若引用了已删的 `card_to_defs`/`charcard_to_defs`,Task 6 修;本步只需 core 绿。

- [ ] **Step 7: 提交**

```bash
git add shirita-core/src/adapters/charcard.rs shirita-core/src/adapters/worldinfo.rs shirita-core/src/lib.rs
git commit -m "feat(core): charcard_to_loreset — lossy ST card -> loreset translation

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 6: 导入路由落「定义 + template + nodes」

**Files:**
- Modify: `shirita-web/src/routes/import_export.rs`(`card_to_defs`/`import_charcard`/`import` 改造)
- Test: `shirita-web/tests/import_test.rs`(或新增 `import_charcard_test.rs`)

**Interfaces:**
- Consumes: `shirita_core::charcard_to_loreset`、`LoreSet`(Task 5);`persist_defs` 的去重思路。
- Produces: POST 角色卡 → 落库 N 定义 + 1 template + 其 nodes;ref 节点 `definition_id` 重映射到去重后实际入库 id。

- [ ] **Step 1: 写失败测试**

新增 `shirita-web/tests/import_charcard_test.rs`(沿用其它 web 测的 `test_state`/`send` 模式,`AppState{...}` 末尾含 `http_client: shirita_web::new_http_client()`):

```rust
use std::sync::Arc;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;
use shirita_core::{Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("import_card.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "m".into(), generations: Arc::new(shirita_web::Generations::new()), http_client: shirita_web::new_http_client() }
}

async fn send(state: &AppState, method: &str, uri: &str, body: Option<&str>) -> (StatusCode, String) {
    let mut b = Request::builder().method(method).uri(uri).header(header::AUTHORIZATION, "Bearer secret-token");
    let body = match body { Some(j) => { b = b.header(header::CONTENT_TYPE, "application/json"); Body::from(j.to_string()) } None => Body::empty() };
    let res = app(state.clone()).oneshot(b.body(body).unwrap()).await.unwrap();
    let st = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (st, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn import_charcard_creates_loreset() {
    let state = test_state().await;
    let card = r#"{"spec":"chara_card_v3","data":{"name":"Neo","description":"d","first_mes":"hi","character_book":{"entries":[{"keys":["zion"],"comment":"Zion","content":"c"}]},"extensions":{"regex_scripts":[{"scriptName":"r","findRegex":"a","replaceString":"b","markdownOnly":true}]}}}"#;
    let (st, _) = send(&state, "POST", "/api/import/charcard", Some(card)).await;
    assert_eq!(st, StatusCode::OK);

    let (_, defs) = send(&state, "GET", "/api/definitions", None).await;
    let defs: Value = serde_json::from_str(&defs).unwrap();
    let arr = defs.as_array().unwrap();
    assert!(arr.iter().any(|d| d["type"] == "char" && d["name"] == "Neo"));
    assert!(arr.iter().any(|d| d["type"] == "first_message"));
    assert!(arr.iter().any(|d| d["type"] == "world"));
    assert!(arr.iter().any(|d| d["type"] == "regex_rule"));

    let (_, tmpls) = send(&state, "GET", "/api/templates", None).await;
    let tmpls: Value = serde_json::from_str(&tmpls).unwrap();
    let t = tmpls.as_array().unwrap().iter().find(|t| t["name"] == "Neo").unwrap();
    let tid = t["id"].as_str().unwrap();
    let (_, nodes) = send(&state, "GET", &format!("/api/templates/{tid}/nodes?owner_kind=template"), None).await;
    let nodes: Value = serde_json::from_str(&nodes).unwrap();
    assert!(nodes.as_array().unwrap().iter().any(|n| n["kind"] == "history"));
}
```

(确认路由 `GET /api/templates` 存在;若名称不同,用对应列模板的端点。)

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p shirita-web --test import_charcard_test`
Expected: FAIL(编译错误:`card_to_defs`/`charcard_to_defs` 已删;或断言不满足)。

- [ ] **Step 3: 改造路由**

在 `import_export.rs` 加一个落库 helper(放在 `persist_defs` 附近):

```rust
use shirita_core::{charcard_to_loreset, LoreSet};

/// 落 LoreSet:定义按 name+def_type 去重(沿用 on_conflict=skip),ref 的 definition_id 重映射到实际入库 id,再落 template + nodes。
async fn persist_loreset(state: &AppState, ls: LoreSet, summary: &mut ImportSummary) -> Result<(), StatusCode> {
    let existing = state.storage.list_definitions().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut idmap: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for d in &ls.definitions {
        if let Some(ex) = existing.iter().find(|e| e.name == d.name && e.def_type == d.def_type) {
            idmap.insert(d.id.clone(), ex.id.clone());
            summary.skipped.push(item("definition", &ex.id, &ex.name));
        } else {
            state.storage.create_definition(d).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            idmap.insert(d.id.clone(), d.id.clone());
            summary.created.push(item("definition", &d.id, &d.name));
        }
    }
    state.storage.create_template(&ls.template).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    for n in ls.nodes {
        let mut n = n;
        if let Some(did) = n.definition_id.as_ref().and_then(|id| idmap.get(id)).cloned() {
            n.definition_id = Some(did);
        }
        state.storage.create_node(&n).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    summary.created.push(item("template", &ls.template.id, &ls.template.name));
    Ok(())
}
```

把 `import_charcard` 改为:

```rust
pub async fn import_charcard(State(state): State<AppState>, Json(card): Json<Value>) -> Result<Json<ImportSummary>, StatusCode> {
    let mut summary = ImportSummary::default();
    persist_loreset(&state, charcard_to_loreset(&card), &mut summary).await?;
    Ok(Json(summary))
}
```

在统一入口 `import`(`:114`)里,把判定为 card 的分支(`is_card`)从旧的 `card_to_defs` + `persist_defs` 改为 `persist_loreset(&state, charcard_to_loreset(&v), oc?...)`。worldinfo 分支不变。删除旧的 `card_to_defs` 函数(及对已删 `charcard_to_defs`/`def_to_charcard` 的引用)。

> `ImportSummary` 的字段名(`created`/`skipped`/`overwritten`)与 `item()` 沿用现状;若 `Default` 未派生,用现有构造方式。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p shirita-web --test import_charcard_test`
Expected: PASS。

- [ ] **Step 5: web 回归**

Run: `cargo test -p shirita-web`
Expected: PASS(旧 `import_test`/`import_export_test` 若引用旧 charcard 行为需相应更新)。

- [ ] **Step 6: 提交**

```bash
git add shirita-web/src/routes/import_export.rs shirita-web/tests/import_charcard_test.rs
git commit -m "feat(web): import ST card as a loreset (defs + template + nodes)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 7: 会话创建 seed anchor + 开场白

**Files:**
- Modify: `shirita-web/src/routes/sessions.rs`(`create_session` :39)
- Test: `shirita-web/tests/sessions_test.rs`(新增用例)

**Interfaces:**
- Consumes: `first_message` 定义(Task 1)、`Message.is_anchor`(Task 2)、render_vars。
- Produces: 带 first_message 的设定集 → 新会话消息树:`is_anchor` user(`<start>`)→ assistant 主开场白 + N 备选(swipes);`active_leaf` 指主。

- [ ] **Step 1: 写失败测试**

在 `sessions_test.rs` 加(沿用其 `test_state`/请求 helper;若无通用 helper,仿 `import_charcard_test` 的 `send`):

```rust
#[tokio::test]
async fn create_session_seeds_first_message_with_anchor() {
    let state = test_state().await;
    // 一个 first_message 定义 + 引用它的 template
    let fm = r#"{"type":"first_message","name":"g","content":"wake up","meta":{"alternate_greetings":["again"]}}"#;
    let (_, d) = send(&state, "POST", "/api/definitions", Some(fm)).await;
    let did = serde_json::from_str::<serde_json::Value>(&d).unwrap()["id"].as_str().unwrap().to_string();
    let (_, t) = send(&state, "POST", "/api/templates", Some(r#"{"name":"T"}"#)).await;
    let tid = serde_json::from_str::<serde_json::Value>(&t).unwrap()["id"].as_str().unwrap().to_string();
    let body = format!(r#"{{"kind":"ref","definition_id":"{did}"}}"#);
    send(&state, "POST", &format!("/api/templates/{tid}/nodes?owner_kind=template"), Some(&body)).await;

    let (st, s) = send(&state, "POST", "/api/sessions", Some(&format!(r#"{{"name":"s","template_id":"{tid}"}}"#))).await;
    assert_eq!(st, StatusCode::OK);
    let sid = serde_json::from_str::<serde_json::Value>(&s).unwrap()["id"].as_str().unwrap().to_string();

    let (_, msgs) = send(&state, "GET", &format!("/api/sessions/{sid}/messages"), None).await;
    let msgs: serde_json::Value = serde_json::from_str(&msgs).unwrap();
    let arr = msgs.as_array().unwrap();
    // anchor user + 2 个 assistant(主 + 备选)
    let anchor = arr.iter().find(|m| m["role"] == "user" && m["is_anchor"] == true).unwrap();
    let assistants: Vec<_> = arr.iter().filter(|m| m["role"] == "assistant").collect();
    assert_eq!(assistants.len(), 2);
    // 两条 assistant 都挂在 anchor 下(互为 swipes)
    for a in &assistants { assert_eq!(a["parent_id"], anchor["id"]); }
    assert!(assistants.iter().any(|a| a["raw_content"] == "wake up"));
    assert!(assistants.iter().any(|a| a["raw_content"] == "again"));
}
```

(确认列消息端点路径 `GET /api/sessions/{id}/messages` 与消息 JSON 含 `is_anchor`/`parent_id`/`role`/`raw_content` 字段;若消息序列化未含 `is_anchor`,在 `Message` 的序列化路径补出——它已是 `Message` 字段,axum `Json(Message)` 默认含。)

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p shirita-web --test sessions_test create_session_seeds_first_message_with_anchor`
Expected: FAIL(无 seeding,消息树为空)。

- [ ] **Step 3: 实现 seeding**

在 `create_session` 落库 session(`create_session(&session)` 之后)新增:取该 session template 的节点,找 first_message ref,seed 消息。在函数返回前插入:

```rust
    // seed 开场白:first_message ref → anchor user(<start>) + assistant 主/备选(swipes)
    if let Some(tid) = session.template_id.as_deref() {
        let nodes = state.storage.list_nodes(&shirita_core::OwnerKind::Template, tid).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut greeting: Option<(String, Vec<String>)> = None;
        for n in nodes.iter().filter(|n| n.kind == shirita_core::NodeKind::Ref) {
            if let Some(did) = &n.definition_id {
                if let Ok(Some(def)) = state.storage.get_definition(did).await {
                    if def.def_type == "first_message" {
                        let alts = def.meta.get("alternate_greetings").and_then(|v| v.as_array())
                            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                            .unwrap_or_default();
                        greeting = Some((def.content.clone(), alts));
                        break;
                    }
                }
            }
        }
        if let Some((first, alts)) = greeting {
            use shirita_core::models::message::Message;
            use shirita_core::Role;
            let mut anchor = Message::new(&session.id, None, Role::User, "<start>");
            anchor.is_anchor = true;
            state.storage.create_message(&anchor).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut greetings = vec![first];
            greetings.extend(alts);
            let mut first_id: Option<String> = None;
            for g in greetings {
                let m = Message::new(&session.id, Some(anchor.id.clone()), Role::Assistant, g);
                if first_id.is_none() { first_id = Some(m.id.clone()); }
                state.storage.create_message(&m).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
            if let Some(fid) = first_id {
                state.storage.set_session_active_leaf(&session.id, Some(&fid)).await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
        }
    }
```

(`render_vars` 暂可省略——seed 时变量状态即 schema 初值;若要严格,用 `shirita_core::render_vars(&g, &session.current_state)`。`set_session_active_leaf` 为现有 trait 方法。)

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p shirita-web --test sessions_test create_session_seeds_first_message_with_anchor`
Expected: PASS。

- [ ] **Step 5: 防 400 回归(anchor 在 prompt、不丢)**

确认 `conversation.rs` 的 context 过滤是 `!m.is_hidden`(anchor 的 is_hidden=false → 保留),故下一次生成历史以 user 起头。无需改动;若想加保险测试,可在 core 加一条「seed 后 send_message 的 req.messages 首条为 user」的集成测(可选)。

Run: `cargo test -p shirita-web`
Expected: PASS。

- [ ] **Step 6: 提交**

```bash
git add shirita-web/src/routes/sessions.rs shirita-web/tests/sessions_test.rs
git commit -m "feat(web): seed first message (anchor user + assistant swipes) on session create

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 8: UI 不渲染 `is_anchor` 消息

**Files:**
- Modify: `shirita-ui/src/api/types.ts`(`Message` 加 `is_anchor`)
- Modify: `shirita-ui/src/components/MessageList.vue`(过滤 anchor)
- Test: `shirita-ui/src/components/MessageList.test.ts`

**Interfaces:**
- Consumes: 后端消息 JSON 的 `is_anchor` 字段(Task 2/7)。
- Produces: anchor 消息不出现在聊天列表。

- [ ] **Step 1: 写失败测试**

在 `MessageList.test.ts` 加(沿用该文件已有挂载/桩数据风格):

```ts
it('does not render anchor messages', () => {
  const messages = [
    { id: 'a', role: 'user', raw_content: '<start>', is_anchor: true },
    { id: 'b', role: 'assistant', raw_content: 'wake up', is_anchor: false },
  ]
  const wrapper = mount(MessageList, { props: { messages, ...requiredProps } })
  expect(wrapper.text()).not.toContain('<start>')
  expect(wrapper.text()).toContain('wake up')
})
```

(`requiredProps` 用该测已有的最小 props 集;字段名按 `api/types.ts` 的 `Message`。)

- [ ] **Step 2: 跑测试确认失败**

Run: `cd shirita-ui && npx vitest run src/components/MessageList.test.ts -t "anchor"`
Expected: FAIL(`<start>` 被渲染)。

- [ ] **Step 3: 类型 + 过滤**

`api/types.ts` 的 `Message` interface 加:

```ts
  is_anchor: boolean
```

`MessageList.vue`:把渲染所用的消息源过滤掉 anchor。找到 `v-for` 遍历的列表(如 `messages` 或某 computed),改为基于一个排除 anchor 的 computed:

```ts
const visibleMessages = computed(() => props.messages.filter(m => !m.is_anchor))
```
并把模板 `v-for` 的数据源换成 `visibleMessages`(swipe 索引等若依赖原数组,确保用过滤后的列表一致)。

- [ ] **Step 4: 跑测试确认通过**

Run: `cd shirita-ui && npx vitest run src/components/MessageList.test.ts`
Expected: PASS。

- [ ] **Step 5: UI 套件回归**

Run: `cd shirita-ui && npx vitest run`
Expected: PASS。

- [ ] **Step 6: 提交**

```bash
git add shirita-ui/src/api/types.ts shirita-ui/src/components/MessageList.vue shirita-ui/src/components/MessageList.test.ts
git commit -m "feat(ui): hide is_anchor messages from chat list

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## 收尾:全栈回归

- [ ] **全工作区测试**

Run: `cargo test --workspace && (cd shirita-ui && npx vitest run)`
Expected: 全绿。

- [ ] **(可选)真卡冒烟**

用 `examples/怪谈社.json` POST `/api/import/charcard` → 200;据返回 template 新建会话 → 首消息为其 HTML 开场白原文(纯文本显示,HTML 渲染留下个 slice)。
