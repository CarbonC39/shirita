# M7 Plan 1 — core 解析与编解码 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 shirita-core 实现 M7 的纯函数地基：从 PNG 提取内嵌角色卡 JSON（带大小上限）、Shirita 原创格式编解码（单定义 + 模板 bundle）、定义名 XML 包裹（`sanitize_tag` + 组装集成）。

**Architecture:** 三块纯函数，互不依赖存储：`pngcard`（只读 PNG tEXt chunk）、`portable`（原创格式 JSON ↔ 结构体，用 `local_id` 解耦真实 UUID）、`assembly::sanitize_tag` + `resolve()` 包裹钩子。全部可单测，Plan 2/3 的 web 层在其上编排落库。

**Tech Stack:** Rust、serde_json、base64（新依赖）。无新迁移。

**Upstream spec:** `docs/superpowers/specs/2026-06-16-m7-import-export-design.md`（§3 pngcard、§4 portable、§5 定义名包裹）。

---

## File Structure

- `shirita-core/Cargo.toml` — **modify**：加 `base64` 依赖。
- `shirita-core/src/pngcard.rs` — **create**：`read_card_json` + `MAX_TEXT_CHUNK` + 单测。
- `shirita-core/src/portable.rs` — **create**：`export_definition`/`export_template`/`parse_portable` + 类型 + 单测。
- `shirita-core/src/assembly.rs` — **modify**：`sanitize_tag` 纯函数 + `resolve()` 包裹钩子 + 单测。
- `shirita-core/src/lib.rs` — **modify**：`pub mod pngcard; pub mod portable;` + re-export。

---

## Task 1: 加 `base64` 依赖 + `pngcard::read_card_json`

**Files:**
- Modify: `shirita-core/Cargo.toml`
- Create: `shirita-core/src/pngcard.rs`
- Modify: `shirita-core/src/lib.rs`

- [ ] **Step 1: 加依赖**

`shirita-core/Cargo.toml` 的 `[dependencies]` 末尾（`tracing.workspace = true` 之后）加一行：

```toml
base64 = "0.22"
```

- [ ] **Step 2: 写 `pngcard.rs`（含失败测试）**

创建 `shirita-core/src/pngcard.rs`：

