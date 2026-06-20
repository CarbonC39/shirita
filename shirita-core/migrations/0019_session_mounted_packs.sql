-- Ordered list of mounted pack ids, mirroring mounted_definitions.
ALTER TABLE chat_sessions ADD COLUMN mounted_packs TEXT NOT NULL DEFAULT '[]';
