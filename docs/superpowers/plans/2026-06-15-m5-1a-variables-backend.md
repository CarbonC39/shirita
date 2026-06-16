# M5 Plan 1a — Dynamic Variables & State Sandbox (Backend) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the write side of variables — a pure Rust state sandbox (`<state_update>` tags → typed instruction set), per-branch snapshots folded during streaming, schema-backfilled effective-state reads, and the web endpoints to read state / declare per-chat variables.

**Architecture:** A new pure `state` module owns the schema types, the three-layer `effective_state` merge, tag parsing/stripping, and `apply_updates`. `conversation.rs` computes the active branch's effective state (used both as the assembly render-state and as the apply-time fold base), then folds the model's updates into the new message's `snapshot_state`. Web adds `GET …/state` (server-side merge) and `PUT …/local-variables`, and seeds `current_state` at session creation.

**Tech Stack:** Rust, `serde_json`, `regex` (already deps); Axum 0.8; sqlx runtime API (no migration — reuses `template.meta`, `session.current_state`, `session.override_config`, `message.snapshot_state`).

**Upstream spec:** `docs/superpowers/specs/2026-06-15-m5-variables-state-design.md`.

---

## File Structure

- `shirita-core/src/state.rs` — **create**: `VarType`, `VarDecl`, `system_variables`, `schema_initials`, `effective_state`, `resolve_schema`, `Action`, `Update`, `parse_state_updates`, `strip_state_tags`, `apply_updates`.
- `shirita-core/src/lib.rs` — **modify**: `pub mod state;` + re-exports.
- `shirita-core/src/conversation.rs` — **modify**: resolve schema + branch effective state; assembly reads it; fold updates into snapshots and strip tags in `send_message` + `regenerate`.
- `shirita-web/src/routes/variables.rs` — **create**: `get_state`, `set_local_variables`.
- `shirita-web/src/routes/sessions.rs` — **modify**: seed `current_state` on `create_session`.
- `shirita-web/src/routes/mod.rs`, `shirita-web/src/lib.rs` — **modify**: register routes.
- Tests: inline in `state.rs` and `conversation.rs`; `shirita-web/tests/variables_test.rs`.

---

## Task 1: `state` module — schema types + `effective_state`

**Files:**
- Create: `shirita-core/src/state.rs`
- Modify: `shirita-core/src/lib.rs`

- [ ] **Step 1: Write the failing test (inside the new file)**

Create `shirita-core/src/state.rs`:

```rust
//! 变量状态沙箱：声明 schema、合并有效状态、解析/应用 <state_update> 指令。
//! 纯函数、无 I/O；写侧（apply）与读侧（effective_state）共用同一 schema 兜底。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// 变量类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarType {
    Number,
    Bool,
    String,
    List,
}

/// 一条变量声明。`scope` 仅供前端分组（system/template/local），存储时可省略。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VarDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub var_type: VarType,
    #[serde(default)]
    pub initial: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// 内置系统变量注册表（保留 `$` 命名空间，恒存在，绑定到渲染行为）。
pub fn system_variables() -> Vec<VarDecl> {
    vec![
        VarDecl { name: "$avatar".into(), var_type: VarType::String, initial: Value::String(String::new()), scope: Some("system".into()) },
        VarDecl { name: "$background".into(), var_type: VarType::String, initial: Value::String(String::new()), scope: Some("system".into()) },
    ]
}

/// schema 的初值映射 {name: initial}。
pub fn schema_initials(schema: &[VarDecl]) -> Map<String, Value> {
    schema.iter().map(|d| (d.name.clone(), d.initial.clone())).collect()
}

/// 读侧唯一真相：schema 初值 < seed(session.current_state) < 分支叶子快照（后者覆盖前者）。
/// 保证新增变量在旧快照分支上回填初值，旧快照对 schema 增长免疫。
pub fn effective_state(schema: &[VarDecl], seed: &Value, leaf_snapshot: &Value) -> Value {
    let mut out = schema_initials(schema);
    if let Some(o) = seed.as_object() {
        for (k, v) in o { out.insert(k.clone(), v.clone()); }
    }
    if let Some(o) = leaf_snapshot.as_object() {
        for (k, v) in o { out.insert(k.clone(), v.clone()); }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Vec<VarDecl> {
        vec![
            VarDecl { name: "hp".into(), var_type: VarType::Number, initial: json!(100), scope: None },
            VarDecl { name: "gold".into(), var_type: VarType::Number, initial: json!(0), scope: None },
            VarDecl { name: "reputation".into(), var_type: VarType::Number, initial: json!(50), scope: None },
        ]
    }

    #[test]
    fn effective_state_backfills_new_vars_and_leaf_wins() {
        // seed predates `reputation`; leaf snapshot has evolved hp/gold but no reputation.
        let seed = json!({ "hp": 100, "gold": 0 });
        let leaf = json!({ "hp": 80, "gold": 30 });
        let eff = effective_state(&schema(), &seed, &leaf);
        assert_eq!(eff["hp"], 80);          // leaf wins
        assert_eq!(eff["gold"], 30);        // leaf wins
        assert_eq!(eff["reputation"], 50);  // backfilled from schema initial
    }

    #[test]
    fn seed_overrides_schema_initial_when_leaf_silent() {
        let seed = json!({ "hp": 120 });    // a session that started richer than the declared 100
        let leaf = json!({});
        let eff = effective_state(&schema(), &seed, &leaf);
        assert_eq!(eff["hp"], 120);         // seed beats schema initial
        assert_eq!(eff["gold"], 0);         // untouched -> schema initial
    }
}
```

