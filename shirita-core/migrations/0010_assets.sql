-- Named media library: uploaded images get a friendly, editable name and are
-- shared by avatars and backgrounds.
CREATE TABLE IF NOT EXISTS assets (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    path       TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT ''
);
