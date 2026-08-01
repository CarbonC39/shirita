-- Remove the obsolete top-level `st_raw` member from definition metadata.
-- It was written only by the removed SillyTavern charcard import adapter as
-- opaque round-trip data; no runtime reads it. Preserve every other metadata
-- key and the definition row itself. Idempotent: json_remove is a no-op when
-- the key is absent, and sqlx tracks this migration as already applied.
-- Match on json_type rather than json_extract IS NOT NULL: json_extract
-- returns SQL NULL both when the key is absent AND when it holds JSON null,
-- so a present `"st_raw": null` would otherwise be left behind. json_type
-- returns the string 'null' for a present JSON null and SQL NULL only when the
-- key is absent.
UPDATE definitions
SET meta = json_remove(meta, '$.st_raw')
WHERE json_valid(meta) = 1 AND json_type(meta, '$.st_raw') IS NOT NULL;
