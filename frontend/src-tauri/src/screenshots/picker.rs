use std::path::Path;

use sqlx::SqlitePool;

use crate::database::repositories::{
    BookmarksRepository, ScreenshotInput, ScreenshotsRepository,
};
use crate::screenshots::scanner::scan_for_novel_frames;

#[derive(Debug, thiserror::Error)]
pub enum PickerError {
    #[error("db error: {0}")]
    Db(String),
    #[error("scanner error: {0}")]
    Scanner(String),
}

/// Phase 2 first-pass picker: every bookmark on `meeting_id` becomes a
/// pre-accepted screenshot candidate at the same timestamp. The full frame
/// is the default; the user can crop in the review UI.
///
/// Subsequent runs are idempotent at the row level — we don't dedupe yet
/// (Phase 2.5 with transcript cues / frame-diff will collide-merge).
///
/// Returns the number of candidates created.
pub async fn generate_candidates_from_bookmarks(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<usize, PickerError> {
    let bookmarks = BookmarksRepository::list_for_meeting(pool, meeting_id)
        .await
        .map_err(|e| PickerError::Db(e.to_string()))?;

    let mut count = 0;
    for b in bookmarks {
        let input = ScreenshotInput {
            meeting_id,
            timestamp_ms: b.timestamp_ms,
            crop: None,
            caption: b.label.as_deref(),
            source: "bookmark",
            confidence: Some(1.0),
            accepted: true,
        };
        ScreenshotsRepository::insert(pool, input)
            .await
            .map_err(|e| PickerError::Db(e.to_string()))?;
        count += 1;
    }

    Ok(count)
}

/// Phase 2.5: produces "frame-diff" candidates by scanning the video for
/// stable + novel frames at low fps. Each surviving timestamp becomes a
/// pending (not pre-accepted) screenshot row so the user reviews it before
/// it lands in the highlights gallery.
///
/// Suppresses any frame-diff candidate whose timestamp is within
/// `dedup_tolerance_ms` of an existing candidate (typically a bookmark)
/// to avoid showing the user near-duplicates.
pub async fn generate_frame_diff_candidates(
    pool: &SqlitePool,
    meeting_id: &str,
    video_path: &Path,
    sample_fps: u32,
    max_candidates: usize,
    dedup_tolerance_ms: i64,
) -> Result<usize, PickerError> {
    let proposed = tokio::task::block_in_place(|| {
        scan_for_novel_frames(video_path, sample_fps, max_candidates)
    })
    .map_err(|e| PickerError::Scanner(e.to_string()))?;

    let existing = ScreenshotsRepository::list_for_meeting(pool, meeting_id)
        .await
        .map_err(|e| PickerError::Db(e.to_string()))?;
    let existing_ts: Vec<i64> = existing.iter().map(|s| s.timestamp_ms).collect();

    let mut count = 0;
    for ts in proposed {
        let too_close = existing_ts
            .iter()
            .any(|e| (e - ts).abs() <= dedup_tolerance_ms);
        if too_close {
            continue;
        }
        let input = ScreenshotInput {
            meeting_id,
            timestamp_ms: ts,
            crop: None,
            caption: None,
            source: "frame_diff",
            confidence: Some(0.5),
            accepted: false,
        };
        ScreenshotsRepository::insert(pool, input)
            .await
            .map_err(|e| PickerError::Db(e.to_string()))?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::BookmarksRepository;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE meetings (id TEXT PRIMARY KEY, title TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)").execute(&pool).await.unwrap();
        sqlx::query(include_str!(
            "../../migrations/20260505100000_add_video_recording_tables.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1','t','t','t')").execute(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn no_bookmarks_yields_zero_candidates() {
        let pool = pool().await;
        let n = generate_candidates_from_bookmarks(&pool, "m1").await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn each_bookmark_becomes_an_accepted_candidate() {
        let pool = pool().await;
        BookmarksRepository::create(&pool, "m1", 1000, Some("intro"), "ui")
            .await
            .unwrap();
        BookmarksRepository::create(&pool, "m1", 5000, None, "hotkey")
            .await
            .unwrap();
        let n = generate_candidates_from_bookmarks(&pool, "m1").await.unwrap();
        assert_eq!(n, 2);

        let shots = ScreenshotsRepository::list_for_meeting(&pool, "m1")
            .await
            .unwrap();
        assert_eq!(shots.len(), 2);
        for s in &shots {
            assert_eq!(s.accepted, 1);
            assert_eq!(s.source, "bookmark");
        }
        // The first bookmark's label propagated.
        assert!(shots.iter().any(|s| s.caption.as_deref() == Some("intro")));
    }
}
