//! 关键词多模式匹配（Aho-Corasick）：一次扫描得全部命中 id。

use std::collections::HashSet;

use aho_corasick::AhoCorasick;

/// 把若干 (id, keys) 编成一个自动机，对文本一次扫描即得命中 id 集合。
pub struct KeywordIndex {
    ac: Option<AhoCorasick>,
    /// pattern 序号 → owner id。
    owners: Vec<String>,
}

impl KeywordIndex {
    /// `entries`: (id, 该 id 的关键词列表)。空关键词会被跳过。
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

    /// 对 `text` 一次扫描，返回命中（任一关键词）的 id 集合。
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
