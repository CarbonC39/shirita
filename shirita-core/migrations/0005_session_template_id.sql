ALTER TABLE chat_sessions ADD COLUMN template_id TEXT REFERENCES templates(id) ON DELETE SET NULL;
