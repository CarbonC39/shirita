-- Remove the obsolete top-level `st_raw` member from definition metadata.
-- It was written only by the removed SillyTavern charcard import adapter as
-- opaque round-trip data; no runtime reads it. Preserve every other metadata
-- key and the definition row itself. Idempotent: json_remove is a no-op when
-- the key is absent, and sqlx tracks this migration as already applied.
UPDATE definitions
SET meta = json_remove(meta, '$.st_raw')
WHERE json_valid(meta) = 1 AND json_extract(meta, '$.st_raw') IS NOT NULL;
