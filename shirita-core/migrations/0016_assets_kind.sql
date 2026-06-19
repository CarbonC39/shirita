-- Split the media library by kind: avatars vs backgrounds. Existing rows
-- default to 'background' (they were used for the app background).
ALTER TABLE assets ADD COLUMN kind TEXT NOT NULL DEFAULT 'background';
