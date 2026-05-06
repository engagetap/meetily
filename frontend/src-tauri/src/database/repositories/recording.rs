use crate::database::models::MeetingRecording;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct RecordingsRepository;

impl RecordingsRepository {
    pub async fn create(
        pool: &SqlitePool,
        meeting_id: &str,
        file_path: &str,
        display_id: Option<i64>,
    ) -> Result<MeetingRecording, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let started_at = Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO meeting_recordings (id, meeting_id, file_path, started_at, display_id) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(meeting_id)
        .bind(file_path)
        .bind(started_at)
        .bind(display_id)
        .execute(pool)
        .await?;

        Ok(MeetingRecording {
            id,
            meeting_id: meeting_id.to_string(),
            file_path: file_path.to_string(),
            started_at,
            ended_at: None,
            width: None,
            height: None,
            fps: None,
            codec: None,
            display_id,
        })
    }

    pub async fn finalize(
        pool: &SqlitePool,
        id: &str,
        ended_at: i64,
        width: Option<i64>,
        height: Option<i64>,
        fps: Option<i64>,
        codec: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE meeting_recordings SET ended_at=?, width=?, height=?, fps=?, codec=? WHERE id=?",
        )
        .bind(ended_at)
        .bind(width)
        .bind(height)
        .bind(fps)
        .bind(codec)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn list_for_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<MeetingRecording>, sqlx::Error> {
        sqlx::query_as::<_, MeetingRecording>(
            "SELECT * FROM meeting_recordings WHERE meeting_id = ? ORDER BY started_at ASC",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
    }

    /// Returns the most recently-finalized recording for a meeting. Used by
    /// the screenshot pipeline to locate the video file to extract from.
    pub async fn latest_finalized_for_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<MeetingRecording>, sqlx::Error> {
        sqlx::query_as::<_, MeetingRecording>(
            "SELECT * FROM meeting_recordings
             WHERE meeting_id = ? AND ended_at IS NOT NULL
             ORDER BY ended_at DESC LIMIT 1",
        )
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
    }

    /// Returns the recording whose `started_at` is closest to `near_ms` and
    /// within `tolerance_ms`. Used to link a screen recording to an audio
    /// meeting whose ids were generated independently (proximity match).
    pub async fn nearest_to_timestamp(
        pool: &SqlitePool,
        near_ms: i64,
        tolerance_ms: i64,
    ) -> Result<Option<MeetingRecording>, sqlx::Error> {
        sqlx::query_as::<_, MeetingRecording>(
            "SELECT * FROM meeting_recordings
             WHERE ABS(started_at - ?) <= ?
             ORDER BY ABS(started_at - ?) ASC LIMIT 1",
        )
        .bind(near_ms)
        .bind(tolerance_ms)
        .bind(near_ms)
        .fetch_optional(pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE meetings (id TEXT PRIMARY KEY, title TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)")
            .execute(&pool).await.unwrap();
        sqlx::query(include_str!(
            "../../../migrations/20260505100000_add_video_recording_tables.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(include_str!(
            "../../../migrations/20260506000000_drop_screen_recording_fks.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1','t','t','t')")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn create_then_finalize_roundtrip() {
        let pool = pool().await;
        let r = RecordingsRepository::create(&pool, "m1", "/tmp/x.mp4", Some(1))
            .await
            .unwrap();
        assert_eq!(r.meeting_id, "m1");
        assert!(r.ended_at.is_none());
        RecordingsRepository::finalize(&pool, &r.id, 12345, Some(1920), Some(1080), Some(30), Some("h264"))
            .await
            .unwrap();
        let list = RecordingsRepository::list_for_meeting(&pool, "m1").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].ended_at, Some(12345));
        assert_eq!(list[0].width, Some(1920));
    }
}
