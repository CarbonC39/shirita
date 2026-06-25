//! Per-session in-flight generation registry. A newer generation aborts the
//! prior one for the same session, so swiping / re-sending can't leave two
//! streams racing to write siblings or move the active leaf.
//!
//! Each slot carries a monotonic generation id so a finishing stream only
//! de-registers itself when it's still the current one — otherwise a just-ended
//! generation could evict the newer generation that replaced it, and slots would
//! otherwise accumulate forever (one per session that ever generated).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use futures::stream::AbortHandle;

#[derive(Default)]
pub struct Generations(Mutex<HashMap<String, (u64, AbortHandle)>>);

static NEXT_GEN: AtomicU64 = AtomicU64::new(0);

impl Generations {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `handle` as the in-flight generation for `session_id`, aborting
    /// and replacing any previous one. Returns this generation's id, to pass to
    /// [`finish`](Self::finish) when the stream ends.
    pub fn replace(&self, session_id: &str, handle: AbortHandle) -> u64 {
        let gen_id = NEXT_GEN.fetch_add(1, Ordering::Relaxed);
        if let Some((_, old)) = self.0.lock().unwrap().insert(session_id.to_string(), (gen_id, handle)) {
            old.abort();
        }
        gen_id
    }

    /// De-register the slot for `session_id` once its generation ends — but only
    /// if it's still ours; a newer generation may already have replaced it.
    pub fn finish(&self, session_id: &str, gen_id: u64) {
        let mut g = self.0.lock().unwrap();
        if g.get(session_id).map(|(id, _)| *id == gen_id).unwrap_or(false) {
            g.remove(session_id);
        }
    }

    /// Abort and forget any in-flight generation for `session_id` (e.g. the
    /// session was deleted, so finishing its generation is pointless).
    pub fn remove(&self, session_id: &str) {
        if let Some((_, handle)) = self.0.lock().unwrap().remove(session_id) {
            handle.abort();
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

    #[tokio::test]
    async fn finish_clears_the_current_generation_without_aborting() {
        let gens = Generations::new();
        let (first, h1) = abortable(stream::pending::<i32>());
        let g1 = gens.replace("s", h1);
        gens.finish("s", g1); // ours → de-register (does not abort)
        assert!(!first.is_aborted());
        // The slot is gone, so replacing finds nothing to abort.
        let (_b, h2) = abortable(stream::pending::<i32>());
        gens.replace("s", h2);
        assert!(!first.is_aborted(), "finish only de-registers; it must not abort");
    }

    #[tokio::test]
    async fn finish_ignores_a_superseded_generation() {
        let gens = Generations::new();
        let (_a, h1) = abortable(stream::pending::<i32>());
        let g1 = gens.replace("s", h1);
        let (second, h2) = abortable(stream::pending::<i32>());
        gens.replace("s", h2); // aborts g1; slot now holds the newer generation
        gens.finish("s", g1); // stale id → must NOT clear the newer slot
        let (_c, h3) = abortable(stream::pending::<i32>());
        gens.replace("s", h3);
        assert!(second.is_aborted(), "the live generation must stay tracked after a stale finish");
    }

    #[tokio::test]
    async fn remove_aborts_and_forgets_the_session() {
        let gens = Generations::new();
        let (first, h1) = abortable(stream::pending::<i32>());
        gens.replace("s", h1);
        gens.remove("s");
        assert!(first.is_aborted(), "removing a deleted session aborts its in-flight generation");
    }
}
