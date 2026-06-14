//! SillyTavern Character Card V2/V3 ↔ char 定义（+ 内嵌 character_book → world 定义）。

use crate::adapters::worldinfo::worldinfo_to_defs;
use crate::models::definition::Definition;

/// 解析 chara_card_v2/v3：返回 (char 定义, 内嵌世界书定义列表)。
pub fn charcard_to_defs(card: &serde_json::Value) -> (Definition, Vec<Definition>) {
    let data = card.get("data").unwrap_or(card);
    let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("Imported character").to_string();
    let description = data.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let mut def = Definition::new("char", name, description);
    // 保留 ST 扩展字段以便回出口（不丢信息）。
    def.meta = serde_json::json!({
        "st": {
            "personality": data.get("personality"),
            "scenario": data.get("scenario"),
            "first_mes": data.get("first_mes"),
            "mes_example": data.get("mes_example"),
            "system_prompt": data.get("system_prompt"),
            "post_history_instructions": data.get("post_history_instructions"),
        }
    });

    let book_defs = data
        .get("character_book")
        .map(worldinfo_to_defs)
        .unwrap_or_default();
    (def, book_defs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_v2_card_with_book() {
        let card = serde_json::json!({
            "spec": "chara_card_v2", "spec_version": "2.0",
            "data": {
                "name": "Neo", "description": "The One",
                "character_book": { "entries": [ { "keys": ["zion"], "comment": "Zion", "content": "Last city" } ] }
            }
        });
        let (ch, book) = charcard_to_defs(&card);
        assert_eq!(ch.def_type, "char");
        assert_eq!(ch.name, "Neo");
        assert_eq!(ch.content, "The One");
        assert_eq!(book.len(), 1);
        assert_eq!(book[0].def_type, "world");
        assert_eq!(book[0].meta["trigger"]["keys"][0], "zion");
    }
}
