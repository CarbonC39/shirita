-- Content hash (sha256 hex) for asset dedup. Nullable; backfilled at startup.
ALTER TABLE assets ADD COLUMN hash TEXT;
