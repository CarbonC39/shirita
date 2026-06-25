//! Context budget and best-effort historical pruning: pure functions, no I/O.

use crate::model::ChatMessage;
use crate::models::message::Role;
use crate::tokenizer::TokenCounter;

/// Checks whether usage has exceeded the threshold (window * threshold).
pub fn over_threshold(prompt_tokens: usize, window: usize, threshold: f64) -> bool {
    (prompt_tokens as f64) > (window as f64) * threshold
}

/// Best-effort pruning: Retain the leading `system` message (if any) and the last message (the current `user` round), starting from the middle of the oldest history.
/// Discard messages one by one until the total count is ≤ `window` or only the protected first and last messages remain. Return (the retained messages, the number of messages discarded).
pub fn trim_history(
    messages: &[ChatMessage],
    window: usize,
    counter: &dyn TokenCounter,
) -> (Vec<ChatMessage>, usize) {
    let tok = |m: &ChatMessage| counter.count(&m.content);
    let mut running: usize = messages.iter().map(tok).sum();
    if running <= window || messages.len() <= 2 {
        return (messages.to_vec(), 0);
    }
    // Include the first entry in the protected lead-in only if it is definitely “system”; otherwise, the middle section starts at 0 (the oldest history may be discarded).
    let mid_start = if messages[0].role == Role::System { 1 } else { 0 };
    let mut keep = vec![true; messages.len()];
    let last = messages.len() - 1;
    let mut dropped = 0usize;
    // Middle segment = index range mid_start..last; the oldest elements are removed first.
    for i in mid_start..last {
        if running <= window {
            break;
        }
        keep[i] = false;
        running -= tok(&messages[i]);
        dropped += 1;
    }
    let out = messages
        .iter()
        .zip(keep)
        .filter_map(|(m, k)| if k { Some(m.clone()) } else { None })
        .collect();
    (out, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::message::Role;

    struct CharCounter;
    impl TokenCounter for CharCounter {
        fn count(&self, t: &str) -> usize {
            t.chars().count()
        }
    }

    fn msg(role: Role, content: &str) -> ChatMessage {
        ChatMessage { role, content: content.into(), ..Default::default() }
    }

    #[test]
    fn over_threshold_compares_against_window_times_ratio() {
        assert!(over_threshold(81, 100, 0.8));
        assert!(!over_threshold(80, 100, 0.8));
    }

    #[test]
    fn trim_keeps_first_and_last_drops_oldest_middle() {
        // sys(2) + h1(10) + h2(10) + h3(10) + last(2) = 34; window 20
        let msgs = vec![
            msg(Role::System, "ss"),
            msg(Role::User, "aaaaaaaaaa"),
            msg(Role::Assistant, "bbbbbbbbbb"),
            msg(Role::User, "cccccccccc"),
            msg(Role::User, "zz"),
        ];
        let (out, dropped) = trim_history(&msgs, 20, &CharCounter);
        assert_eq!(dropped, 2); // h1,h2 dropped
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].content, "ss"); // first system message reserved
        assert_eq!(out[1].content, "cccccccccc"); // recent history reserved
        assert_eq!(out[2].content, "zz"); // last message reserved
    }

    #[test]
    fn trim_noop_when_within_window() {
        let msgs = vec![msg(Role::System, "ss"), msg(Role::User, "hi")];
        let (out, dropped) = trim_history(&msgs, 100, &CharCounter);
        assert_eq!(dropped, 0);
        assert_eq!(out.len(), 2);
    }
}
