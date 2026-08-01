-- Auth: opaque session tokens issued at login (and auto-provisioned on desktop).
-- `token` is the bearer secret (32 random bytes, base64url). The user_id FK is
-- ON DELETE CASCADE so a future user delete cleans up its sessions.
CREATE TABLE IF NOT EXISTS sessions (
    token      TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT '',
    expires_at TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);
