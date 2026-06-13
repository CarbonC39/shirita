CREATE TABLE IF NOT EXISTS templates (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    meta       TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS prompt_nodes (
    id            TEXT PRIMARY KEY,
    owner_kind    TEXT NOT NULL CHECK(owner_kind IN ('template', 'session')),
    owner_id      TEXT NOT NULL,
    parent_id     TEXT REFERENCES prompt_nodes(id) ON DELETE CASCADE,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    kind          TEXT NOT NULL CHECK(kind IN ('folder', 'ref')),
    tag           TEXT,
    definition_id TEXT REFERENCES definitions(id) ON DELETE SET NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_prompt_nodes_owner ON prompt_nodes(owner_kind, owner_id);
CREATE INDEX IF NOT EXISTS idx_prompt_nodes_parent ON prompt_nodes(parent_id);