- [ ] **Step 2: Wire the module + run the test**

In `shirita-core/src/lib.rs`, add `pub mod state;` next to the other `pub mod` declarations, and add to the `pub use` block:

```rust
pub use state::{
    apply_updates, effective_state, parse_state_updates, resolve_schema, strip_state_tags,
    system_variables, Update, VarDecl, VarType,
};
```

> `apply_updates`, `parse_state_updates`, `resolve_schema`, `strip_state_tags`, and `Update` are added in Tasks 2–4; the `pub use` line will not compile until then. To keep this task green on its own, **add only the symbols that exist so far** now:
> ```rust
> pub use state::{effective_state, schema_initials, system_variables, VarDecl, VarType};
> ```
> and extend the re-export in each later task.

Run: `cargo test -p shirita-core --lib state::`
Expected: PASS (2 tests).

- [ ] **Step 3: Commit**

```bash
git add shirita-core/src/state.rs shirita-core/src/lib.rs
git commit -m "feat(core): state module — VarDecl + effective_state merge"
```

---

## Task 2: `state` module — tag parsing + stripping

**Files:**
- Modify: `shirita-core/src/state.rs`

- [ ] **Step 1: Write the failing test**

Add to `shirita-core/src/state.rs` (above the `#[cfg(test)] mod tests` block, in module body):

```rust
/// 指令动作集。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Set,
    Add,
    Sub,
    Toggle,
    Append,
    Remove,
}

impl Action {
    fn parse(s: &str) -> Option<Action> {
        match s.to_ascii_uppercase().as_str() {
            "SET" => Some(Action::Set),
            "ADD" => Some(Action::Add),
            "SUB" => Some(Action::Sub),
            "TOGGLE" => Some(Action::Toggle),
            "APPEND" => Some(Action::Append),
            "REMOVE" => Some(Action::Remove),
            _ => None,
        }
    }
}

/// 一条解析后的状态更新指令。
#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    pub action: Action,
    pub key: String,
    pub value: Option<String>,
}

/// 从流式文本中提取所有 `<state_update action=".." key=".." value=".."/>`（按出现顺序）。
pub fn parse_state_updates(text: &str) -> Vec<Update> {
    let tag_re = regex::Regex::new(r#"(?is)<state_update\b([^>]*?)/?>"#).unwrap();
    let attr_re = regex::Regex::new(r#"(\w+)\s*=\s*"([^"]*)""#).unwrap();
    let mut out = Vec::new();
    for caps in tag_re.captures_iter(text) {
        let mut action = None;
        let mut key = None;
        let mut value = None;
        for a in attr_re.captures_iter(&caps[1]) {
            match a[1].to_ascii_lowercase().as_str() {
                "action" => action = Action::parse(&a[2]),
                "key" => key = Some(a[2].to_string()),
                "value" => value = Some(a[2].to_string()),
                _ => {}
            }
        }
        if let (Some(action), Some(key)) = (action, key) {
            out.push(Update { action, key, value });
        }
    }
    out
}

/// 移除所有 state_update 标签（用于展示文本）。
pub fn strip_state_tags(text: &str) -> String {
    let tag_re = regex::Regex::new(r#"(?is)<state_update\b[^>]*?/?>"#).unwrap();
    tag_re.replace_all(text, "").trim().to_string()
}
```

Add these tests inside `mod tests`:

```rust
    #[test]
    fn parses_multiple_updates_in_order() {
        let text = "You take a hit. <state_update action=\"SUB\" key=\"hp\" value=\"5\"/> \
                    <state_update action=\"TOGGLE\" key=\"alarmed\"/>";
        let ups = parse_state_updates(text);
        assert_eq!(ups.len(), 2);
        assert_eq!(ups[0], Update { action: Action::Sub, key: "hp".into(), value: Some("5".into()) });
        assert_eq!(ups[1], Update { action: Action::Toggle, key: "alarmed".into(), value: None });
    }

    #[test]
    fn strips_tags_from_display() {
        let text = "Hello there. <state_update action=\"SET\" key=\"$avatar\" value=\"a.png\"/>";
        assert_eq!(strip_state_tags(text), "Hello there.");
    }

    #[test]
    fn unknown_action_is_dropped() {
        assert!(parse_state_updates("<state_update action=\"NUKE\" key=\"hp\" value=\"1\"/>").is_empty());
    }
```

