//! SillyTavern World Info / lorebook ↔ world type definition (with meta.trigger).

use crate::models::definition::Definition;

/// Converts the ST world book JSON (map- or array-style entries) into a list of world definitions.
pub fn worldinfo_to_defs(wi: &serde_json::Value) -> Vec<Definition> {
    // Borrow each entry; `entry_to_def` only reads fields, so cloning the whole
    // (potentially large) entry JSON just to iterate it is wasted work.
    let entries: Vec<&serde_json::Value> = match wi.get("entries") {
        Some(serde_json::Value::Object(map)) => map.values().collect(),
        Some(serde_json::Value::Array(arr)) => arr.iter().collect(),
        _ => Vec::new(),
    };
    entries.into_iter().map(entry_to_def).collect()
}

/// Read a keys field that may be an array of strings or a single string.
fn str_array(v: Option<&serde_json::Value>) -> Vec<String> {
    match v {
        Some(serde_json::Value::Array(a)) => a.iter().filter_map(|s| s.as_str().map(String::from)).collect(),
        Some(serde_json::Value::String(s)) if !s.is_empty() => vec![s.clone()],
        _ => Vec::new(),
    }
}

/// Coerce a non-negative integer from a JSON value that ST may store as an
/// integer, a float (e.g. `50.0`), or a numeric string (e.g. `"7"`).
fn as_u64_lenient(v: Option<&serde_json::Value>) -> Option<u64> {
    let v = v?;
    if let Some(n) = v.as_u64() {
        return Some(n);
    }
    if let Some(f) = v.as_f64() {
        return (f.is_finite() && f >= 0.0).then_some(f as u64);
    }
    let s = v.as_str()?.trim();
    s.parse::<u64>()
        .ok()
        .or_else(|| s.parse::<f64>().ok().filter(|f| f.is_finite() && *f >= 0.0).map(|f| f as u64))
}

fn entry_to_def(e: &serde_json::Value) -> Definition {
    // `key` (native) and `keys` (v2) can coexist; prefer whichever is populated
    // instead of blindly taking `key` (which may be present but empty).
    let keys = {
        let primary = str_array(e.get("key"));
        if primary.is_empty() { str_array(e.get("keys")) } else { primary }
    };
    let constant = e.get("constant").and_then(|v| v.as_bool()).unwrap_or(false);
    let use_prob = e.get("useProbability").and_then(|v| v.as_bool()).unwrap_or(false);
    let probability = as_u64_lenient(e.get("probability")).unwrap_or(100).min(100);
    let mode = if constant { "constant" } else if !keys.is_empty() { "keyword" } else if use_prob { "random" } else { "constant" };
    let name = e.get("comment").and_then(|v| v.as_str()).unwrap_or("Imported entry").to_string();
    let content = e.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
    // Same dual-field handling as keys: a present-but-invalid `order` falls
    // through to `insertion_order` rather than jumping straight to the default.
    let order = as_u64_lenient(e.get("order"))
        .or_else(|| as_u64_lenient(e.get("insertion_order")))
        .unwrap_or(100);

    let mut def = Definition::new("world", name, content);
    def.meta = serde_json::json!({
        "trigger": { "mode": mode, "keys": keys, "probability": probability, "order": order }
    });
    def
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
    fn prefers_populated_keys_when_both_key_fields_present() {
        // V2 `keys` carries the real data while native `key` is an empty array;
        // pick the non-empty one instead of blindly preferring `key`.
        let wi = serde_json::json!({
            "entries": [ { "key": [], "keys": ["zion"], "comment": "Z", "content": "x" } ]
        });
        let defs = worldinfo_to_defs(&wi);
        assert_eq!(defs[0].meta["trigger"]["keys"][0], "zion");
        assert_eq!(defs[0].meta["trigger"]["mode"], "keyword");
    }

    #[test]
    fn accepts_a_single_string_key() {
        let wi = serde_json::json!({ "entries": [ { "key": "zion", "content": "x" } ] });
        let defs = worldinfo_to_defs(&wi);
        assert_eq!(defs[0].meta["trigger"]["keys"][0], "zion");
        assert_eq!(defs[0].meta["trigger"]["mode"], "keyword");
    }

    #[test]
    fn coerces_float_and_string_numbers_for_probability_and_order() {
        let wi = serde_json::json!({
            "entries": [ { "keys": ["a"], "content": "x", "probability": 50.0, "order": "7" } ]
        });
        let defs = worldinfo_to_defs(&wi);
        assert_eq!(defs[0].meta["trigger"]["probability"], 50);
        assert_eq!(defs[0].meta["trigger"]["order"], 7);
    }

    #[test]
    fn falls_back_to_insertion_order_when_order_is_invalid() {
        let wi = serde_json::json!({
            "entries": [ { "keys": ["a"], "content": "x", "order": "nope", "insertion_order": 9 } ]
        });
        let defs = worldinfo_to_defs(&wi);
        assert_eq!(defs[0].meta["trigger"]["order"], 9);
    }
}
