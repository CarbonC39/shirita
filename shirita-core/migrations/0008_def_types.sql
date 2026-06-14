CREATE TABLE def_types (
    id         TEXT PRIMARY KEY,                 -- stable english id, e.g. char/persona/world
    label      TEXT NOT NULL,                    -- display name (i18n-able)
    sort       INTEGER NOT NULL DEFAULT 0,
    builtin    INTEGER NOT NULL DEFAULT 0,       -- 1 = cannot delete
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO def_types (id, label, sort, builtin) VALUES
    ('char',    'Character', 0, 1),
    ('persona', 'User',      1, 1),
    ('world',   'World',     2, 1);
