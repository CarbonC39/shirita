//! Pure helpers over the message tree: the active branch path and branch descent.

use crate::models::message::Message;
use std::collections::HashMap;

/// The linear path root→`active_leaf_id`, following `parent_id` upward, root first.
/// If `active_leaf_id` is `None` or unknown, falls back to the newest message as
/// the leaf (keeps pre-M4 / freshly-forked sessions working).
pub fn active_path<'a>(messages: &'a [Message], active_leaf_id: Option<&str>) -> Vec<&'a Message> {
    let by_id: HashMap<&str, &Message> = messages.iter().map(|m| (m.id.as_str(), m)).collect();
    let leaf = active_leaf_id
        .and_then(|id| by_id.get(id).copied())
        .or_else(|| messages.iter().max_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id))));
    let mut path = Vec::new();
    let mut cur = leaf;
    while let Some(m) = cur {
        path.push(m);
        cur = m.parent_id.as_deref().and_then(|p| by_id.get(p).copied());
    }
    path.reverse();
    path
}

/// From `from_id`, descend by picking the newest child at each level until a
/// leaf; returns that leaf id (= `from_id` if it has no children).
pub fn deepest_leaf(messages: &[Message], from_id: &str) -> String {
    let mut cur = from_id.to_string();
    loop {
        let next = messages
            .iter()
            .filter(|m| m.parent_id.as_deref() == Some(cur.as_str()))
            .max_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        match next {
            Some(child) => cur = child.id.clone(),
            None => return cur,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::message::Role;

    fn msg(id: &str, parent: Option<&str>, created: &str) -> Message {
        let mut m = Message::new("s", parent.map(|p| p.to_string()), Role::User, "x");
        m.id = id.to_string();
        m.created_at = created.to_string();
        m
    }

    #[test]
    fn active_path_walks_root_to_leaf() {
        // a -> b -> c   and a -> b -> c2 (sibling of c)
        let ms = vec![
            msg("a", None, "1"),
            msg("b", Some("a"), "2"),
            msg("c", Some("b"), "3"),
            msg("c2", Some("b"), "4"),
        ];
        let path: Vec<&str> = active_path(&ms, Some("c2")).iter().map(|m| m.id.as_str()).collect();
        assert_eq!(path, vec!["a", "b", "c2"]);
    }

    #[test]
    fn active_path_falls_back_to_newest_when_leaf_missing() {
        let ms = vec![msg("a", None, "1"), msg("b", Some("a"), "2")];
        let path: Vec<&str> = active_path(&ms, None).iter().map(|m| m.id.as_str()).collect();
        assert_eq!(path, vec!["a", "b"]);
    }

    #[test]
    fn deepest_leaf_follows_newest_child() {
        let ms = vec![
            msg("a", None, "1"),
            msg("b", Some("a"), "2"),
            msg("c_old", Some("b"), "3"),
            msg("c_new", Some("b"), "4"),
        ];
        assert_eq!(deepest_leaf(&ms, "b"), "c_new");
        assert_eq!(deepest_leaf(&ms, "c_new"), "c_new");
    }
}
