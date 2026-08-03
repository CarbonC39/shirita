-- Typed MCP server records. `transport` holds the full typed config, which may
-- contain secret header/env values; the API never returns them (it returns a
-- redacted view with `has_secret`). Timestamps are opaque sort keys.
CREATE TABLE IF NOT EXISTS mcp_servers (
    id                TEXT PRIMARY KEY,
    name              TEXT NOT NULL,
    enabled           INTEGER NOT NULL DEFAULT 0,
    transport         TEXT NOT NULL,
    request_timeout_ms INTEGER NOT NULL DEFAULT 5000,
    created_at        TEXT NOT NULL DEFAULT '',
    updated_at        TEXT NOT NULL DEFAULT ''
);
