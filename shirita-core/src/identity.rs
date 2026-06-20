//! Resolve per-side chat identity (display name + avatar) from a session's
//! definitions. Pure: the web layer gathers nodes/defs/template and calls this.

use std::collections::HashMap;

use serde::Serialize;

use crate::models::definition::Definition;
use crate::models::pack::PackIdentity;
use crate::models::prompt_node::{NodeKind, PromptNode};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SideIdentity {
    pub name: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Identity {
    pub assistant: SideIdentity,
    pub user: SideIdentity,
}

/// Pick the identity definition of `def_type` among enabled ref nodes (in tree
/// order): the one whose name equals `template_name`, else the first.
fn pick<'a>(
    nodes: &[PromptNode],
    defs: &'a HashMap<String, Definition>,
    def_type: &str,
    template_name: Option<&str>,
) -> Option<&'a Definition> {
    let candidates: Vec<&Definition> = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Ref && n.enabled)
        .filter_map(|n| n.definition_id.as_ref())
        .filter_map(|id| defs.get(id))
        .filter(|d| d.def_type == def_type)
        .collect();
    if let Some(tn) = template_name {
        if let Some(m) = candidates.iter().find(|d| d.name == tn) {
            return Some(m);
        }
    }
    candidates.into_iter().next()
}

/// Resolve the assistant/user identity. `session_avatar` is the chat's avatar
/// (the assistant/character avatar source); persona avatar comes from its
/// definition's `meta.avatar`. No-pack wrapper over `resolve_identity_with_packs`.
pub fn resolve_identity(
    nodes: &[PromptNode],
    defs: &HashMap<String, Definition>,
    template_name: Option<&str>,
    session_avatar: Option<&str>,
) -> Identity {
    resolve_identity_with_packs(nodes, defs, template_name, session_avatar, None, None)
}

