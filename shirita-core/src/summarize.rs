//! 滚动总结管道：自检阈值 → 选水位线 → 构造请求 → 聚合 provider → 落库摘要。
//! 后台 fire-and-forget 调用（web 层 spawn），幂等可重入（见 M6 spec §2）。

/// 内置默认总结指令（settings `summarize.instruction` 可整体覆盖）。
pub const DEFAULT_INSTRUCTION: &str = "Summarize the prior conversation faithfully and concisely. \
Preserve facts, decisions, character state, world details and any unresolved threads. \
Write plain prose, third person, no preamble and no meta commentary.";

/// 选待折叠区间 `[start, end)`（path 下标）：start = 上一水位线之后，end = 保留最近
/// `keep_recent` 条之前。无可折叠时返回 None。
pub fn fold_range(path_len: usize, prev_cutoff_idx: Option<usize>, keep_recent: usize) -> Option<(usize, usize)> {
    let start = prev_cutoff_idx.map(|i| i + 1).unwrap_or(0);
    let end = path_len.saturating_sub(keep_recent);
    if start < end {
        Some((start, end))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_range_first_fold_keeps_recent() {
        assert_eq!(fold_range(20, None, 10), Some((0, 10)));
    }

    #[test]
    fn fold_range_advances_from_prev_cutoff() {
        assert_eq!(fold_range(20, Some(4), 10), Some((5, 10)));
    }

    #[test]
    fn fold_range_none_when_nothing_new_to_fold() {
        assert_eq!(fold_range(12, Some(4), 10), None); // start 5 >= end 2
        assert_eq!(fold_range(8, None, 10), None); // end saturates to 0
    }
}
