//! Token count abstraction. Used only for logging and budget display; it is not subject to capping.

pub mod tiktoken;

pub trait TokenCounter: Send + Sync {
    fn count(&self, text: &str) -> usize;
}