- [ ] **Step 2: Run to verify it passes**

Extend the `pub use` in `lib.rs` to add `parse_state_updates, strip_state_tags, Update`.

Run: `cargo test -p shirita-core --lib state::`
Expected: PASS (5 tests).

- [ ] **Step 3: Commit**

```bash
git add shirita-core/src/state.rs shirita-core/src/lib.rs
git commit -m "feat(core): state module — parse + strip <state_update> tags"
```

---

## Task 3: `state` module — `apply_updates` (typed fold)

**Files:**
- Modify: `shirita-core/src/state.rs`

- [ ] **Step 1: Write the failing test**

Add to `shirita-core/src/state.rs` module body:

```rust
fn num_value(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        Value::Number(serde_json::Number::from(n as i64))
    } else {
        serde_json::Number::from_f64(n).map(Value::Number).unwrap_or(Value::Null)
    }
}

fn coerce(value: &Option<String>, vt: VarType) -> Option<Value> {
    let s = value.as_ref()?;
    match vt {
        VarType::Number => s.parse::<f64>().ok().map(num_value),
        VarType::Bool => match s.to_ascii_lowercase().as_str() {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
        VarType::String => Some(Value::String(s.clone())),
        VarType::List => serde_json::from_str::<Vec<Value>>(s).ok().map(Value::Array),
    }
}

/// 按 schema 类型逐条应用更新；未声明的 key 或类型不符的动作一律忽略（沙箱不执行代码）。
pub fn apply_updates(state: &Value, schema: &[VarDecl], updates: &[Update]) -> Value {
    let mut obj = state.as_object().cloned().unwrap_or_default();
    for u in updates {
        let Some(vt) = schema.iter().find(|d| d.name == u.key).map(|d| d.var_type) else {
            continue; // 未声明
        };
        match (u.action, vt) {
            (Action::Set, _) => {
                if let Some(v) = coerce(&u.value, vt) {
                    obj.insert(u.key.clone(), v);
                }
            }
            (Action::Add, VarType::Number) | (Action::Sub, VarType::Number) => {
                let cur = obj.get(&u.key).and_then(|v| v.as_f64()).unwrap_or(0.0);
                if let Some(n) = u.value.as_ref().and_then(|s| s.parse::<f64>().ok()) {
                    let next = if u.action == Action::Add { cur + n } else { cur - n };
                    obj.insert(u.key.clone(), num_value(next));
                }
            }
            (Action::Toggle, VarType::Bool) => {
                let cur = obj.get(&u.key).and_then(|v| v.as_bool()).unwrap_or(false);
                obj.insert(u.key.clone(), Value::Bool(!cur));
            }
            (Action::Append, VarType::List) => {
                if let Some(val) = &u.value {
                    let mut arr = obj.get(&u.key).and_then(|v| v.as_array().cloned()).unwrap_or_default();
                    arr.push(Value::String(val.clone()));
                    obj.insert(u.key.clone(), Value::Array(arr));
                }
            }
            (Action::Remove, VarType::List) => {
                if let Some(val) = &u.value {
                    let mut arr = obj.get(&u.key).and_then(|v| v.as_array().cloned()).unwrap_or_default();
                    if let Some(pos) = arr.iter().position(|e| e.as_str() == Some(val.as_str())) {
                        arr.remove(pos);
                    }
                    obj.insert(u.key.clone(), Value::Array(arr));
                }
            }
            _ => {} // 动作/类型不匹配
        }
    }
    Value::Object(obj)
}
```

Add tests inside `mod tests`:

```rust
    fn full_schema() -> Vec<VarDecl> {
        vec![
            VarDecl { name: "hp".into(), var_type: VarType::Number, initial: json!(100), scope: None },
            VarDecl { name: "alarmed".into(), var_type: VarType::Bool, initial: json!(false), scope: None },
            VarDecl { name: "name".into(), var_type: VarType::String, initial: json!(""), scope: None },
            VarDecl { name: "bag".into(), var_type: VarType::List, initial: json!([]), scope: None },
        ]
    }

    #[test]
    fn applies_typed_actions_and_ignores_invalid() {
        let s = full_schema();
        let st = json!({ "hp": 100, "alarmed": false, "bag": [] });
        let ups = vec![
            Update { action: Action::Sub, key: "hp".into(), value: Some("30".into()) },
            Update { action: Action::Toggle, key: "alarmed".into(), value: None },
            Update { action: Action::Set, key: "name".into(), value: Some("Ada".into()) },
            Update { action: Action::Append, key: "bag".into(), value: Some("key".into()) },
            Update { action: Action::Add, key: "hp".into(), value: Some("oops".into()) }, // non-numeric -> ignored
            Update { action: Action::Set, key: "ghost".into(), value: Some("x".into()) },  // undeclared -> ignored
        ];
        let out = apply_updates(&st, &s, &ups);
        assert_eq!(out["hp"], 70);
        assert_eq!(out["alarmed"], true);
        assert_eq!(out["name"], "Ada");
        assert_eq!(out["bag"], json!(["key"]));
        assert!(out.get("ghost").is_none());
    }

    #[test]
    fn remove_drops_first_match() {
        let s = full_schema();
        let st = json!({ "bag": ["key", "map", "key"] });
        let out = apply_updates(&st, &s, &[Update { action: Action::Remove, key: "bag".into(), value: Some("key".into()) }]);
        assert_eq!(out["bag"], json!(["map", "key"]));
    }
```

