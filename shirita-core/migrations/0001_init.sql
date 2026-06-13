CREATE TABLE IF NOT EXISTS definitions (
    id      TEXT PRIMARY KEY,
    type    TEXT NOT NULL,
    name    TEXT NOT NULL,
    content TEXT NOT NULL,
    meta    TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS chat_sessions (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    avatar          TEXT,
    override_config TEXT NOT NULL DEFAULT '{}',
    current_state   TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS messages (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
    parent_id       TEXT REFERENCES messages(id) ON DELETE CASCADE,
    role            TEXT NOT NULL,
    raw_content     TEXT NOT NULL,
    display_content TEXT,
    is_hidden       INTEGER NOT NULL DEFAULT 0,
    snapshot_state  TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_definitions_type ON definitions(type);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
CREATE INDEX IF NOT EXISTS idx_messages_parent  ON messages(parent_id);