/// Resolve identity with optional pack-bound overrides. A mounted character
/// pack's `PackIdentity` (display_name/avatar) takes priority over the char
/// definition's name and the session avatar; a persona pack's identity takes
/// priority over the persona definition. Empty-string fields count as "unset"
/// and fall through to the next source.
pub fn resolve_identity_with_packs(
    nodes: &[PromptNode],
    defs: &HashMap<String, Definition>,
    template_name: Option<&str>,
    session_avatar: Option<&str>,
    assistant_pack: Option<&PackIdentity>,
    user_pack: Option<&PackIdentity>,
) -> Identity {
    let char_def = pick(nodes, defs, "char", template_name);
    let persona_def = pick(nodes, defs, "persona", template_name);
    Identity {
        assistant: SideIdentity {
            name: pack_name(assistant_pack).or_else(|| char_def.map(|d| d.name.clone())),
            avatar: pack_avatar(assistant_pack).or_else(|| session_avatar.map(|s| s.to_string())),
        },
        user: SideIdentity {
            name: pack_name(user_pack).or_else(|| persona_def.map(|d| d.name.clone())),
            avatar: pack_avatar(user_pack).or_else(|| {
                persona_def
                    .and_then(|d| d.meta.get("avatar"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            }),
        },
    }
}

/// A pack identity's non-empty display name (treats `Some("")` as unset).
fn pack_name(p: Option<&PackIdentity>) -> Option<String> {
    p.and_then(|p| p.display_name.clone()).filter(|s| !s.is_empty())
}

/// A pack identity's non-empty avatar (treats `Some("")` as unset).
fn pack_avatar(p: Option<&PackIdentity>) -> Option<String> {
    p.and_then(|p| p.avatar.clone()).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::prompt_node::{OwnerKind, PromptNode};

    fn refn(def_id: &str, sort: i64, enabled: bool) -> PromptNode {
        let mut n = PromptNode::new_ref(OwnerKind::Template, "t", None, sort, def_id);
        n.enabled = enabled;
        n
    }

    fn def(id: &str, ty: &str, name: &str, avatar: Option<&str>) -> Definition {
        let mut d = Definition::new(ty, name, "");
        d.id = id.to_string();
        if let Some(a) = avatar {
            d.meta = serde_json::json!({ "avatar": a });
        }
        d
    }

    fn map(defs: Vec<Definition>) -> HashMap<String, Definition> {
        defs.into_iter().map(|d| (d.id.clone(), d)).collect()
    }

    #[test]
    fn assistant_name_prefers_template_name_match() {
        let nodes = vec![refn("d1", 0, true), refn("d2", 1, true)];
        let defs = map(vec![
            def("d1", "char", "Neo·personality", None),
            def("d2", "char", "Neo", None),
        ]);
        let id = resolve_identity(&nodes, &defs, Some("Neo"), Some("a.png"));
        assert_eq!(id.assistant.name.as_deref(), Some("Neo"));
        assert_eq!(id.assistant.avatar.as_deref(), Some("a.png"));
    }

    #[test]
    fn falls_back_to_first_char_and_reads_persona_avatar() {
        let nodes = vec![refn("p", 0, true), refn("c", 1, true)];
        let defs = map(vec![
            def("p", "persona", "Me", Some("u.png")),
            def("c", "char", "Alice", None),
        ]);
        let id = resolve_identity(&nodes, &defs, Some("Mismatch"), None);
        assert_eq!(id.assistant.name.as_deref(), Some("Alice")); // first char
        assert_eq!(id.user.name.as_deref(), Some("Me"));
        assert_eq!(id.user.avatar.as_deref(), Some("u.png"));
    }

    #[test]
    fn no_definitions_yields_nulls() {
        let id = resolve_identity(&[], &HashMap::new(), None, None);
        assert_eq!(id.assistant.name, None);
        assert_eq!(id.user.name, None);
        assert_eq!(id.user.avatar, None);
    }

    #[test]
    fn disabled_ref_is_ignored() {
        let nodes = vec![refn("c", 0, false)];
        let defs = map(vec![def("c", "char", "Ghost", None)]);
        let id = resolve_identity(&nodes, &defs, None, None);
        assert_eq!(id.assistant.name, None);
    }

    fn pack_id(display: Option<&str>, avatar: Option<&str>) -> PackIdentity {
        PackIdentity {
            display_name: display.map(String::from),
            avatar: avatar.map(String::from),
        }
    }

    #[test]
    fn pack_identity_overrides_char_name_and_avatar() {
        let nodes = vec![refn("c", 0, true)];
        let defs = map(vec![def("c", "char", "Alice", None)]);
        let ap = pack_id(Some("Alice the Bound"), Some("p.png"));
        let id = resolve_identity_with_packs(&nodes, &defs, None, Some("s.png"), Some(&ap), None);
        assert_eq!(id.assistant.name.as_deref(), Some("Alice the Bound"));
        assert_eq!(id.assistant.avatar.as_deref(), Some("p.png"));
    }

    #[test]
    fn empty_pack_identity_falls_back_to_def_and_session() {
        let nodes = vec![refn("c", 0, true)];
        let defs = map(vec![def("c", "char", "Alice", None)]);
        let ap = pack_id(Some(""), Some("")); // empty string == unset
        let id = resolve_identity_with_packs(&nodes, &defs, None, Some("s.png"), Some(&ap), None);
        assert_eq!(id.assistant.name.as_deref(), Some("Alice"));
        assert_eq!(id.assistant.avatar.as_deref(), Some("s.png"));
    }

    #[test]
    fn user_pack_overrides_persona() {
        let nodes = vec![refn("p", 0, true)];
        let defs = map(vec![def("p", "persona", "Me", Some("u.png"))]);
        let up = pack_id(Some("Hero"), Some("hero.png"));
        let id = resolve_identity_with_packs(&nodes, &defs, None, None, None, Some(&up));
        assert_eq!(id.user.name.as_deref(), Some("Hero"));
        assert_eq!(id.user.avatar.as_deref(), Some("hero.png"));
    }

    #[test]
    fn resolve_identity_matches_no_pack_call() {
        let nodes = vec![refn("c", 0, true)];
        let defs = map(vec![def("c", "char", "Alice", None)]);
        let a = resolve_identity(&nodes, &defs, None, Some("s.png"));
        let b = resolve_identity_with_packs(&nodes, &defs, None, Some("s.png"), None, None);
        assert_eq!(a, b);
    }
}
