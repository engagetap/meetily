use sqlx::SqlitePool;

use crate::database::repositories::{
    BookmarksRepository, ScreenshotInput, ScreenshotsRepository,
};

#[derive(Debug, thiserror::Error)]
pub enum PickerError {
    #[error("db error: {0}")]
    Db(String),
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
