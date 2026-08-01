use std::collections::HashMap;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use shirita_core::OwnerKind;

use crate::AppState;

#[derive(Serialize)]
pub struct RegexScope {
    pub id: String,
    /// "global" (orphan, applies everywhere) or "template" (template-scoped).
    pub scope: String,
    /// Names of templates whose tree references this rule (deduped).
    pub template_names: Vec<String>,
    /// fancy-regex compile error, if the pattern is invalid.
    pub pattern_error: Option<String>,
}

/// Per-`regex_rule` scope + source templates + validity, for the Settings UI.
pub async fn list_regex_scopes(
    State(state): State<AppState>,
) -> Result<Json<Vec<RegexScope>>, StatusCode> {
    let err = |_| StatusCode::INTERNAL_SERVER_ERROR;
    let defs = state.storage.list_definitions().await.map_err(err)?;

    let packs = state.storage.list_packs().await.map_err(err)?;

    // def_id -> ordered unique template/pack names referencing it. Both
    // owner kinds are checked — a rule referenced only by a Pack's node tree
    // (e.g. an imported character card's status-bar regex) is template-scoped
    // in the real sense (effective_regex_rules only applies it when that pack
    // is mounted), not global; it must not be mislabeled here.
    let mut refs: HashMap<String, Vec<String>> = HashMap::new();
    for (template_name, def_id) in state.storage.template_definition_refs().await.map_err(err)? {
        let names = refs.entry(def_id).or_default();
        if !names.contains(&template_name) {
            names.push(template_name);
        }
    }
    for p in &packs {
        let nodes = state.storage.list_nodes(&OwnerKind::Pack, &p.id).await.map_err(err)?;
        for n in nodes {
            if let Some(did) = n.definition_id {
                let names = refs.entry(did).or_default();
                if !names.contains(&p.name) {
                    names.push(p.name.clone());
                }
            }
        }
    }

    let out = defs
        .iter()
        .filter(|d| d.def_type == "regex_rule")
        .map(|d| {
            let names = refs.get(&d.id).cloned().unwrap_or_default();
            let is_global = d.meta.get("is_global").and_then(|v| v.as_bool()).unwrap_or(false);
            let scope = if is_global { "global" } else { "template" };
            let pattern = d.meta.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
            RegexScope {
                id: d.id.clone(),
                scope: scope.to_string(),
                template_names: names,
                pattern_error: shirita_core::regex_error(pattern),
            }
        })
        .collect();
    Ok(Json(out))
}
