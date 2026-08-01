-- Auth: user accounts. Single account today, but the schema (per-row id +
-- UNIQUE username) supports many. `password_hash` is an argon2 PHC string; an
-- empty string means "no interactive password set yet" (a desktop
-- auto-provisioned account until the user sets one in Settings).
CREATE TABLE IF NOT EXISTS users (
    id            TEXT PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL DEFAULT '',
    created_at    TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
