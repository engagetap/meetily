-- Phase 1B: drop the foreign-key constraint on meeting_id from
-- meeting_recordings, meeting_bookmarks, and meeting_screenshots.
--
-- The original schema (20260505100000) tied screen recordings strictly to
-- existing meetings, but the audio + screen recording flows now generate
-- their own ids independently — audio creates a `meetings.id` lazily when
-- the first transcripts arrive, while screen recording creates a
-- `meeting_recordings.id` immediately at start time. Linking the two
-- happens at view time via timestamp-proximity (see
-- RecordingsRepository::nearest_to_timestamp).
--
-- SQLite doesn't support ALTER TABLE DROP CONSTRAINT, so we use the
-- canonical recreate-and-copy pattern. PRAGMA foreign_keys is left to
-- whatever the caller configured (sqlx defaults to OFF during migrations).

PRAGMA foreign_keys=OFF;

-- meeting_recordings
CREATE TABLE meeting_recordings_new (
    id            TEXT PRIMARY KEY,
    meeting_id    TEXT NOT NULL,
    file_path     TEXT NOT NULL,
    started_at    INTEGER NOT NULL,
    ended_at      INTEGER,
    width         INTEGER,
    height        INTEGER,
    fps           INTEGER,
    codec         TEXT,
    display_id    INTEGER
);
INSERT INTO meeting_recordings_new (id, meeting_id, file_path, started_at, ended_at, width, height, fps, codec, display_id)
SELECT id, meeting_id, file_path, started_at, ended_at, width, height, fps, codec, display_id
FROM meeting_recordings;
DROP TABLE meeting_recordings;
ALTER TABLE meeting_recordings_new RENAME TO meeting_recordings;
CREATE INDEX IF NOT EXISTS idx_recordings_meeting ON meeting_recordings(meeting_id);

-- meeting_bookmarks
CREATE TABLE meeting_bookmarks_new (
    id            TEXT PRIMARY KEY,
    meeting_id    TEXT NOT NULL,
    timestamp_ms  INTEGER NOT NULL,
    label         TEXT,
    source        TEXT NOT NULL CHECK (source IN ('hotkey','api','ui')),
    created_at    INTEGER NOT NULL
);
INSERT INTO meeting_bookmarks_new (id, meeting_id, timestamp_ms, label, source, created_at)
SELECT id, meeting_id, timestamp_ms, label, source, created_at
FROM meeting_bookmarks;
DROP TABLE meeting_bookmarks;
ALTER TABLE meeting_bookmarks_new RENAME TO meeting_bookmarks;
CREATE INDEX IF NOT EXISTS idx_bookmarks_meeting_ts ON meeting_bookmarks(meeting_id, timestamp_ms);

-- meeting_screenshots
CREATE TABLE meeting_screenshots_new (
    id            TEXT PRIMARY KEY,
    meeting_id    TEXT NOT NULL,
    timestamp_ms  INTEGER NOT NULL,
    crop_x        INTEGER,
    crop_y        INTEGER,
    crop_w        INTEGER,
    crop_h        INTEGER,
    image_path    TEXT,
    caption       TEXT,
    source        TEXT NOT NULL
                  CHECK (source IN ('bookmark','transcript_cue','frame_diff','cloud_vision')),
    confidence    REAL,
    accepted      INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);
INSERT INTO meeting_screenshots_new (id, meeting_id, timestamp_ms, crop_x, crop_y, crop_w, crop_h, image_path, caption, source, confidence, accepted, created_at, updated_at)
SELECT id, meeting_id, timestamp_ms, crop_x, crop_y, crop_w, crop_h, image_path, caption, source, confidence, accepted, created_at, updated_at
FROM meeting_screenshots;
DROP TABLE meeting_screenshots;
ALTER TABLE meeting_screenshots_new RENAME TO meeting_screenshots;
CREATE INDEX IF NOT EXISTS idx_screenshots_meeting_ts ON meeting_screenshots(meeting_id, timestamp_ms);

PRAGMA foreign_keys=ON;
