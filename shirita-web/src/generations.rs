//! Per-session in-flight generation registry. A newer generation aborts the
//! prior one for the same session, so swiping / re-sending can't leave two
//! streams racing to write siblings or move the active leaf.
//!
//! Each slot carries a monotonic generation id so a finishing stream only
//! de-registers itself when it's still the current one — otherwise a just-ended
//! generation could evict the newer generation that replaced it, and slots would
//! otherwise accumulate forever (one per session that ever generated).
//!
//! In addition to the hard `AbortHandle` (newer-replaces-prior), each slot
//! carries a cooperative [`StopHandle`] fired by the Stop button / abort route:
//! it lets the stream persist whatever was generated so far instead of
//! discarding everything a hard abort would lose.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use futures::stream::AbortHandle;
use shirita_core::StopHandle;

#[derive(Default)]
pub struct Generations(Mutex<HashMap<String, (u64, AbortHandle, StopHandle)>>);

static NEXT_GEN: AtomicU64 = AtomicU64::new(0);

impl Generations {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an in-flight generation for `session_id`, aborting and replacing
    /// any previous one. `handle` is the hard abort (newer-replaces-prior);
    /// `stop` is the cooperative stop token already threaded into the stream, so
    /// the Stop button / abort route can end it gracefully (persisting partial
    /// text). Returns this generation's id, to pass to
    /// [`finish`](Self::finish) when the stream ends.
    pub fn replace(&self, session_id: &str, handle: AbortHandle, stop: StopHandle) -> u64 {
        let gen_id = NEXT_GEN.fetch_add(1, Ordering::Relaxed);
        if let Some((_, old_handle, old_stop)) =
            self.0.lock().unwrap().insert(session_id.to_string(), (gen_id, handle, stop))
        {
            // Hard-abort the superseded stream and signal its stop token so its
            // cooperative select! (if still polling) also wakes.
            old_handle.abort();
            old_stop.stop();
        }
        gen_id
    }

    /// De-register the slot for `session_id` once its generation ends — but only
    /// if it's still ours; a newer generation may already have replaced it.
    pub fn finish(&self, session_id: &str, gen_id: u64) {
        let mut g = self.0.lock().unwrap();
        if g.get(session_id).map(|(id, _, _)| *id == gen_id).unwrap_or(false) {
            g.remove(session_id);
        }
    }

    /// Abort and forget any in-flight generation for `session_id` (e.g. the
    /// session was deleted, so finishing its generation is pointless).
    pub fn remove(&self, session_id: &str) {
        if let Some((_, handle, stop)) = self.0.lock().unwrap().remove(session_id) {
            handle.abort();
            stop.stop();
        }
    }

    /// Cooperatively stop the in-flight generation for `session_id` (Stop button
    /// / navigate-away): the stream persists the partial text so far, then ends.
    /// Returns false if there is nothing in flight. Does NOT de-register — the
    /// stream itself calls `finish` as it terminates.
    pub fn stop(&self, session_id: &str) -> bool {
        if let Some((_, _, stop)) = self.0.lock().unwrap().get(session_id) {
            stop.stop();
            true
        } else {
            false
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
        gens.replace("s", h1, StopHandle::default());

        let (_second, h2) = abortable(stream::iter(vec![1, 2, 3]));
        gens.replace("s", h2, StopHandle::default()); // must abort the first

        assert!(first.is_aborted());
        // a different session is unaffected
        let (third, h3) = abortable(stream::pending::<i32>());
        gens.replace("other", h3, StopHandle::default());
        assert!(!third.is_aborted());
        let _ = first.collect::<Vec<_>>().await; // ends immediately (aborted)
    }

    #[tokio::test]
    async fn finish_clears_the_current_generation_without_aborting() {
        let gens = Generations::new();
        let (first, h1) = abortable(stream::pending::<i32>());
        let g1 = gens.replace("s", h1, StopHandle::default());
        gens.finish("s", g1); // ours → de-register (does not abort)
        assert!(!first.is_aborted());
        // The slot is gone, so replacing finds nothing to abort.
        let (_b, h2) = abortable(stream::pending::<i32>());
        gens.replace("s", h2, StopHandle::default());
        assert!(!first.is_aborted(), "finish only de-registers; it must not abort");
    }

    #[tokio::test]
    async fn finish_ignores_a_superseded_generation() {
        let gens = Generations::new();
        let (_a, h1) = abortable(stream::pending::<i32>());
        let g1 = gens.replace("s", h1, StopHandle::default());
        let (second, h2) = abortable(stream::pending::<i32>());
        gens.replace("s", h2, StopHandle::default()); // aborts g1; slot now holds the newer generation
        gens.finish("s", g1); // stale id → must NOT clear the newer slot
        let (_c, h3) = abortable(stream::pending::<i32>());
        gens.replace("s", h3, StopHandle::default());
        assert!(second.is_aborted(), "the live generation must stay tracked after a stale finish");
    }

    #[tokio::test]
    async fn remove_aborts_and_forgets_the_session() {
        let gens = Generations::new();
        let (first, h1) = abortable(stream::pending::<i32>());
        gens.replace("s", h1, StopHandle::default());
        gens.remove("s");
        assert!(first.is_aborted(), "removing a deleted session aborts its in-flight generation");
    }

    #[tokio::test]
    async fn stop_signals_the_live_generation() {
        let gens = Generations::new();
        let (_first, h1) = abortable(stream::pending::<i32>());
        let _ = gens.replace("s", h1, StopHandle::default());
        assert!(gens.stop("s"), "stopping a live generation returns true");
        assert!(!gens.stop("none"), "stopping a session with nothing in flight returns false");
    }
}
