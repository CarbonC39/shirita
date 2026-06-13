//! Token 计数抽象。M1 仅用于日志/预算展示，不做裁剪。

pub mod tiktoken;

pub trait TokenCounter: Send + Sync {
    fn count(&self, text: &str) -> usize;
}
