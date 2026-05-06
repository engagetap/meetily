use crate::database::models::MeetingBookmark;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct BookmarksRepository;

impl BookmarksRepository {
    pub async fn create(
        pool: &SqlitePool,
        meeting_id: &str,
        timestamp_ms: i64,
        label: Option<&str>,
        source: &str,
    ) -> Result<MeetingBookmark, sqlx::Error> {
        if !matches!(source, "hotkey" | "api" | "ui") {
            return Err(sqlx::Error::Protocol(format!("invalid source: {}", source)));
        }
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO meeting_bookmarks (id, meeting_id, timestamp_ms, label, source, created_at) VALUES (?,?,?,?,?,?)",
        )
        .bind(&id)
        .bind(meeting_id)
        .bind(timestamp_ms)
        .bind(label)
        .bind(source)
        .bind(created_at)
        .execute(pool)
        .await?;
        Ok(MeetingBookmark {
            id,
            meeting_id: meeting_id.to_string(),
            timestamp_ms,
            label: label.map(str::to_string),
            source: source.to_string(),
            created_at,
        })
    }

    pub async fn list_for_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<MeetingBookmark>, sqlx::Error> {
        sqlx::query_as::<_, MeetingBookmark>(
            "SELECT * FROM meeting_bookmarks WHERE meeting_id=? ORDER BY timestamp_ms ASC",
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
    async fn rejects_invalid_source() {
        let pool = pool().await;
        let err = BookmarksRepository::create(&pool, "m1", 100, None, "bogus")
            .await
            .unwrap_err();
        match err {
            sqlx::Error::Protocol(_) => {}
            other => panic!("expected Protocol, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn create_and_list_in_order() {
        let pool = pool().await;
        BookmarksRepository::create(&pool, "m1", 2000, Some("b"), "ui")
            .await
            .unwrap();
        BookmarksRepository::create(&pool, "m1", 1000, Some("a"), "hotkey")
            .await
            .unwrap();
        let list = BookmarksRepository::list_for_meeting(&pool, "m1").await.unwrap();
        assert_eq!(
            list.iter().map(|b| b.timestamp_ms).collect::<Vec<_>>(),
            vec![1000, 2000]
        );
    }
}
