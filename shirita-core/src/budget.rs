//! 上下文预算与 best-effort 历史裁剪：纯函数、无 I/O。
//! 既定决策：保守安全边际、不做严格逐 token 裁剪，溢出优雅暴露（见 M6 spec §1）。

use crate::model::ChatMessage;
use crate::models::message::Role;
use crate::tokenizer::TokenCounter;

/// 用量是否越过触发线（window * threshold）。
pub fn over_threshold(prompt_tokens: usize, window: usize, threshold: f64) -> bool {
    (prompt_tokens as f64) > (window as f64) * threshold
}

/// best-effort 裁剪：保留前导 system（若有）与末条（当前 user 轮），从最旧的中段历史
/// 逐条丢弃直到总用量 <= window 或只剩受保护的首末。返回（保留的消息，丢弃条数）。
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
    // 仅当首条确为 system 时才将其纳入受保护前导；否则中段从 0 开始（最旧历史也可丢）。
    let mid_start = if messages[0].role == Role::System { 1 } else { 0 };
    let mut keep = vec![true; messages.len()];
    let last = messages.len() - 1;
    let mut dropped = 0usize;
    // 中段 = 索引 mid_start..last，最旧的先丢。
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
        ChatMessage { role, content: content.into() }
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
        assert_eq!(dropped, 2); // h1,h2 丢弃
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].content, "ss"); // 首条 system 保留
        assert_eq!(out[1].content, "cccccccccc"); // 最近 history 保留
        assert_eq!(out[2].content, "zz"); // 末条保留
    }

    #[test]
    fn trim_noop_when_within_window() {
        let msgs = vec![msg(Role::System, "ss"), msg(Role::User, "hi")];
        let (out, dropped) = trim_history(&msgs, 100, &CharCounter);
        assert_eq!(dropped, 0);
        assert_eq!(out.len(), 2);
    }
}
