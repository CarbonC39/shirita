//! Multi-mode keyword matching (Aho-Corasick): Retrieves all matching IDs in a single pass.
use std::collections::HashSet;
use aho_corasick::AhoCorasick;

/// Constructs an automaton from a set of (id, keys) pairs, allowing a single scan of the text to yield the set of matching IDs.
pub struct KeywordIndex {
    ac: Option<AhoCorasick>,
    /// Pattern index → owner ID.
    owners: Vec<String>,
}

impl KeywordIndex {
    /// `entries`: (id, a list of keywords associated with that id). Empty keywords are skipped.
    pub fn build(entries: &[(String, Vec<String>)]) -> Self {
        let mut patterns: Vec<String> = Vec::new();
        let mut owners: Vec<String> = Vec::new();
        for (id, keys) in entries {
            for k in keys {
                let k = k.trim().to_lowercase();
                if k.is_empty() {
                    continue;
                }
                patterns.push(k);
                owners.push(id.clone());
            }
        }
        let ac = if patterns.is_empty() {
            None
        } else {
            Some(AhoCorasick::new(&patterns).expect("aho-corasick build"))
        };
        Self { ac, owners }
    }

    /// Performs a single scan of `text` and returns a set of IDs that match any of the keywords.
    pub fn scan(&self, text: &str) -> HashSet<String> {
        let mut hit = HashSet::new();
        if let Some(ac) = &self.ac {
            let lower = text.to_lowercase();
            for m in ac.find_iter(&lower) {
                hit.insert(self.owners[m.pattern().as_usize()].clone());
            }
        }
        hit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, keys: &[&str]) -> (String, Vec<String>) {
        (id.to_string(), keys.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn matches_case_insensitive_substring() {
        let idx = KeywordIndex::build(&[entry("zion", &["Zion"]), entry("neo", &["Neo", "the one"])]);
        let hit = idx.scan("Tell me about ZION please");
        assert!(hit.contains("zion"));
        assert!(!hit.contains("neo"));
    }

    #[test]
    fn multiple_keys_any_match() {
        let idx = KeywordIndex::build(&[entry("neo", &["neo", "the one"])]);
        assert!(idx.scan("he is the one").contains("neo"));
    }

    #[test]
    fn empty_index_matches_nothing() {
        let idx = KeywordIndex::build(&[entry("x", &[])]);
        assert!(idx.scan("anything").is_empty());
    }

    #[test]
    fn cjk_keys_match() {
        let idx = KeywordIndex::build(&[entry("zion", &["锡安"])]);
        assert!(idx.scan("说说锡安城").contains("zion"));
    }
}
