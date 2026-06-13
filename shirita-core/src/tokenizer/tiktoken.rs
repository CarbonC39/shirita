//! 基于 tiktoken（cl100k_base）的轻量计数器，作为所有模型的近似计数。

use tiktoken_rs::CoreBPE;

use super::TokenCounter;

pub struct TiktokenCounter {
    bpe: CoreBPE,
}

impl TiktokenCounter {
    pub fn new() -> Self {
        // cl100k_base 内置词表，无需联网。
        let bpe = tiktoken_rs::cl100k_base().expect("cl100k_base must load");
        Self { bpe }
    }
}

impl Default for TiktokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenCounter for TiktokenCounter {
    fn count(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }
        self.bpe.encode_with_special_tokens(text).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_positive_and_monotonic() {
        let counter = TiktokenCounter::new();
        assert!(counter.count("hello world") > 0);
        assert!(counter.count("a longer piece of text goes here") > counter.count("hi"));
        assert_eq!(counter.count(""), 0);
    }
}
