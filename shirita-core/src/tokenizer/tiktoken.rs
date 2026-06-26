//! Lightweight counter based on tiktoken (o200k_base — the GPT-4o / modern
//! encoding), used as an approximate token count for budget/trim decisions
//! across all providers. It is an estimate, not an exact per-model count:
//! o200k matches GPT-4o-class models, and is a closer proxy for current models
//! than the older cl100k_base, but no local tokenizer reproduces a given
//! provider's exact counts (Claude in particular tokenizes differently). The
//! 0.8 summarize threshold and provider-side overflow handling absorb the gap.

use tiktoken_rs::CoreBPE;

use super::TokenCounter;

pub struct TiktokenCounter {
    bpe: CoreBPE,
}

impl TiktokenCounter {
    pub fn new() -> Self {
        let bpe = tiktoken_rs::o200k_base().expect("o200k_base must load");
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
        self.bpe.encode_ordinary(text).len()
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
