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
    /// "global" (orphan, applies everywhere) or "template" (loreset-scoped).
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
    let templates = state.storage.list_templates().await.map_err(err)?;

    // def_id -> ordered unique template names referencing it
    let mut refs: HashMap<String, Vec<String>> = HashMap::new();
    for t in &templates {
        let nodes = state.storage.list_nodes(&OwnerKind::Template, &t.id).await.map_err(err)?;
        for n in nodes {
            if let Some(did) = n.definition_id {
                let names = refs.entry(did).or_default();
                if !names.contains(&t.name) {
                    names.push(t.name.clone());
                }
            }
        }
    }

    let out = defs
        .iter()
        .filter(|d| d.def_type == "regex_rule")
        .map(|d| {
            let names = refs.get(&d.id).cloned().unwrap_or_default();
            let scope = if names.is_empty() { "global" } else { "template" };
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