- [ ] **Step 2: Run to verify it passes**

Extend the `pub use` in `lib.rs` to add `apply_updates`.

Run: `cargo test -p shirita-core --lib state::`
Expected: PASS (7 tests).

- [ ] **Step 3: Commit**

```bash
git add shirita-core/src/state.rs shirita-core/src/lib.rs
git commit -m "feat(core): state module — typed apply_updates fold"
```

---

## Task 4: `state` module — `resolve_schema`

**Files:**
- Modify: `shirita-core/src/state.rs`

- [ ] **Step 1: Write the failing test**

Add to `shirita-core/src/state.rs` module body:

```rust
fn parse_decls(v: Option<&Value>, scope: &str) -> Vec<VarDecl> {
    let Some(arr) = v.and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let mut d: VarDecl = serde_json::from_value(item.clone()).ok()?;
            d.scope = Some(scope.to_string());
            Some(d)
        })
        .collect()
}

fn merge_decls(out: &mut Vec<VarDecl>, decls: Vec<VarDecl>) {
    for d in decls {
        if let Some(existing) = out.iter_mut().find(|x| x.name == d.name) {
            *existing = d; // 后者覆盖（precedence: system < template < local）
        } else {
            out.push(d);
        }
    }
}

/// 解析会话的有效 schema：系统 ∪ 模板 `meta.variables` ∪ 会话 `override_config.local_variables`。
pub fn resolve_schema(template_meta: Option<&Value>, override_config: &Value) -> Vec<VarDecl> {
    let mut out = system_variables();
    merge_decls(&mut out, parse_decls(template_meta.and_then(|m| m.get("variables")), "template"));
    merge_decls(&mut out, parse_decls(override_config.get("local_variables"), "local"));
    out
}
```

Add tests inside `mod tests`:

```rust
    #[test]
    fn resolve_schema_unions_system_template_local() {
        let tmeta = json!({ "variables": [ {"name":"hp","type":"number","initial":100} ] });
        let cfg = json!({ "local_variables": [ {"name":"reputation","type":"number","initial":0} ] });
        let s = resolve_schema(Some(&tmeta), &cfg);
        let names: Vec<&str> = s.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"$avatar"));   // system always present
        assert!(names.contains(&"hp"));        // template
        assert!(names.contains(&"reputation")); // local
        assert_eq!(s.iter().find(|d| d.name == "hp").unwrap().scope.as_deref(), Some("template"));
    }

    #[test]
    fn local_overrides_template_on_name_clash() {
        let tmeta = json!({ "variables": [ {"name":"hp","type":"number","initial":100} ] });
        let cfg = json!({ "local_variables": [ {"name":"hp","type":"number","initial":250} ] });
        let s = resolve_schema(Some(&tmeta), &cfg);
        let hp = s.iter().find(|d| d.name == "hp").unwrap();
        assert_eq!(hp.initial, 250);
        assert_eq!(hp.scope.as_deref(), Some("local"));
    }
```

- [ ] **Step 2: Run to verify it passes**

Extend the `pub use` in `lib.rs` to add `resolve_schema`.

Run: `cargo test -p shirita-core --lib state::`
Expected: PASS (9 tests).

- [ ] **Step 3: Commit**

```bash
git add shirita-core/src/state.rs shirita-core/src/lib.rs
git commit -m "feat(core): state module — resolve_schema (system/template/local)"
```

---

## Task 5: `send_message` folds state into snapshots + reads branch state

**Files:**
- Modify: `shirita-core/src/conversation.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` in `shirita-core/src/conversation.rs` (it already has `temp_storage`, `RecordingProvider`, and uses `serde_json::json`):

