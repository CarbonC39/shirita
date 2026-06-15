-- Track session activity time (for default recency ordering) and a manual
-- sort key (for drag-to-reorder on the home screen). sort_order holds the
-- epoch-millis of last activity by default; a manual reorder overwrites it.
ALTER TABLE chat_sessions ADD COLUMN created_at TEXT NOT NULL DEFAULT '';
ALTER TABLE chat_sessions ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';
ALTER TABLE chat_sessions ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