```rust
//! 只读解析 PNG 内嵌的 SillyTavern 角色卡 JSON（tEXt chunk）。纯函数，不解码像素、不触库。
//!
//! PNG = 8 字节签名 + 一串 chunk：`length(4 BE) | type(4) | data(length) | crc(4)`。
//! ST 把卡 JSON 以 base64 存在 tEXt chunk，keyword `chara`（V2）或 `ccv3`（V3）；
//! tEXt data = `keyword\0text`。

use base64::Engine;

use crate::{Error, Result};

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// 单个 tEXt chunk 的大小上限（Sanity Limit）：纯文本角色卡远小于此。
/// 超过则报错且**绝不**按声明长度预分配，保护服务器内存。
pub const MAX_TEXT_CHUNK: usize = 8 * 1024 * 1024;

/// 从 PNG 字节提取内嵌角色卡 JSON：扫 tEXt chunk，keyword 优先 `ccv3` 再 `chara`，
/// base64 解码其 text 为 UTF-8 JSON。非 PNG / 无匹配 / 超限 / 解码失败 → Err。
pub fn read_card_json(png: &[u8]) -> Result<serde_json::Value> {
    if png.len() < 8 || png[..8] != PNG_SIGNATURE {
        return Err(Error::Config("not a PNG file".into()));
    }
    let mut pos = 8usize;
    let mut chara: Option<String> = None;
    let mut ccv3: Option<String> = None;
    while pos + 8 <= png.len() {
        let len = u32::from_be_bytes([png[pos], png[pos + 1], png[pos + 2], png[pos + 3]]) as usize;
        let ctype = &png[pos + 4..pos + 8];
        // 越过 length 上限 → 直接报错，不读、不分配。
        if len > MAX_TEXT_CHUNK {
            return Err(Error::Config(format!("PNG chunk too large: {len} bytes")));
        }
        let data_start = pos + 8;
        let data_end = data_start + len;
        if data_end + 4 > png.len() {
            break; // 截断/损坏，停止扫描
        }
        if ctype == b"tEXt" {
            let data = &png[data_start..data_end];
            if let Some(nul) = data.iter().position(|&b| b == 0) {
                let keyword = &data[..nul];
                let text = String::from_utf8_lossy(&data[nul + 1..]).to_string();
                if keyword == b"ccv3" {
                    ccv3 = Some(text);
                } else if keyword == b"chara" {
                    chara = Some(text);
                }
            }
        }
        if ctype == b"IEND" {
            break;
        }
        pos = data_end + 4; // 跳过 CRC
    }
    let b64 = ccv3.or(chara).ok_or_else(|| Error::Config("no character card tEXt chunk".into()))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| Error::Config(format!("base64 decode failed: {e}")))?;
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| Error::Config(format!("card JSON parse failed: {e}")))?;
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    /// 造一个最小 PNG：签名 + 一个 tEXt(keyword) + IEND。CRC 置零（read 不校验）。
    fn png_with_text(keyword: &str, text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&PNG_SIGNATURE);
        let mut data = Vec::new();
        data.extend_from_slice(keyword.as_bytes());
        data.push(0);
        data.extend_from_slice(text.as_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(b"tEXt");
        out.extend_from_slice(&data);
        out.extend_from_slice(&[0, 0, 0, 0]); // crc
        // IEND
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(b"IEND");
        out.extend_from_slice(&[0, 0, 0, 0]);
        out
    }

    fn b64(json: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(json.as_bytes())
    }

    #[test]
    fn reads_chara_v2() {
        let png = png_with_text("chara", &b64(r#"{"spec":"chara_card_v2","data":{"name":"Neo"}}"#));
        let v = read_card_json(&png).unwrap();
        assert_eq!(v["data"]["name"], "Neo");
    }

    #[test]
    fn ccv3_takes_precedence_over_chara() {
        // 两个 tEXt：chara=Old, ccv3=New → 取 ccv3。
        let mut png = PNG_SIGNATURE.to_vec();
        for (kw, name) in [("chara", "Old"), ("ccv3", "New")] {
            let body = b64(&format!(r#"{{"data":{{"name":"{name}"}}}}"#));
            let mut data = Vec::new();
            data.extend_from_slice(kw.as_bytes());
            data.push(0);
            data.extend_from_slice(body.as_bytes());
            png.extend_from_slice(&(data.len() as u32).to_be_bytes());
            png.extend_from_slice(b"tEXt");
            png.extend_from_slice(&data);
            png.extend_from_slice(&[0, 0, 0, 0]);
        }
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&[0, 0, 0, 0]);
        let v = read_card_json(&png).unwrap();
        assert_eq!(v["data"]["name"], "New");
    }

    #[test]
    fn rejects_non_png() {
        assert!(read_card_json(b"not a png at all").is_err());
    }

    #[test]
    fn errors_when_no_text_chunk() {
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&[0, 0, 0, 0]);
        assert!(read_card_json(&png).is_err());
    }

    #[test]
    fn errors_on_bad_base64() {
        let png = png_with_text("chara", "!!!! not base64 !!!!");
        assert!(read_card_json(&png).is_err());
    }

    #[test]
    fn rejects_oversized_chunk_without_allocating() {
        // 构造一个声明超大 length 的 tEXt chunk（实际数据不提供）→ 必须在分配前报错。
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend_from_slice(&((MAX_TEXT_CHUNK as u32) + 1).to_be_bytes());
        png.extend_from_slice(b"tEXt");
        // 不追加真实数据：函数应在检查 length 时即 Err，不读到这里。
        assert!(read_card_json(&png).is_err());
    }
}
```

- [ ] **Step 3: 接 lib + 跑测试**

`shirita-core/src/lib.rs` 在模块声明区（`pub mod model;` 一带）加 `pub mod pngcard;`，并在 re-export 区加 `pub use pngcard::read_card_json;`。

Run: `cargo test -p shirita-core --lib pngcard::`
Expected: PASS（6 tests）。

- [ ] **Step 4: 提交**

```bash
git add shirita-core/Cargo.toml shirita-core/src/pngcard.rs shirita-core/src/lib.rs
git commit -m "feat(core): pngcard::read_card_json — extract embedded ST card JSON with size cap"
```

---

## Task 2: `sanitize_tag` + 组装包裹钩子

**Files:**
- Modify: `shirita-core/src/assembly.rs`
- Modify: `shirita-core/src/lib.rs`

- [ ] **Step 1: 写 `sanitize_tag` + `maybe_wrap` + 失败测试**

在 `shirita-core/src/assembly.rs` 的 `effective_def_content`（约 230 行）之后、`#[cfg(test)]` 之前，加两个函数：

