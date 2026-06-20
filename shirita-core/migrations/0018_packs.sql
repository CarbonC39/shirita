CREATE TABLE IF NOT EXISTS packs (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    identity_json TEXT NOT NULL DEFAULT '{}',
    meta          TEXT NOT NULL DEFAULT '{}',
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
);
