CREATE TABLE IF NOT EXISTS meeting_recordings (
    id            TEXT PRIMARY KEY,
    meeting_id    TEXT NOT NULL,
    file_path     TEXT NOT NULL,
    started_at    INTEGER NOT NULL,
    ended_at      INTEGER,
    width         INTEGER,
    height        INTEGER,
    fps           INTEGER,
    codec         TEXT,
    display_id    INTEGER,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_recordings_meeting ON meeting_recordings(meeting_id);

CREATE TABLE IF NOT EXISTS meeting_bookmarks (
    id            TEXT PRIMARY KEY,
    meeting_id    TEXT NOT NULL,
    timestamp_ms  INTEGER NOT NULL,
    label         TEXT,
    source        TEXT NOT NULL CHECK (source IN ('hotkey','api','ui')),
    created_at    INTEGER NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_bookmarks_meeting_ts ON meeting_bookmarks(meeting_id, timestamp_ms);

CREATE TABLE IF NOT EXISTS meeting_screenshots (
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
    updated_at    INTEGER NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_screenshots_meeting_ts ON meeting_screenshots(meeting_id, timestamp_ms);
