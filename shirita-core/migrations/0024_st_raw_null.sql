-- Follow-up to 0023: also remove `st_raw` entries whose value is JSON null.
-- 0023 matched on `json_extract(meta, '$.st_raw') IS NOT NULL`, which returns
-- SQL NULL both when the key is absent and when it holds JSON null, so a
-- present `"st_raw": null` was left behind. This migration matches on
-- `json_type(...) IS NOT NULL` instead (returns the string 'null' for a
-- present JSON null, SQL NULL only when the key is absent), and is safe to
-- run after 0023 on the same database.
UPDATE definitions
SET meta = json_remove(meta, '$.st_raw')
WHERE json_valid(meta) = 1 AND json_type(meta, '$.st_raw') IS NOT NULL;