```rust
/// 把定义名净化为可用作 XML 标签的字符串：trim → 连续空白折叠为单个 `_` →
/// 移除 XML 致命字符 `< > & " ' /` → 保留其余（含中文/字母/数字/`_`/`-`）。
/// 结果可能为空（名字全是被剔字符）；兜底由调用方负责。
pub fn sanitize_tag(name: &str) -> String {
    let mut out = String::new();
    let mut pending_us = false;
    for ch in name.trim().chars() {
        if ch.is_whitespace() {
            if !out.is_empty() {
                pending_us = true;
            }
            continue;
        }
        if matches!(ch, '<' | '>' | '&' | '"' | '\'' | '/') {
            continue;
        }
        if pending_us {
            out.push('_');
            pending_us = false;
        }
        out.push(ch);
    }
    out
}

/// 若定义开了 `meta.wrap_in_tag`，用其 `sanitize_tag(name)`（空则兜底 def_type）包裹内容。
fn maybe_wrap(def: &Definition, content: String) -> String {
    let on = def.meta.get("wrap_in_tag").and_then(|v| v.as_bool()).unwrap_or(false);
    if !on {
        return content;
    }
    let mut tag = sanitize_tag(&def.name);
    if tag.is_empty() {
        tag = def.def_type.clone();
    }
    format!("<{tag}>\n{content}\n</{tag}>")
}
```

在文件底部 `#[cfg(test)] mod tests` 内（沿用其既有 `use super::*;`）加测试：

```rust
    #[test]
    fn sanitize_tag_folds_spaces_and_strips_fatal() {
        assert_eq!(sanitize_tag("Alice Smith"), "Alice_Smith");
        assert_eq!(sanitize_tag("  Hello   World  "), "Hello_World");
        assert_eq!(sanitize_tag("a <b>/c"), "a_bc");
        assert_eq!(sanitize_tag("主角·凛"), "主角·凛");
    }

    #[test]
    fn sanitize_tag_empty_when_all_stripped() {
        assert_eq!(sanitize_tag("<>&\"'/"), "");
    }

    #[test]
    fn maybe_wrap_wraps_only_when_flag_on() {
        let mut d = Definition::new("char", "Alice Smith", "body");
        assert_eq!(maybe_wrap(&d, "body".into()), "body"); // 默认关
        d.meta = serde_json::json!({ "wrap_in_tag": true });
        assert_eq!(maybe_wrap(&d, "body".into()), "<Alice_Smith>\nbody\n</Alice_Smith>");
    }

    #[test]
    fn maybe_wrap_falls_back_to_def_type_when_name_empty() {
        let mut d = Definition::new("world", "<>", "body");
        d.meta = serde_json::json!({ "wrap_in_tag": true });
        assert_eq!(maybe_wrap(&d, "body".into()), "<world>\nbody\n</world>");
    }
```

> `Definition` 已在 `assembly.rs` 顶部 `use` 进来（其它函数已用）；测试模块亦可见。

- [ ] **Step 2: 跑测试看通过**

Run: `cargo test -p shirita-core --lib assembly::tests::sanitize_tag assembly::tests::maybe_wrap`
Expected: PASS（4 tests）。

- [ ] **Step 3: 在 `resolve()` 接入包裹**

把 `assemble_from_nodes` 里的 `resolve` 闭包最后一行（约 282 行）：

```rust
        Some(render_vars(&effective_def_content(def, overrides), state))
```

改为：

```rust
        let body = render_vars(&effective_def_content(def, overrides), state);
        Some(maybe_wrap(def, body))
```

- [ ] **Step 4: 写组装级集成测试**

在 `#[cfg(test)] mod tests` 内加一个端到端测试，验证开关开启时根级 ref 输出被包裹。沿用既有测试里构建 `PromptNode`/`Definition`/`HashMap` 的写法（参考同文件 `folder_node`/`def` 辅助）：

```rust
    #[test]
    fn ref_node_wraps_content_when_definition_flag_on() {
        let mut d = def("char", "Hero", "I am hero");
        d.meta = serde_json::json!({ "wrap_in_tag": true });
        let r = PromptNode::new_ref(OwnerKind::Template, "t", None, 0, &d.id);
        let mut defs = std::collections::HashMap::new();
        defs.insert(d.id.clone(), d);
        let plan = assemble_from_nodes(
            &[r],
            &defs,
            &serde_json::json!({}),
            &serde_json::json!({}),
            &[],
            &mut || 0.0,
        );
        assert_eq!(plan.segments.len(), 1);
        assert_eq!(plan.segments[0].content, "<Hero>\nI am hero\n</Hero>");
    }
```

