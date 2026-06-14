//! SillyTavern World Info / lorebook ↔ 我们的 world 类型定义（带 meta.trigger）。

use crate::assembly::{parse_trigger, TriggerMode};
use crate::models::definition::Definition;

/// 把 ST 世界书 JSON（map 形 或 array 形 entries）转成 world 定义列表。
pub fn worldinfo_to_defs(wi: &serde_json::Value) -> Vec<Definition> {
    let entries = match wi.get("entries") {
        Some(serde_json::Value::Object(map)) => map.values().cloned().collect::<Vec<_>>(),
        Some(serde_json::Value::Array(arr)) => arr.clone(),
        _ => Vec::new(),
    };
    entries.iter().map(entry_to_def).collect()
}

fn str_array(v: Option<&serde_json::Value>) -> Vec<String> {
    v.and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

fn entry_to_def(e: &serde_json::Value) -> Definition {
    // keys：ST 标准用 "key"，character_book 用 "keys"。
    let keys = if e.get("key").is_some() { str_array(e.get("key")) } else { str_array(e.get("keys")) };
    let constant = e.get("constant").and_then(|v| v.as_bool()).unwrap_or(false);
    let use_prob = e.get("useProbability").and_then(|v| v.as_bool()).unwrap_or(false);
    let probability = e.get("probability").and_then(|v| v.as_u64()).unwrap_or(100).min(100);
    let mode = if constant { "constant" } else if !keys.is_empty() { "keyword" } else if use_prob { "random" } else { "constant" };
    let name = e.get("comment").and_then(|v| v.as_str()).unwrap_or("Imported entry").to_string();
    let content = e.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let order = e.get("order").or_else(|| e.get("insertion_order")).and_then(|v| v.as_u64()).unwrap_or(100);

    let mut def = Definition::new("world", name, content);
    def.meta = serde_json::json!({
        "trigger": { "mode": mode, "keys": keys, "probability": probability, "order": order }
    });
    def
}

/// world 定义列表 → ST 标准世界书 JSON（map 形 entries，键为序号）。
pub fn defs_to_worldinfo(defs: &[Definition]) -> serde_json::Value {
    let mut entries = serde_json::Map::new();
    for (i, d) in defs.iter().enumerate() {
        let t = parse_trigger(&d.meta);
        let constant = matches!(t.mode, TriggerMode::Constant);
        let use_prob = matches!(t.mode, TriggerMode::Random);
        let order = d.meta.get("trigger").and_then(|x| x.get("order")).and_then(|v| v.as_u64()).unwrap_or(100);
        entries.insert(
            i.to_string(),
            serde_json::json!({
                "uid": i,
                "key": t.keys,
                "keysecondary": [],
                "comment": d.name,
                "content": d.content,
                "constant": constant,
                "selective": matches!(t.mode, TriggerMode::Keyword),
                "order": order,
                "position": 0,
                "disable": false,
                "probability": t.probability,
                "useProbability": use_prob,
            }),
        );
    }
    serde_json::json!({ "entries": entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_standalone_map_entries() {
        let wi = serde_json::json!({
            "entries": {
                "0": { "key": ["zion"], "comment": "Zion", "content": "Last city", "constant": false, "order": 5 }
            }
        });
        let defs = worldinfo_to_defs(&wi);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].def_type, "world");
        assert_eq!(defs[0].name, "Zion");
        assert_eq!(defs[0].meta["trigger"]["mode"], "keyword");
        assert_eq!(defs[0].meta["trigger"]["keys"][0], "zion");
        assert_eq!(defs[0].meta["trigger"]["order"], 5);
    }

    #[test]
    fn imports_character_book_array_with_constant() {
        let wi = serde_json::json!({
            "entries": [ { "keys": [], "comment": "Lore", "content": "x", "constant": true } ]
        });
        let defs = worldinfo_to_defs(&wi);
        assert_eq!(defs[0].meta["trigger"]["mode"], "constant");
    }

    #[test]
    fn exports_defs_to_standalone_map() {
        let mut d = Definition::new("world", "Zion", "Last city");
        d.meta = serde_json::json!({ "trigger": { "mode": "keyword", "keys": ["zion"], "probability": 100, "order": 7 } });
        let wi = defs_to_worldinfo(&[d]);
        let e = &wi["entries"]["0"];
        assert_eq!(e["comment"], "Zion");
        assert_eq!(e["content"], "Last city");
        assert_eq!(e["key"][0], "zion");
        assert_eq!(e["constant"], false);
        assert_eq!(e["order"], 7);
        assert_eq!(e["disable"], false);
    }

    #[test]
    fn worldinfo_roundtrips() {
        let mut d = Definition::new("world", "Trinity", "She");
        d.meta = serde_json::json!({ "trigger": { "mode": "keyword", "keys": ["trinity", "she"], "probability": 100, "order": 100 } });
        let back = worldinfo_to_defs(&defs_to_worldinfo(std::slice::from_ref(&d)));
        assert_eq!(back[0].name, "Trinity");
        assert_eq!(back[0].meta["trigger"]["keys"], serde_json::json!(["trinity", "she"]));
        assert_eq!(back[0].meta["trigger"]["mode"], "keyword");
    }
}
