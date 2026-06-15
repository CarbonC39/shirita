//! Per-session in-flight generation registry. A newer generation aborts the
//! prior one for the same session, so swiping / re-sending can't leave two
//! streams racing to write siblings or move the active leaf.

use std::collections::HashMap;
use std::sync::Mutex;

use futures::stream::AbortHandle;

#[derive(Default)]
pub struct Generations(Mutex<HashMap<String, AbortHandle>>);

impl Generations {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `handle` as the in-flight generation for `session_id`, aborting
    /// and replacing any previous one.
    pub fn replace(&self, session_id: &str, handle: AbortHandle) {
        if let Some(old) = self.0.lock().unwrap().insert(session_id.to_string(), handle) {
            old.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::{self, abortable, StreamExt};

    #[tokio::test]
    async fn a_new_generation_aborts_the_previous_one() {
        let gens = Generations::new();
        let (first, h1) = abortable(stream::pending::<i32>());
        gens.replace("s", h1);

        let (_second, h2) = abortable(stream::iter(vec![1, 2, 3]));
        gens.replace("s", h2); // must abort the first

        assert!(first.is_aborted());
        // a different session is unaffected
        let (third, h3) = abortable(stream::pending::<i32>());
        gens.replace("other", h3);
        assert!(!third.is_aborted());
        let _ = first.collect::<Vec<_>>().await; // ends immediately (aborted)
    }
}