> 若 `def(...)` 辅助签名为 `def(type, name, content)`（见同文件既有测试），按实际调整。`assemble_from_nodes` 末参为 `roll: &mut impl FnMut() -> f64`，传 `&mut || 0.0` 使所有条目激活（trigger 默认 always/概率比较）。

- [ ] **Step 5: 跑测试 + re-export**

`shirita-core/src/lib.rs` 的 assembly re-export 行加 `sanitize_tag`：找到 `pub use assembly::{ ... };`，把 `sanitize_tag` 加进列表。

Run: `cargo test -p shirita-core --lib assembly::`
Expected: PASS（含新 5 个 + 既有全绿）。

- [ ] **Step 6: 提交**

```bash
git add shirita-core/src/assembly.rs shirita-core/src/lib.rs
git commit -m "feat(core): definition-name XML wrap (sanitize_tag + assemble resolve hook)"
```

---

## Task 3: `portable` 原创格式编解码

**Files:**
- Create: `shirita-core/src/portable.rs`
- Modify: `shirita-core/src/lib.rs`

- [ ] **Step 1: 写 `portable.rs`（类型 + 导出函数 + 失败测试）**

创建 `shirita-core/src/portable.rs`：

```rust
//! Shirita 原创导入导出格式编解码（纯数据变换，不触库）。
//! 节点/定义间引用用 `local_id`（与真实 UUID 解耦），导入侧再重映射为新 UUID。

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::models::definition::Definition;
use crate::models::prompt_node::{NodeKind, PromptNode};
use crate::models::template::Template;
use crate::{Error, Result};

/// 单定义 → 原创信封。
pub fn export_definition(def: &Definition) -> Value {
    json!({
        "format": "shirita.definition",
        "version": 1,
        "definition": {
            "type": def.def_type,
            "name": def.name,
            "content": def.content,
            "meta": def.meta,
        }
    })
}

/// 仅保留自身及全部祖先都 enabled 的节点（排除 disabled 子树）。
fn filter_enabled(nodes: &[PromptNode]) -> Vec<&PromptNode> {
    let by_id: HashMap<&str, &PromptNode> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    nodes
        .iter()
        .filter(|n| {
            let mut cur: &PromptNode = n;
            loop {
                if !cur.enabled {
                    return false;
                }
                match cur.parent_id.as_deref().and_then(|p| by_id.get(p)) {
                    Some(p) => cur = p,
                    None => return true,
                }
            }
        })
        .collect()
}

/// 模板「启用部分」→ 原创信封：排除 disabled 子树；defs 只含被保留 ref 实际引用者（去重）；
/// 悬空 definition_id 的 ref 节点跳过 + warn，保证产出引用完整。
pub fn export_template(
    template: &Template,
    nodes: &[PromptNode],
    defs: &HashMap<String, Definition>,
) -> Value {
    let kept = filter_enabled(nodes);
    let node_lid: HashMap<&str, String> =
        kept.iter().enumerate().map(|(i, n)| (n.id.as_str(), format!("n{i}"))).collect();

    let mut def_lid: HashMap<String, String> = HashMap::new();
    let mut out_defs: Vec<Value> = Vec::new();
    let mut out_nodes: Vec<Value> = Vec::new();

    for n in &kept {
        let mut def_local: Option<String> = None;
        if n.kind == NodeKind::Ref {
            match n.definition_id.as_ref().and_then(|id| defs.get(id)) {
                Some(d) => {
                    let lid = def_lid
                        .entry(d.id.clone())
                        .or_insert_with(|| {
                            let l = format!("d{}", out_defs.len());
                            out_defs.push(json!({
                                "local_id": l,
                                "type": d.def_type,
                                "name": d.name,
                                "content": d.content,
                                "meta": d.meta,
                            }));
                            l
                        })
                        .clone();
                    def_local = Some(lid);
                }
                None => {
                    tracing::warn!(node_id = %n.id, "export_template: ref has dangling definition_id, skipping");
                    continue;
                }
            }
        }
        out_nodes.push(json!({
            "local_id": node_lid[n.id.as_str()],
            "parent_local_id": n.parent_id.as_deref().and_then(|p| node_lid.get(p)).cloned(),
            "kind": n.kind.as_str(),
            "tag": n.tag,
            "def_local_id": def_local,
            "enabled": n.enabled,
            "sort_order": n.sort_order,
        }));
    }

    json!({
        "format": "shirita.template",
        "version": 1,
        "template": { "name": template.name, "meta": template.meta },
        "nodes": out_nodes,
        "definitions": out_defs,
    })
}

/// 解析后的可移植节点（local_id 形态，未落库）。
#[derive(Debug, Clone, PartialEq)]
pub struct PortableNode {
    pub local_id: String,
    pub parent_local_id: Option<String>,
    pub kind: NodeKind,
    pub tag: Option<String>,
    pub def_local_id: Option<String>,
    pub enabled: bool,
    pub sort_order: i64,
}

/// 解析后的可移植定义（带 local_id）。
#[derive(Debug, Clone, PartialEq)]
pub struct PortableDef {
    pub local_id: String,
    pub def_type: String,
    pub name: String,
    pub content: String,
    pub meta: Value,
}

/// 解析结果：单定义或模板 bundle。
#[derive(Debug, Clone, PartialEq)]
pub enum PortableDoc {
    Definition(Definition),
    Template { name: String, meta: Value, nodes: Vec<PortableNode>, defs: Vec<PortableDef> },
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

/// 解析原创信封。`format` 不识别 → Err。
pub fn parse_portable(v: &Value) -> Result<PortableDoc> {
    match v.get("format").and_then(|f| f.as_str()) {
        Some("shirita.definition") => {
            let d = v.get("definition").ok_or_else(|| Error::Config("missing definition".into()))?;
            let mut def = Definition::new(s(d, "type"), s(d, "name"), s(d, "content"));
            def.meta = d.get("meta").cloned().unwrap_or_else(|| json!({}));
            Ok(PortableDoc::Definition(def))
        }
        Some("shirita.template") => {
            let t = v.get("template").ok_or_else(|| Error::Config("missing template".into()))?;
            let name = s(t, "name");
            let meta = t.get("meta").cloned().unwrap_or_else(|| json!({}));
            let defs = v.get("definitions").and_then(|x| x.as_array()).cloned().unwrap_or_default();
            let defs: Vec<PortableDef> = defs
                .iter()
                .map(|d| PortableDef {
                    local_id: s(d, "local_id"),
                    def_type: s(d, "type"),
                    name: s(d, "name"),
                    content: s(d, "content"),
                    meta: d.get("meta").cloned().unwrap_or_else(|| json!({})),
                })
                .collect();
            let nodes = v.get("nodes").and_then(|x| x.as_array()).cloned().unwrap_or_default();
            let nodes: Result<Vec<PortableNode>> = nodes
                .iter()
                .map(|n| {
                    Ok(PortableNode {
                        local_id: s(n, "local_id"),
                        parent_local_id: n.get("parent_local_id").and_then(|x| x.as_str()).map(|x| x.to_string()),
                        kind: NodeKind::from_db(&s(n, "kind"))?,
                        tag: n.get("tag").and_then(|x| x.as_str()).map(|x| x.to_string()),
                        def_local_id: n.get("def_local_id").and_then(|x| x.as_str()).map(|x| x.to_string()),
                        enabled: n.get("enabled").and_then(|x| x.as_bool()).unwrap_or(true),
                        sort_order: n.get("sort_order").and_then(|x| x.as_i64()).unwrap_or(0),
                    })
                })
                .collect();
            Ok(PortableDoc::Template { name, meta, nodes: nodes?, defs })
        }
        _ => Err(Error::Config("unrecognized shirita format".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::prompt_node::OwnerKind;

    #[test]
    fn definition_round_trip() {
        let mut d = Definition::new("char", "Neo", "The One");
        d.meta = json!({ "wrap_in_tag": true });
        let v = export_definition(&d);
        assert_eq!(v["format"], "shirita.definition");
        match parse_portable(&v).unwrap() {
            PortableDoc::Definition(got) => {
                assert_eq!(got.def_type, "char");
                assert_eq!(got.name, "Neo");
                assert_eq!(got.content, "The One");
                assert_eq!(got.meta["wrap_in_tag"], true);
            }
            _ => panic!("expected definition"),
        }
    }

    #[test]
    fn template_export_filters_disabled_subtree() {
        // root folder(enabled) > ref A(enabled); disabled folder > ref B
        let fa = PromptNode::new_folder(OwnerKind::Template, "t", None, 0, "char");
        let a = Definition::new("char", "A", "aa");
        let ra = PromptNode::new_ref(OwnerKind::Template, "t", Some(fa.id.clone()), 0, &a.id);
        let mut fb = PromptNode::new_folder(OwnerKind::Template, "t", None, 1, "world");
        fb.enabled = false;
        let b = Definition::new("world", "B", "bb");
        let rb = PromptNode::new_ref(OwnerKind::Template, "t", Some(fb.id.clone()), 0, &b.id);

        let mut defs = HashMap::new();
        defs.insert(a.id.clone(), a.clone());
        defs.insert(b.id.clone(), b.clone());
        let tmpl = Template::new("T");
        let v = export_template(&tmpl, &[fa, ra, fb, rb], &defs);

        // 只剩 fa + ra（2 节点），defs 只含 A。
        assert_eq!(v["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(v["definitions"].as_array().unwrap().len(), 1);
        assert_eq!(v["definitions"][0]["name"], "A");
    }

    #[test]
    fn template_export_skips_dangling_ref() {
        // ref 指向 defs 里不存在的 id → 跳过该 ref，bundle 无 dangling。
        let r = PromptNode::new_ref(OwnerKind::Template, "t", None, 0, "missing-def-id");
        let defs: HashMap<String, Definition> = HashMap::new();
        let v = export_template(&Template::new("T"), &[r], &defs);
        assert_eq!(v["nodes"].as_array().unwrap().len(), 0);
        assert_eq!(v["definitions"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn template_parse_reads_nodes_and_local_refs() {
        let v = json!({
            "format": "shirita.template", "version": 1,
            "template": { "name": "T", "meta": {} },
            "nodes": [
                { "local_id": "n0", "parent_local_id": null, "kind": "folder", "tag": "char",
                  "def_local_id": null, "enabled": true, "sort_order": 0 },
                { "local_id": "n1", "parent_local_id": "n0", "kind": "ref", "tag": null,
                  "def_local_id": "d0", "enabled": true, "sort_order": 0 }
            ],
            "definitions": [ { "local_id": "d0", "type": "char", "name": "A", "content": "aa", "meta": {} } ]
        });
        match parse_portable(&v).unwrap() {
            PortableDoc::Template { name, nodes, defs, .. } => {
                assert_eq!(name, "T");
                assert_eq!(nodes.len(), 2);
                assert_eq!(nodes[1].parent_local_id.as_deref(), Some("n0"));
                assert_eq!(nodes[1].def_local_id.as_deref(), Some("d0"));
                assert_eq!(defs.len(), 1);
                assert_eq!(defs[0].local_id, "d0");
            }
            _ => panic!("expected template"),
        }
    }

    #[test]
    fn unknown_format_errors() {
        assert!(parse_portable(&json!({ "format": "whatever" })).is_err());
    }
}
```

