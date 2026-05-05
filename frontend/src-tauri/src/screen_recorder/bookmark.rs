use std::time::Instant;

use chrono::Utc;
use sqlx::SqlitePool;

use crate::database::repositories::BookmarksRepository;

/// Public-facing source of a bookmark trigger. Persisted to the
/// `meeting_bookmarks.source` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookmarkSource {
    Hotkey,
    Api,
    Ui,
}

impl BookmarkSource {
    pub fn as_str(self) -> &'static str {
        match self {
            BookmarkSource::Hotkey => "hotkey",
            BookmarkSource::Api => "api",
            BookmarkSource::Ui => "ui",
        }
    }
}

/// Result of a bookmark drop. `meeting_id` and `timestamp_ms` are returned
/// so the controller (hotkey/UI/API) can surface a confirmation toast.
#[derive(Debug, Clone)]
pub struct DroppedBookmark {
    pub id: String,
    pub meeting_id: String,
    pub timestamp_ms: i64,
}

/// Errors a bookmark drop can produce. Distinguished so the API layer can
/// translate cleanly to HTTP status codes.
#[derive(Debug, thiserror::Error, serde::Serialize)]
#[serde(tag = "type", content = "message")]
pub enum BookmarkError {
    #[error("not currently recording")]
    NotRecording,
    #[error("db error: {0}")]
    Db(String),
}

/// Drops a bookmark at "now" against the active recording.
///
/// `recording_started_at` is the `Instant` the active recording started; the
/// bookmark's `timestamp_ms` is the elapsed time since that instant. This
/// keeps timestamps anchored to the video, not wall-clock time.
pub async fn drop_bookmark(
    pool: &SqlitePool,
    meeting_id: &str,
    recording_started_at: Instant,
    label: Option<&str>,
    source: BookmarkSource,
) -> Result<DroppedBookmark, BookmarkError> {
    let timestamp_ms = recording_started_at.elapsed().as_millis() as i64;
    let row = BookmarksRepository::create(pool, meeting_id, timestamp_ms, label, source.as_str())
        .await
        .map_err(|e| BookmarkError::Db(e.to_string()))?;
    Ok(DroppedBookmark {
        id: row.id,
        meeting_id: row.meeting_id,
        timestamp_ms,
    })
}

/// Convenience for callers that want to record a bookmark without an active
/// recording context (e.g. attached to a meeting's wall-clock time). Not
/// used by Phase 1B's main flow — kept for completeness when the feature
/// expands beyond an active recording.
#[allow(dead_code)]
pub async fn drop_bookmark_at_wall_clock(
    pool: &SqlitePool,
    meeting_id: &str,
    label: Option<&str>,
    source: BookmarkSource,
) -> Result<DroppedBookmark, BookmarkError> {
    let timestamp_ms = Utc::now().timestamp_millis();
    let row = BookmarksRepository::create(pool, meeting_id, timestamp_ms, label, source.as_str())
        .await
        .map_err(|e| BookmarkError::Db(e.to_string()))?;
    Ok(DroppedBookmark {
        id: row.id,
        meeting_id: row.meeting_id,
        timestamp_ms,
    })
}