```rust
    #[tokio::test]
    async fn state_update_folds_into_snapshot_and_strips_display() {
        let storage = Arc::new(temp_storage().await);
        let mut t = crate::models::template::Template::new("T");
        t.meta = serde_json::json!({ "variables": [ {"name":"hp","type":"number","initial":100} ] });
        storage.create_template(&t).await.unwrap();
        let mut session = Session::new("s");
        session.template_id = Some(t.id.clone());
        session.current_state = serde_json::json!({ "hp": 100 });
        storage.create_session(&session).await.unwrap();

        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider {
            seen: Arc::new(Mutex::new(None)),
            reply: "You take a hit. <state_update action=\"SUB\" key=\"hp\" value=\"5\"/>".into(),
        });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "hi".into());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let msgs = storage.list_messages(&session.id).await.unwrap();
        let assistant = msgs.iter().find(|m| m.role == Role::Assistant).unwrap();
        assert_eq!(assistant.snapshot_state["hp"], 95);                         // folded
        assert_eq!(assistant.display_content.as_deref(), Some("You take a hit.")); // tag stripped
        assert!(assistant.raw_content.contains("<state_update"));               // raw keeps the tag
    }

    #[tokio::test]
    async fn assembly_renders_the_active_branch_state() {
        let storage = Arc::new(temp_storage().await);
        // a char definition that renders {{hp}}
        let ch = crate::models::definition::Definition::new("char", "C", "HP is {{hp}}");
        storage.create_definition(&ch).await.unwrap();
        let t = crate::models::template::Template::new("T");
        storage.create_template(&t).await.unwrap();
        let f = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "char");
        storage.create_node(&f).await.unwrap();
        storage.create_node(&PromptNode::new_ref(OwnerKind::Template, &t.id, Some(f.id.clone()), 0, &ch.id)).await.unwrap();

        let mut session = Session::new("s");
        session.template_id = Some(t.id.clone());
        session.current_state = serde_json::json!({ "hp": 100 });
        storage.create_session(&session).await.unwrap();

        // seed an existing assistant leaf whose snapshot has hp=42, and point the leaf at it
        let mut leaf = Message::new(&session.id, None, Role::Assistant, "prior");
        leaf.snapshot_state = serde_json::json!({ "hp": 42 });
        storage.create_message(&leaf).await.unwrap();
        storage.set_session_active_leaf(&session.id, Some(&leaf.id)).await.unwrap();

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "hi".into());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let req = seen.lock().unwrap().clone().unwrap();
        assert!(req.messages[0].content.contains("HP is 42"), "assembly must read the branch leaf snapshot");
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p shirita-core --lib state_update_folds_into_snapshot_and_strips_display`
Expected: FAIL — snapshot stays `{}`, display keeps the tag, assembly renders `HP is 100` (seed) not `42`.

- [ ] **Step 3: Add the schema/state helper + thread state through assembly**

In `shirita-core/src/conversation.rs`, add imports near the top:

```rust
use crate::state::{apply_updates, effective_state, parse_state_updates, resolve_schema, strip_state_tags, VarDecl};
```

Add a helper near `effective_nodes`:

```rust
/// 解析会话的有效变量 schema（系统 ∪ 模板 meta ∪ 会话 local）。
async fn session_schema(storage: &dyn Storage, session: &Session) -> Vec<VarDecl> {
    let template_meta = match &session.template_id {
        Some(tid) => storage.get_template(tid).await.ok().flatten().map(|t| t.meta),
        None => None,
    };
    resolve_schema(template_meta.as_ref(), &session.override_config)
}
```

Change `assemble_request`'s signature to take the render state explicitly. Replace:

```rust
async fn assemble_request(
    storage: &dyn Storage,
    session: &Session,
    model: String,
    context: &[ChatMessage],
) -> crate::Result<(ChatRequest, Vec<Definition>)> {
```

with:

```rust
async fn assemble_request(
    storage: &dyn Storage,
    session: &Session,
    model: String,
    context: &[ChatMessage],
    state: &serde_json::Value,
) -> crate::Result<(ChatRequest, Vec<Definition>)> {
```

and inside it replace the `&session.current_state` argument to `assemble_from_nodes` with `state`:

```rust
    let plan = crate::assembly::assemble_from_nodes(
        &nodes,
        &defs,
        &local,
        state,
        &recent,
        &mut || rand::Rng::gen::<f64>(&mut rng),
    );
```

- [ ] **Step 4: Compute the branch state + fold in `send_message`**

In `send_message`'s `async_stream!`, after `let path = crate::tree::active_path(&all, session.active_leaf_id.as_deref());`, add:

```rust
        let schema = session_schema(storage.as_ref(), &session).await;
        let leaf_snapshot = path.last().map(|m| m.snapshot_state.clone()).unwrap_or_else(|| serde_json::json!({}));
        let branch_state = effective_state(&schema, &session.current_state, &leaf_snapshot);
```

Set the user message's snapshot to carry the branch state forward — change the `Message::new(...user...)` block to:

```rust
        let mut user_msg = Message::new(&session_id, parent_id, Role::User, &user_text);
        user_msg.snapshot_state = branch_state.clone();
        if let Err(e) = storage.create_message(&user_msg).await {
            yield SendEvent::Error(e.to_string());
            return;
        }
```

Pass `branch_state` into the assemble call:

```rust
        let (req, regex_rules) = match assemble_request(storage.as_ref(), &session, model, &context, &branch_state).await {
            Ok(r) => r,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
```

Replace the assistant-persist block (step 4 of the stream) with the fold + strip:

```rust
        let updates = parse_state_updates(&full);
        let new_snapshot = apply_updates(&branch_state, &schema, &updates);
        let cleaned = strip_state_tags(&full);
        let mut assistant = Message::new(&session_id, Some(user_msg.id.clone()), Role::Assistant, &full);
        assistant.snapshot_state = new_snapshot;
        assistant.display_content = match crate::assembly::apply_regex_rules(&cleaned, &regex_rules) {
            Some(s) => Some(s),
            None => if cleaned != full { Some(cleaned) } else { None },
        };
        if let Err(e) = storage.create_message(&assistant).await {
            yield SendEvent::Error(e.to_string());
            return;
        }
        let _ = storage.set_session_active_leaf(&session_id, Some(&assistant.id)).await;
        yield SendEvent::Done { message_id: assistant.id };
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p shirita-core --lib`
Expected: PASS — the two new tests plus the existing `send_chains_under_active_leaf_and_updates_it`, `regex_rule_sets_display_content`, etc. (Assembly behavior is unchanged for sessions with empty state — `effective_state` of an empty schema/seed/leaf is `{}`.)

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/conversation.rs
git commit -m "feat(core): send_message folds <state_update> into per-branch snapshots"
```

---

## Task 6: `regenerate` folds state into the sibling snapshot

**Files:**
- Modify: `shirita-core/src/conversation.rs`

- [ ] **Step 1: Write the failing test**

Add to `#[cfg(test)] mod tests`:

```rust
    #[tokio::test]
    async fn regenerate_folds_state_from_the_parent_branch() {
        let storage = Arc::new(temp_storage().await);
        let mut t = crate::models::template::Template::new("T");
        t.meta = serde_json::json!({ "variables": [ {"name":"hp","type":"number","initial":100} ] });
        storage.create_template(&t).await.unwrap();
        let mut session = Session::new("s");
        session.template_id = Some(t.id.clone());
        session.current_state = serde_json::json!({ "hp": 100 });
        storage.create_session(&session).await.unwrap();

        // user -> assistant(hp 90); regenerate the assistant from the same parent
        let user = Message::new(&session.id, None, Role::User, "go");
        storage.create_message(&user).await.unwrap();
        let mut a1 = Message::new(&session.id, Some(user.id.clone()), Role::Assistant, "first");
        a1.snapshot_state = serde_json::json!({ "hp": 90 });
        storage.create_message(&a1).await.unwrap();
        storage.set_session_active_leaf(&session.id, Some(&a1.id)).await.unwrap();

        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider {
            seen: Arc::new(Mutex::new(None)),
            reply: "retry <state_update action=\"SUB\" key=\"hp\" value=\"20\"/>".into(),
        });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let stream = regenerate(storage_dyn, provider, counter, "m".into(), session.id.clone(), a1.id.clone());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let msgs = storage.list_messages(&session.id).await.unwrap();
        let sibling = msgs.iter().filter(|m| m.role == Role::Assistant).find(|m| m.id != a1.id).unwrap();
        // parent branch state at the user turn is hp=100 (the user carries the seed); SUB 20 -> 80
        assert_eq!(sibling.snapshot_state["hp"], 80);
        assert_eq!(sibling.display_content.as_deref(), Some("retry"));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-core --lib regenerate_folds_state_from_the_parent_branch`
Expected: FAIL — sibling snapshot is `{}`, display keeps the tag.

- [ ] **Step 3: Apply the same fold in `regenerate`**

In `regenerate`'s `async_stream!`, after `let path = crate::tree::active_path(&all, target.parent_id.as_deref());`, add:

```rust
        let schema = session_schema(storage.as_ref(), &session).await;
        let leaf_snapshot = path.last().map(|m| m.snapshot_state.clone()).unwrap_or_else(|| serde_json::json!({}));
        let branch_state = effective_state(&schema, &session.current_state, &leaf_snapshot);
```

Pass `&branch_state` into `assemble_request`:

```rust
        let (req, regex_rules) = match assemble_request(storage.as_ref(), &session, model, &context, &branch_state).await {
            Ok(r) => r,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
```

Replace the sibling-persist block with the fold + strip:

