-- 滚动摘要侧带表：按 cutoff_message_id 锚定水位线，不进消息树（M6 spec §3）。
CREATE TABLE summaries (
    id                TEXT PRIMARY KEY,
    session_id        TEXT NOT NULL,
    cutoff_message_id TEXT NOT NULL,
    content           TEXT NOT NULL,
    created_at        TEXT NOT NULL
);
CREATE INDEX idx_summaries_session ON summaries(session_id);
