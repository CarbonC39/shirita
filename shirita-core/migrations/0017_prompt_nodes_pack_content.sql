-- Relax prompt_nodes CHECKs: allow owner_kind='pack' and kind='content'.
-- SQLite can't alter a CHECK in place, so rebuild (mirrors 0007). Preserves the
-- meta column added in 0015.
PRAGMA foreign_keys=OFF;

CREATE TABLE prompt_nodes_new (
    id            TEXT PRIMARY KEY,
    owner_kind    TEXT NOT NULL CHECK(owner_kind IN ('template', 'session', 'pack')),
    owner_id      TEXT NOT NULL,
    parent_id     TEXT REFERENCES prompt_nodes_new(id) ON DELETE CASCADE,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    kind          TEXT NOT NULL CHECK(kind IN ('folder', 'ref', 'history', 'content')),
    tag           TEXT,
    definition_id TEXT REFERENCES definitions(id) ON DELETE SET NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    meta          TEXT NOT NULL DEFAULT '{}'
);

INSERT INTO prompt_nodes_new
    SELECT id, owner_kind, owner_id, parent_id, sort_order, kind, tag, definition_id, enabled, created_at, meta
    FROM prompt_nodes;

DROP TABLE prompt_nodes;
ALTER TABLE prompt_nodes_new RENAME TO prompt_nodes;

CREATE INDEX IF NOT EXISTS idx_prompt_nodes_owner ON prompt_nodes(owner_kind, owner_id);
CREATE INDEX IF NOT EXISTS idx_prompt_nodes_parent ON prompt_nodes(parent_id);

PRAGMA foreign_keys=ON;