- [ ] **Step 2: 接 lib + 跑测试**

`shirita-core/src/lib.rs` 加 `pub mod portable;`，并 re-export：`pub use portable::{export_definition, export_template, parse_portable, PortableDoc, PortableNode, PortableDef};`。

Run: `cargo test -p shirita-core --lib portable::`
Expected: PASS（5 tests）。

- [ ] **Step 3: 全量回归 + 提交**

Run: `cargo test -p shirita-core && cargo build -p shirita-core`
Expected: 全绿、零警告。

```bash
git add shirita-core/src/portable.rs shirita-core/src/lib.rs
git commit -m "feat(core): portable codec — shirita.definition/template export + parse"
```

---

## Self-Review Checklist

- **Spec 覆盖**：§3 pngcard 只读 + MAX_TEXT_CHUNK（Task1）✓；§5 `meta.wrap_in_tag` + `sanitize_tag`（兜底 def_type）+ 组装钩子（Task2）✓；§4 portable 两信封 + local_id + enabled 过滤 + 悬空 ref 跳过+warn（Task3）✓。
- **Placeholder 扫描**：无 TBD；每步含完整代码与命令。
- **类型一致**：`read_card_json(&[u8])->Result<Value>`、`sanitize_tag(&str)->String`、`export_definition(&Definition)->Value`、`export_template(&Template,&[PromptNode],&HashMap<String,Definition>)->Value`、`parse_portable(&Value)->Result<PortableDoc>`、`PortableDoc::{Definition,Template{name,meta,nodes,defs}}`、`PortableNode`/`PortableDef` 字段在 Plan 3 还原时复用。
- **依赖前置**：仅依赖既有 `Definition`/`PromptNode`/`Template`/`assemble_from_nodes`；新增 `base64`。Plan 2/3 依赖本 plan 的 `read_card_json`/`portable`。
```
