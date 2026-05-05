use crate::database::models::MeetingScreenshot;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct ScreenshotsRepository;

#[derive(Debug, Clone, Default)]
pub struct ScreenshotInput<'a> {
    pub meeting_id: &'a str,
    pub timestamp_ms: i64,
    pub crop: Option<(i64, i64, i64, i64)>,
    pub caption: Option<&'a str>,
    pub source: &'a str,
    pub confidence: Option<f64>,
    pub accepted: bool,
}

impl ScreenshotsRepository {
    pub async fn insert(
        pool: &SqlitePool,
        input: ScreenshotInput<'_>,
    ) -> Result<MeetingScreenshot, sqlx::Error> {
        if !matches!(
            input.source,
            "bookmark" | "transcript_cue" | "frame_diff" | "cloud_vision"
        ) {
            return Err(sqlx::Error::Protocol(format!(
                "invalid source: {}",
                input.source
            )));
        }
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();
        let (cx, cy, cw, ch) = input
            .crop
            .map(|c| (Some(c.0), Some(c.1), Some(c.2), Some(c.3)))
            .unwrap_or((None, None, None, None));
        sqlx::query(
            "INSERT INTO meeting_screenshots
                (id, meeting_id, timestamp_ms, crop_x, crop_y, crop_w, crop_h,
                 image_path, caption, source, confidence, accepted, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(input.meeting_id)
        .bind(input.timestamp_ms)
        .bind(cx)
        .bind(cy)
        .bind(cw)
        .bind(ch)
        .bind(input.caption)
        .bind(input.source)
        .bind(input.confidence)
        .bind(if input.accepted { 1 } else { 0 })
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;
        Ok(MeetingScreenshot {
            id,
            meeting_id: input.meeting_id.to_string(),
            timestamp_ms: input.timestamp_ms,
            crop_x: cx,
            crop_y: cy,
            crop_w: cw,
            crop_h: ch,
            image_path: None,
            caption: input.caption.map(str::to_string),
            source: input.source.to_string(),
            confidence: input.confidence,
            accepted: if input.accepted { 1 } else { 0 },
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn list_for_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<MeetingScreenshot>, sqlx::Error> {
        sqlx::query_as::<_, MeetingScreenshot>(
            "SELECT * FROM meeting_screenshots WHERE meeting_id=? ORDER BY timestamp_ms ASC",
        )
        .bind(meeting_id)
        .fetch_all(pool)
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
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1','t','t','t')")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn inserts_with_crop_and_lists() {
        let pool = pool().await;
        let input = ScreenshotInput {
            meeting_id: "m1",
            timestamp_ms: 5000,
            crop: Some((10, 20, 800, 600)),
            caption: Some("dashboard"),
            source: "bookmark",
            confidence: Some(1.0),
            accepted: true,
        };
        let row = ScreenshotsRepository::insert(&pool, input).await.unwrap();
        assert_eq!(row.crop_w, Some(800));
        let list = ScreenshotsRepository::list_for_meeting(&pool, "m1").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].accepted, 1);
    }

    #[tokio::test]
    async fn rejects_invalid_source() {
        let pool = pool().await;
        let input = ScreenshotInput {
            meeting_id: "m1",
            timestamp_ms: 0,
            source: "weird",
            ..Default::default()
        };
        let err = ScreenshotsRepository::insert(&pool, input).await.unwrap_err();
        match err {
            sqlx::Error::Protocol(_) => {}
            other => panic!("got {:?}", other),
        }
    }
}