```rust
        let updates = parse_state_updates(&full);
        let new_snapshot = apply_updates(&branch_state, &schema, &updates);
        let cleaned = strip_state_tags(&full);
        let mut sibling = Message::new(&session_id, target.parent_id.clone(), Role::Assistant, &full);
        sibling.snapshot_state = new_snapshot;
        sibling.display_content = match crate::assembly::apply_regex_rules(&cleaned, &regex_rules) {
            Some(s) => Some(s),
            None => if cleaned != full { Some(cleaned) } else { None },
        };
        if let Err(e) = storage.create_message(&sibling).await {
            yield SendEvent::Error(e.to_string());
            return;
        }
        let _ = storage.set_session_active_leaf(&session_id, Some(&sibling.id)).await;
        yield SendEvent::Done { message_id: sibling.id };
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p shirita-core --lib`
Expected: PASS (all core tests).

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/conversation.rs
git commit -m "feat(core): regenerate folds <state_update> into the sibling snapshot"
```

---

## Task 7: Seed `current_state` on session creation

**Files:**
- Modify: `shirita-web/src/routes/sessions.rs`
- Test: `shirita-web/tests/variables_test.rs`

- [ ] **Step 1: Write the failing test**

Create `shirita-web/tests/variables_test.rs` (copy the `test_state`/`send`/`json`/`create` harness from `shirita-web/tests/local_overrides_test.rs`, including the `generations:` field), then add:

```rust
async fn create_template(state: &AppState, name: &str, meta: &str) -> String {
    let (_, out) = send(state, "POST", "/api/templates", Some(&format!(r#"{{"name":"{name}","meta":{meta}}}"#))).await;
    json(&out)["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn creating_a_session_seeds_declared_initials() {
    let state = test_state().await;
    let tid = create_template(&state, "RPG", r#"{"variables":[{"name":"hp","type":"number","initial":100}]}"#).await;
    let (st, out) = send(&state, "POST", "/api/sessions",
        Some(&format!(r#"{{"name":"Chat","template_id":"{tid}"}}"#))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&out)["current_state"]["hp"], 100);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-web --test variables_test creating_a_session_seeds`
Expected: FAIL — `current_state` is `{}`.

- [ ] **Step 3: Seed in the handler**

In `shirita-web/src/routes/sessions.rs`, add the import:

```rust
use shirita_core::state::{resolve_schema, schema_initials};
```

In `create_session`, after `session.template_id = body.template_id.clone();` and before `create_session` is persisted, seed the state:

```rust
    let template_meta = match &session.template_id {
        Some(tid) => state.storage.get_template(tid).await.ok().flatten().map(|t| t.meta),
        None => None,
    };
    let schema = resolve_schema(template_meta.as_ref(), &session.override_config);
    session.current_state = serde_json::Value::Object(schema_initials(&schema));
```

> `schema_initials` returns a `serde_json::Map`; wrap it in `Value::Object`. Add `pub use state::schema_initials;` to `shirita-core/src/lib.rs` if you prefer the flat path, or use `shirita_core::state::schema_initials` as above.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p shirita-web --test variables_test creating_a_session_seeds`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-web/src/routes/sessions.rs shirita-web/tests/variables_test.rs
git commit -m "feat(web): seed current_state from declared variable initials on session create"
```

---

## Task 8: `GET /api/sessions/{id}/state`

**Files:**
- Create: `shirita-web/src/routes/variables.rs`
- Modify: `shirita-web/src/routes/mod.rs`, `shirita-web/src/lib.rs`
- Test: `shirita-web/tests/variables_test.rs`

- [ ] **Step 1: Write the failing test**

Add to `shirita-web/tests/variables_test.rs`:

```rust
#[tokio::test]
async fn get_state_merges_schema_seed_and_leaf() {
    let state = test_state().await;
    let tid = create_template(&state, "RPG", r#"{"variables":[{"name":"hp","type":"number","initial":100},{"name":"gold","type":"number","initial":0}]}"#).await;
    let (_, sout) = send(&state, "POST", "/api/sessions", Some(&format!(r#"{{"name":"Chat","template_id":"{tid}"}}"#))).await;
    let sid = json(&sout)["id"].as_str().unwrap().to_string();
    // a turn that spends gold and is hit (EchoProvider can't emit tags; assert the schema/seed path instead)
    let (_, state_out) = send(&state, "GET", &format!("/api/sessions/{sid}/state"), None).await;
    let body = json(&state_out);
    assert_eq!(body["values"]["hp"], 100);   // seeded
    assert_eq!(body["values"]["gold"], 0);
    let names: Vec<&str> = body["schema"].as_array().unwrap().iter().map(|d| d["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"$avatar"));      // system var present in schema
    assert!(names.contains(&"hp"));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-web --test variables_test get_state_merges`
Expected: FAIL — route not registered (404).

- [ ] **Step 3: Implement the handler**

Create `shirita-web/src/routes/variables.rs`:

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use shirita_core::state::{effective_state, resolve_schema};
use shirita_core::tree::active_path;

use crate::AppState;

/// 返回当前激活分支的有效变量状态 + schema（合并在服务端完成，单一真相）。
pub async fn get_state(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let session = state.storage.get_session(&id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let template_meta = match &session.template_id {
        Some(tid) => state.storage.get_template(tid).await.ok().flatten().map(|t| t.meta),
        None => None,
    };
    let schema = resolve_schema(template_meta.as_ref(), &session.override_config);
    let all = state.storage.list_messages(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let leaf = active_path(&all, session.active_leaf_id.as_deref())
        .last()
        .map(|m| m.snapshot_state.clone())
        .unwrap_or_else(|| json!({}));
    let values = effective_state(&schema, &session.current_state, &leaf);
    Ok(Json(json!({ "schema": schema, "values": values })))
}
```

In `shirita-web/src/routes/mod.rs` add `pub mod variables;`. In `shirita-web/src/lib.rs`:

```rust
        .route("/sessions/{id}/state", get(routes::variables::get_state))
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p shirita-web --test variables_test get_state_merges`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-web/src/routes/variables.rs shirita-web/src/routes/mod.rs shirita-web/src/lib.rs shirita-web/tests/variables_test.rs
git commit -m "feat(web): GET /sessions/{id}/state — effective branch state + schema"
```

---

## Task 9: `PUT /api/sessions/{id}/local-variables`

**Files:**
- Modify: `shirita-web/src/routes/variables.rs`, `shirita-web/src/lib.rs`
- Test: `shirita-web/tests/variables_test.rs`

- [ ] **Step 1: Write the failing test**

Add to `shirita-web/tests/variables_test.rs`:

```rust
#[tokio::test]
async fn set_local_variables_adds_to_the_schema() {
    let state = test_state().await;
    let tid = create_template(&state, "RPG", r#"{"variables":[{"name":"hp","type":"number","initial":100}]}"#).await;
    let (_, sout) = send(&state, "POST", "/api/sessions", Some(&format!(r#"{{"name":"Chat","template_id":"{tid}"}}"#))).await;
    let sid = json(&sout)["id"].as_str().unwrap().to_string();

    let (st, _) = send(&state, "PUT", &format!("/api/sessions/{sid}/local-variables"),
        Some(r#"{"variables":[{"name":"reputation","type":"number","initial":5}]}"#)).await;
    assert_eq!(st, StatusCode::OK);

    let (_, state_out) = send(&state, "GET", &format!("/api/sessions/{sid}/state"), None).await;
    let body = json(&state_out);
    assert_eq!(body["values"]["reputation"], 5);   // backfilled from the new local schema initial
    let names: Vec<&str> = body["schema"].as_array().unwrap().iter().map(|d| d["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"reputation"));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-web --test variables_test set_local_variables`
Expected: FAIL — route not registered (404/405).

- [ ] **Step 3: Implement the handler**

Append to `shirita-web/src/routes/variables.rs`:

```rust
use serde::Deserialize;

#[derive(Deserialize)]
pub struct LocalVarsBody {
    pub variables: Value, // a JSON array of {name,type,initial}
}

/// 替换会话本地变量声明（存于 override_config.local_variables）。
pub async fn set_local_variables(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<LocalVarsBody>,
) -> Result<StatusCode, StatusCode> {
    let session = state.storage.get_session(&id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let mut cfg = session.override_config.clone();
    if !cfg.is_object() {
        cfg = json!({});
    }
    cfg.as_object_mut().unwrap().insert("local_variables".into(), body.variables);
    state.storage.update_session_override_config(&id, &cfg).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}
```

In `shirita-web/src/lib.rs`:

```rust
        .route("/sessions/{id}/local-variables", put(routes::variables::set_local_variables))
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p shirita-web --test variables_test`
Expected: PASS (all three tests).

- [ ] **Step 5: Verify the whole workspace is green**

Run: `cargo test --workspace`
Expected: PASS, zero warnings (`cargo build --workspace` clean).

- [ ] **Step 6: Commit**

```bash
git add shirita-web/src/routes/variables.rs shirita-web/src/lib.rs shirita-web/tests/variables_test.rs
git commit -m "feat(web): PUT /sessions/{id}/local-variables — per-chat variable schema"
```

---

## Self-Review Checklist

- **Spec coverage:** sandbox parse/strip/apply (T2/T3) ✓; typed instruction set SET/ADD/SUB/TOGGLE/APPEND/REMOVE (T3) ✓; pre-declared+typed schema, undeclared ignored (T3/T4) ✓; system variables `$avatar`/`$background` (T1) ✓; template + per-chat schema resolution (T4/T9) ✓; effective-state 3-layer merge used at read + fold base (T1/T5/T6/T8) ✓; per-branch snapshots in send + regenerate (T5/T6) ✓; seed on creation (T7) ✓; `GET …/state` server-side merge (T8) ✓; `PUT …/local-variables` (T9) ✓. Deferred per spec: native tool_calls, recompute-on-edit, manual edit (none implemented here).
- **Placeholders:** none — every step has concrete code/commands.
- **Type consistency:** `effective_state(&[VarDecl], &Value, &Value) -> Value`, `resolve_schema(Option<&Value>, &Value) -> Vec<VarDecl>`, `apply_updates(&Value, &[VarDecl], &[Update]) -> Value`, `parse_state_updates(&str) -> Vec<Update>`, `strip_state_tags(&str) -> String` are used identically across T1–T9; `assemble_request` gains a trailing `state: &Value` arg, updated at both call sites (T5/T6).
- **Open verification points for the implementer:** the exact field set of `RecordingProvider`/`temp_storage` in `conversation.rs` (reuse as-is); whether `lib.rs` re-exports should be flat (`shirita_core::schema_initials`) or module-pathed (`shirita_core::state::schema_initials`) — either works, pick one and be consistent; confirm `Storage::get_template` returns `Option<Template>` (it does, used in `routes/templates.rs`).
