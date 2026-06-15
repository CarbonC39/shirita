-- The leaf message of the session's currently active branch (NULL = no messages).
ALTER TABLE chat_sessions ADD COLUMN active_leaf_id TEXT;
