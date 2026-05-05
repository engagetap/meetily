# Phase 1A — Recording Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a working ScreenCaptureKit-based screen + system-audio recorder to Meetily, exposed through Tauri commands, plus the database schema that subsequent phases will populate.

**Architecture:** Pure Rust, using the project's existing `cidre` dependency. We add the `sc` feature flag and use `sc::RecordingOutput` (macOS 15+ high-level API) which writes a properly muxed `.mp4` for free — no manual `VTCompressionSession`, no separate Swift module, no FFI. Three new SQLite tables (`meeting_recordings`, `meeting_bookmarks`, `meeting_screenshots`) are created up front so all later phases can write to a stable schema.

**Tech Stack:** Tauri 2 (Rust), `cidre` (existing dep) for ScreenCaptureKit, sqlx + SQLite, Next.js for the dev page.

**Platform constraint:** macOS 15 (Sequoia) or newer. The user's target machine is on 26.2; this is acceptable for the personal-use scope. If upstreaming later we'd add a fallback path using `VTCompressionSession` for macOS 12.3–14.x.

---

## File structure

**New files:**
- `frontend/src-tauri/src/screen_recorder/mod.rs` — module entry
- `frontend/src-tauri/src/screen_recorder/types.rs` — public types (DisplayInfo, RecordingMeta, errors)
- `frontend/src-tauri/src/screen_recorder/recorder.rs` — `ScreenRecorder` struct using cidre
- `frontend/src-tauri/src/screen_recorder/commands.rs` — Tauri commands
- `frontend/src-tauri/migrations/20260505100000_add_video_recording_tables.sql` — schema
- `frontend/src-tauri/src/database/repositories/recording.rs` — `RecordingsRepository`
- `frontend/src-tauri/src/database/repositories/bookmark.rs` — `BookmarksRepository`
- `frontend/src-tauri/src/database/repositories/screenshot.rs` — `ScreenshotsRepository`
- `frontend/src-tauri/tests/screen_recorder_smoke.rs` — integration test
- `frontend/src/app/dev/screen-recorder/page.tsx` — dev-only test page

**Modified files:**
- `frontend/src-tauri/Cargo.toml` — add `sc` feature to existing `cidre`
- `frontend/src-tauri/src/database/models.rs` — additions
- `frontend/src-tauri/src/database/repositories/mod.rs` — re-exports
- `frontend/src-tauri/src/lib.rs` — register module + state + commands

**Module boundaries:**
- `screen_recorder` knows nothing about meetings — just records a display to a file.
- `RecordingsRepository` is the only thing tying a recording file path to a `meeting_id`.
- `meeting_bookmarks` and `meeting_screenshots` tables exist after Phase 1A; only `meeting_recordings` is written this phase. Their repositories are scaffolded so Phase 1B/2 can pick them up.

---

## Task 1: Enable `sc` feature on cidre

**Files:**
- Modify: `frontend/src-tauri/Cargo.toml`

- [ ] **Step 1: Add `sc` (and supporting features) to cidre**

In `frontend/src-tauri/Cargo.toml`, find the `[target.'cfg(target_os = "macos")'.dependencies]` section. Replace the existing cidre line with:

```toml
cidre = { git = "https://github.com/yury/cidre", rev = "a9587fa", features = ["av", "sc", "cm", "vt", "macos_15_0"] }
```

(Adds `sc` for ScreenCaptureKit, `cm`/`vt` for time/format helpers, and the `macos_15_0` API gate for SCRecordingOutput.)

- [ ] **Step 2: Verify**

```bash
cd frontend/src-tauri && cargo check 2>&1 | tail -10
```
Expected: succeeds (or fails only on actual code we haven't written yet).

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/Cargo.toml frontend/src-tauri/Cargo.lock
git commit -m "build(deps): enable cidre sc feature for ScreenCaptureKit"
```

---

## Task 2: Database migration

**Files:**
- Create: `frontend/src-tauri/migrations/20260505100000_add_video_recording_tables.sql`

- [ ] **Step 1: Write migration SQL**

```sql
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
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src-tauri/migrations/20260505100000_add_video_recording_tables.sql
git commit -m "feat(db): migration for recordings, bookmarks, screenshots"
```

---

## Task 3: Database models

**Files:**
- Modify: `frontend/src-tauri/src/database/models.rs`

- [ ] **Step 1: Append models** (do not remove existing code; add to the bottom of the file)

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct MeetingRecording {
    pub id: String,
    pub meeting_id: String,
    pub file_path: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub fps: Option<i64>,
    pub codec: Option<String>,
    pub display_id: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct MeetingBookmark {
    pub id: String,
    pub meeting_id: String,
    pub timestamp_ms: i64,
    pub label: Option<String>,
    pub source: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct MeetingScreenshot {
    pub id: String,
    pub meeting_id: String,
    pub timestamp_ms: i64,
    pub crop_x: Option<i64>,
    pub crop_y: Option<i64>,
    pub crop_w: Option<i64>,
    pub crop_h: Option<i64>,
    pub image_path: Option<String>,
    pub caption: Option<String>,
    pub source: String,
    pub confidence: Option<f64>,
    pub accepted: i64,
    pub created_at: i64,
    pub updated_at: i64,
}
```

- [ ] **Step 2: Verify**

```bash
cd frontend/src-tauri && cargo check 2>&1 | tail -10
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/database/models.rs
git commit -m "feat(db): models for recordings, bookmarks, screenshots"
```

---

## Task 4: RecordingsRepository

**Files:**
- Create: `frontend/src-tauri/src/database/repositories/recording.rs`
- Modify: `frontend/src-tauri/src/database/repositories/mod.rs`

- [ ] **Step 1: Write the repository with inline tests**

```rust
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
            "UPDATE meeting_recordings SET ended_at=?, width=?, height=?, fps=?, codec=? WHERE id=?"
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
            "SELECT * FROM meeting_recordings WHERE meeting_id = ? ORDER BY started_at ASC"
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
        sqlx::query(include_str!("../../../migrations/20260505100000_add_video_recording_tables.sql"))
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1','t','t','t')")
            .execute(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn create_then_finalize_roundtrip() {
        let pool = pool().await;
        let r = RecordingsRepository::create(&pool, "m1", "/tmp/x.mp4", Some(1)).await.unwrap();
        assert_eq!(r.meeting_id, "m1");
        assert!(r.ended_at.is_none());
        RecordingsRepository::finalize(&pool, &r.id, 12345, Some(1920), Some(1080), Some(30), Some("h264")).await.unwrap();
        let list = RecordingsRepository::list_for_meeting(&pool, "m1").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].ended_at, Some(12345));
        assert_eq!(list[0].width, Some(1920));
    }
}
```

- [ ] **Step 2: Re-export**

In `frontend/src-tauri/src/database/repositories/mod.rs`, add:
```rust
pub mod recording;
pub use recording::RecordingsRepository;
```

- [ ] **Step 3: Run tests**
```bash
cd frontend/src-tauri && cargo test --lib database::repositories::recording 2>&1 | tail -10
```
Expected: 1 test passes.

- [ ] **Step 4: Commit**
```bash
git add frontend/src-tauri/src/database/repositories/
git commit -m "feat(db): RecordingsRepository with create/finalize/list"
```

---

## Task 5: BookmarksRepository

**Files:**
- Create: `frontend/src-tauri/src/database/repositories/bookmark.rs`
- Modify: `frontend/src-tauri/src/database/repositories/mod.rs`

- [ ] **Step 1: Write repository + tests**

```rust
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
            "INSERT INTO meeting_bookmarks (id, meeting_id, timestamp_ms, label, source, created_at) VALUES (?,?,?,?,?,?)"
        )
        .bind(&id).bind(meeting_id).bind(timestamp_ms).bind(label).bind(source).bind(created_at)
        .execute(pool).await?;
        Ok(MeetingBookmark {
            id, meeting_id: meeting_id.to_string(), timestamp_ms,
            label: label.map(str::to_string),
            source: source.to_string(),
            created_at,
        })
    }

    pub async fn list_for_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<MeetingBookmark>, sqlx::Error> {
        sqlx::query_as::<_, MeetingBookmark>(
            "SELECT * FROM meeting_bookmarks WHERE meeting_id=? ORDER BY timestamp_ms ASC"
        ).bind(meeting_id).fetch_all(pool).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE meetings (id TEXT PRIMARY KEY, title TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)").execute(&pool).await.unwrap();
        sqlx::query(include_str!("../../../migrations/20260505100000_add_video_recording_tables.sql")).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1','t','t','t')").execute(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn rejects_invalid_source() {
        let pool = pool().await;
        let err = BookmarksRepository::create(&pool, "m1", 100, None, "bogus").await.unwrap_err();
        match err {
            sqlx::Error::Protocol(_) => {},
            other => panic!("expected Protocol, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn create_and_list_in_order() {
        let pool = pool().await;
        BookmarksRepository::create(&pool, "m1", 2000, Some("b"), "ui").await.unwrap();
        BookmarksRepository::create(&pool, "m1", 1000, Some("a"), "hotkey").await.unwrap();
        let list = BookmarksRepository::list_for_meeting(&pool, "m1").await.unwrap();
        assert_eq!(list.iter().map(|b| b.timestamp_ms).collect::<Vec<_>>(), vec![1000, 2000]);
    }
}
```

- [ ] **Step 2: Re-export, run, commit**
```bash
echo "pub mod bookmark;
pub use bookmark::BookmarksRepository;" >> frontend/src-tauri/src/database/repositories/mod.rs
cd frontend/src-tauri && cargo test --lib database::repositories::bookmark 2>&1 | tail -10
git add frontend/src-tauri/src/database/repositories/
git commit -m "feat(db): BookmarksRepository scaffold + tests"
```

---

## Task 6: ScreenshotsRepository

**Files:**
- Create: `frontend/src-tauri/src/database/repositories/screenshot.rs`
- Modify: `frontend/src-tauri/src/database/repositories/mod.rs`

- [ ] **Step 1: Write repository + tests**

```rust
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
    pub async fn insert(pool: &SqlitePool, input: ScreenshotInput<'_>) -> Result<MeetingScreenshot, sqlx::Error> {
        if !matches!(input.source, "bookmark" | "transcript_cue" | "frame_diff" | "cloud_vision") {
            return Err(sqlx::Error::Protocol(format!("invalid source: {}", input.source)));
        }
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();
        let (cx, cy, cw, ch) = input.crop.map(|c| (Some(c.0), Some(c.1), Some(c.2), Some(c.3))).unwrap_or((None, None, None, None));
        sqlx::query(
            "INSERT INTO meeting_screenshots
                (id, meeting_id, timestamp_ms, crop_x, crop_y, crop_w, crop_h,
                 image_path, caption, source, confidence, accepted, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id).bind(input.meeting_id).bind(input.timestamp_ms)
        .bind(cx).bind(cy).bind(cw).bind(ch)
        .bind(input.caption).bind(input.source).bind(input.confidence)
        .bind(if input.accepted { 1 } else { 0 })
        .bind(now).bind(now)
        .execute(pool).await?;
        Ok(MeetingScreenshot {
            id, meeting_id: input.meeting_id.to_string(), timestamp_ms: input.timestamp_ms,
            crop_x: cx, crop_y: cy, crop_w: cw, crop_h: ch,
            image_path: None,
            caption: input.caption.map(str::to_string),
            source: input.source.to_string(),
            confidence: input.confidence,
            accepted: if input.accepted { 1 } else { 0 },
            created_at: now, updated_at: now,
        })
    }

    pub async fn list_for_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<MeetingScreenshot>, sqlx::Error> {
        sqlx::query_as::<_, MeetingScreenshot>(
            "SELECT * FROM meeting_screenshots WHERE meeting_id=? ORDER BY timestamp_ms ASC"
        ).bind(meeting_id).fetch_all(pool).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE meetings (id TEXT PRIMARY KEY, title TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)").execute(&pool).await.unwrap();
        sqlx::query(include_str!("../../../migrations/20260505100000_add_video_recording_tables.sql")).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1','t','t','t')").execute(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn inserts_with_crop_and_lists() {
        let pool = pool().await;
        let input = ScreenshotInput {
            meeting_id: "m1", timestamp_ms: 5000, crop: Some((10, 20, 800, 600)),
            caption: Some("dashboard"), source: "bookmark", confidence: Some(1.0), accepted: true,
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
        let input = ScreenshotInput { meeting_id: "m1", timestamp_ms: 0, source: "weird", ..Default::default() };
        let err = ScreenshotsRepository::insert(&pool, input).await.unwrap_err();
        match err { sqlx::Error::Protocol(_) => {}, other => panic!("got {:?}", other) }
    }
}
```

- [ ] **Step 2: Re-export, run, commit**

```bash
echo "pub mod screenshot;
pub use screenshot::{ScreenshotsRepository, ScreenshotInput};" >> frontend/src-tauri/src/database/repositories/mod.rs
cd frontend/src-tauri && cargo test --lib database::repositories::screenshot 2>&1 | tail -10
git add frontend/src-tauri/src/database/repositories/
git commit -m "feat(db): ScreenshotsRepository scaffold + tests"
```

---

## Task 7: `screen_recorder` types and module skeleton

**Files:**
- Create: `frontend/src-tauri/src/screen_recorder/mod.rs`
- Create: `frontend/src-tauri/src/screen_recorder/types.rs`
- Modify: `frontend/src-tauri/src/lib.rs` (add `pub mod screen_recorder;`)

- [ ] **Step 1: Write `mod.rs`**

```rust
pub mod commands;
pub mod types;

#[cfg(target_os = "macos")]
pub mod recorder;

#[cfg(target_os = "macos")]
pub use recorder::ScreenRecorder;
pub use types::{DisplayInfo, RecordingMeta, ScreenRecorderError};
```

- [ ] **Step 2: Write `types.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub id: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingMeta {
    pub file_path: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub codec: String,
    pub display_id: u32,
    pub duration_ms: u64,
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum ScreenRecorderError {
    #[error("permission denied for screen recording")]
    PermissionDenied,
    #[error("display not found: {0}")]
    DisplayNotFound(u32),
    #[error("already recording")]
    AlreadyRecording,
    #[error("not currently recording")]
    NotRecording,
    #[error("io error: {0}")]
    Io(String),
    #[error("internal error: {0}")]
    Internal(String),
}
```

- [ ] **Step 3: Stub `commands.rs`**

```rust
// Tauri commands; populated in Task 9.
```

- [ ] **Step 4: Add `pub mod screen_recorder;` near other module declarations in `lib.rs`**

- [ ] **Step 5: Verify**

```bash
cd frontend/src-tauri && cargo check 2>&1 | tail -10
```

- [ ] **Step 6: Commit**

```bash
git add frontend/src-tauri/src/screen_recorder/ frontend/src-tauri/src/lib.rs
git commit -m "feat(screen_recorder): module skeleton and shared types"
```

---

## Task 8: Implement `ScreenRecorder` using cidre + SCRecordingOutput

**Files:**
- Create: `frontend/src-tauri/src/screen_recorder/recorder.rs`

- [ ] **Step 1: Write the recorder**

```rust
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use cidre::{arc, cm, define_obj_type, ns, objc, sc};

use crate::screen_recorder::types::{DisplayInfo, RecordingMeta, ScreenRecorderError};

// SCRecordingOutputDelegate is required to construct an SCRecordingOutput.
// We don't currently surface its callbacks, but we need a real obj-c object.
#[repr(C)]
struct DelegateInner;

define_obj_type!(SilentDelegate + sc::recording_output::DelegateImpl, DelegateInner, SILENT_DELEGATE);

impl sc::recording_output::Delegate for SilentDelegate {}

#[objc::add_methods]
impl sc::recording_output::DelegateImpl for SilentDelegate {}

struct ActiveSession {
    display_id: u32,
    output_path: PathBuf,
    fps: u32,
    started_at: Instant,
    width: u32,
    height: u32,
    stream: arc::R<sc::Stream>,
    output: arc::R<sc::RecordingOutput>,
    _delegate: arc::R<SilentDelegate>,
}

pub struct ScreenRecorder {
    active: Mutex<Option<ActiveSession>>,
}

impl ScreenRecorder {
    pub fn new() -> Self {
        Self { active: Mutex::new(None) }
    }

    pub fn is_recording(&self) -> bool {
        self.active.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    pub fn list_displays(&self) -> Result<Vec<DisplayInfo>, ScreenRecorderError> {
        list_displays()
    }

    pub fn start(
        &self,
        display_id: u32,
        output_path: &Path,
        fps: u32,
        _bitrate_kbps: u32,
    ) -> Result<(), ScreenRecorderError> {
        let mut guard = self.active.lock().map_err(|_| ScreenRecorderError::Internal("mutex poisoned".into()))?;
        if guard.is_some() {
            return Err(ScreenRecorderError::AlreadyRecording);
        }

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ScreenRecorderError::Io(e.to_string()))?;
        }
        let _ = std::fs::remove_file(output_path);

        // Block on async ScreenCaptureKit operations.
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let content = sc::ShareableContent::current()
                    .await
                    .map_err(|e| ScreenRecorderError::Internal(format!("shareable content: {:?}", e)))?;
                let displays = content.displays();
                let display = displays
                    .iter()
                    .find(|d| d.display_id() == display_id)
                    .ok_or(ScreenRecorderError::DisplayNotFound(display_id))?;

                let width = display.width() as u32;
                let height = display.height() as u32;

                let mut cfg = sc::StreamCfg::new();
                cfg.set_width(width as usize);
                cfg.set_height(height as usize);
                cfg.set_minimum_frame_interval(cm::Time::new(1, fps as i32));
                cfg.set_captures_audio(true);
                cfg.set_excludes_current_process_audio(true);
                cfg.set_shows_cursor(true);

                let windows = ns::Array::new();
                let filter = sc::ContentFilter::with_display_excluding_windows(display, &windows);
                let stream = sc::Stream::new(&filter, &cfg);

                let url = ns::Url::with_fs_path_str(
                    output_path.to_str().ok_or_else(|| ScreenRecorderError::Io("non-UTF8 path".into()))?,
                    false,
                );
                let mut output_cfg = sc::RecordingOutputCfg::new()
                    .ok_or_else(|| ScreenRecorderError::Internal("RecordingOutputCfg::new failed (need macOS 15)".into()))?;
                output_cfg.set_output_url(&url);

                let delegate = SilentDelegate::with(DelegateInner);
                let output = sc::RecordingOutput::with_cfg(&output_cfg, delegate.as_ref());

                stream
                    .add_recording_output(&output)
                    .map_err(|e| ScreenRecorderError::Internal(format!("add_recording_output: {:?}", e)))?;

                stream
                    .start()
                    .await
                    .map_err(|e| ScreenRecorderError::Internal(format!("stream start: {:?}", e)))?;

                Ok::<_, ScreenRecorderError>(ActiveSession {
                    display_id,
                    output_path: output_path.to_path_buf(),
                    fps,
                    started_at: Instant::now(),
                    width,
                    height,
                    stream,
                    output,
                    _delegate: delegate,
                })
            })
        })?;

        *guard = Some(result);
        Ok(())
    }

    pub fn stop(&self) -> Result<RecordingMeta, ScreenRecorderError> {
        let mut guard = self.active.lock().map_err(|_| ScreenRecorderError::Internal("mutex poisoned".into()))?;
        let session = guard.take().ok_or(ScreenRecorderError::NotRecording)?;

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let _ = session.stream.stop().await;
                let _ = session.stream.remove_recording_output(&session.output);
            });
        });

        Ok(RecordingMeta {
            file_path: session.output_path.to_string_lossy().to_string(),
            width: session.width,
            height: session.height,
            fps: session.fps,
            codec: "h264".to_string(),
            display_id: session.display_id,
            duration_ms: session.started_at.elapsed().as_millis() as u64,
        })
    }
}

impl Default for ScreenRecorder {
    fn default() -> Self {
        Self::new()
    }
}

pub fn list_displays() -> Result<Vec<DisplayInfo>, ScreenRecorderError> {
    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let content = sc::ShareableContent::current()
                .await
                .map_err(|e| ScreenRecorderError::Internal(format!("shareable content: {:?}", e)))?;
            let primary_id = unsafe { core_graphics::display::CGMainDisplayID() };
            let mut out = Vec::new();
            for d in content.displays().iter() {
                out.push(DisplayInfo {
                    id: d.display_id(),
                    name: format!("Display {}", d.display_id()),
                    width: d.width() as u32,
                    height: d.height() as u32,
                    is_primary: d.display_id() == primary_id,
                });
            }
            Ok::<_, ScreenRecorderError>(out)
        })
    });
    result
}
```

Note: cidre's exact API surface (`add_recording_output`, `remove_recording_output`, `stream.stop()`, etc.) may need minor name adjustments. If a method isn't found, search `~/.cargo/git/checkouts/cidre-*/src/sc/` for the exact name.

- [ ] **Step 2: Compile**

```bash
cd frontend/src-tauri && cargo check 2>&1 | tail -20
```

If compile errors come from cidre API drift, look up actual symbols in the cidre source:
```bash
grep -rn "add_recording_output\|remove_recording_output\|fn stop" ~/.cargo/git/checkouts/cidre-*/cidre/src/sc/ | head
```
Adjust call sites accordingly.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/screen_recorder/recorder.rs
git commit -m "feat(screen_recorder): cidre-based recorder with SCRecordingOutput"
```

---

## Task 9: Tauri commands

**Files:**
- Modify: `frontend/src-tauri/src/screen_recorder/commands.rs`
- Modify: `frontend/src-tauri/src/lib.rs`

The project's existing `AppState` lives in `crate::state` and holds `db_manager: DatabaseManager`. Commands access the pool via `state.db_manager.pool()`.

- [ ] **Step 1: Write commands.rs (with non-macOS stub)**

```rust
#[cfg(target_os = "macos")]
mod imp {
    use std::path::PathBuf;
    use std::sync::Arc;
    use tauri::State;

    use crate::database::repositories::RecordingsRepository;
    use crate::screen_recorder::types::{DisplayInfo, RecordingMeta, ScreenRecorderError};
    use crate::screen_recorder::ScreenRecorder;
    use crate::state::AppState;

    pub struct ScreenRecorderState {
        pub recorder: Arc<ScreenRecorder>,
        pub current_recording_id: tokio::sync::Mutex<Option<String>>,
    }

    impl ScreenRecorderState {
        pub fn new() -> Self {
            Self {
                recorder: Arc::new(ScreenRecorder::new()),
                current_recording_id: tokio::sync::Mutex::new(None),
            }
        }
    }

    fn recordings_dir() -> Result<PathBuf, ScreenRecorderError> {
        let base = dirs::data_local_dir()
            .ok_or_else(|| ScreenRecorderError::Io("no data dir".into()))?;
        Ok(base.join("Meetily").join("recordings"))
    }

    #[tauri::command]
    pub async fn screen_list_displays() -> Result<Vec<DisplayInfo>, ScreenRecorderError> {
        crate::screen_recorder::recorder::list_displays()
    }

    #[tauri::command]
    pub async fn screen_is_recording(state: State<'_, ScreenRecorderState>) -> Result<bool, ScreenRecorderError> {
        Ok(state.recorder.is_recording())
    }

    #[tauri::command]
    pub async fn screen_start_recording(
        meeting_id: String,
        display_id: u32,
        fps: Option<u32>,
        bitrate_kbps: Option<u32>,
        state: State<'_, ScreenRecorderState>,
        app_state: State<'_, AppState>,
    ) -> Result<String, ScreenRecorderError> {
        let dir = recordings_dir()?;
        std::fs::create_dir_all(&dir).map_err(|e| ScreenRecorderError::Io(e.to_string()))?;
        let filename = format!("{}-{}.mp4", meeting_id, chrono::Utc::now().timestamp_millis());
        let path = dir.join(filename);

        let pool = app_state.db_manager.pool();
        let row = RecordingsRepository::create(pool, &meeting_id, &path.to_string_lossy(), Some(display_id as i64))
            .await
            .map_err(|e| ScreenRecorderError::Internal(format!("db: {}", e)))?;

        state
            .recorder
            .start(display_id, &path, fps.unwrap_or(30), bitrate_kbps.unwrap_or(3000))?;

        let mut cur = state.current_recording_id.lock().await;
        *cur = Some(row.id.clone());
        Ok(row.id)
    }

    #[tauri::command]
    pub async fn screen_stop_recording(
        state: State<'_, ScreenRecorderState>,
        app_state: State<'_, AppState>,
    ) -> Result<RecordingMeta, ScreenRecorderError> {
        let meta = state.recorder.stop()?;
        let mut cur = state.current_recording_id.lock().await;
        if let Some(id) = cur.take() {
            let pool = app_state.db_manager.pool();
            let _ = RecordingsRepository::finalize(
                pool, &id, chrono::Utc::now().timestamp_millis(),
                Some(meta.width as i64), Some(meta.height as i64),
                Some(meta.fps as i64), Some(&meta.codec),
            ).await;
        }
        Ok(meta)
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use tauri::State;
    use crate::screen_recorder::types::{DisplayInfo, RecordingMeta, ScreenRecorderError};
    use crate::state::AppState;

    pub struct ScreenRecorderState;
    impl ScreenRecorderState { pub fn new() -> Self { Self } }

    #[tauri::command]
    pub async fn screen_list_displays() -> Result<Vec<DisplayInfo>, ScreenRecorderError> { Ok(vec![]) }
    #[tauri::command]
    pub async fn screen_is_recording(_s: State<'_, ScreenRecorderState>) -> Result<bool, ScreenRecorderError> { Ok(false) }
    #[tauri::command]
    pub async fn screen_start_recording(
        _meeting_id: String, _display_id: u32, _fps: Option<u32>, _bitrate_kbps: Option<u32>,
        _s: State<'_, ScreenRecorderState>, _a: State<'_, AppState>,
    ) -> Result<String, ScreenRecorderError> {
        Err(ScreenRecorderError::Internal("not supported on this platform".into()))
    }
    #[tauri::command]
    pub async fn screen_stop_recording(
        _s: State<'_, ScreenRecorderState>, _a: State<'_, AppState>,
    ) -> Result<RecordingMeta, ScreenRecorderError> {
        Err(ScreenRecorderError::Internal("not supported on this platform".into()))
    }
}

pub use imp::*;
```

- [ ] **Step 2: Register state in lib.rs**

In `lib.rs`, find the fluent `tauri::Builder::default()` chain (around line 393) and add to the existing `.manage(...)` calls:
```rust
        .manage(screen_recorder::commands::ScreenRecorderState::new())
```

- [ ] **Step 3: Add the four commands to `tauri::generate_handler![...]`**

```rust
            screen_recorder::commands::screen_list_displays,
            screen_recorder::commands::screen_is_recording,
            screen_recorder::commands::screen_start_recording,
            screen_recorder::commands::screen_stop_recording,
```

- [ ] **Step 4: Verify and commit**

```bash
cd frontend/src-tauri && cargo build 2>&1 | tail -20
git add frontend/src-tauri/src/screen_recorder/commands.rs frontend/src-tauri/src/lib.rs
git commit -m "feat(screen_recorder): Tauri commands wired into AppState"
```

---

## Task 10: End-to-end smoke test

**Files:**
- Create: `frontend/src-tauri/tests/screen_recorder_smoke.rs`

- [ ] **Step 1: Write the test (manual, ignored by default)**

```rust
#[cfg(target_os = "macos")]
#[test]
#[ignore] // requires Screen Recording permission; run with: cargo test --test screen_recorder_smoke -- --ignored
fn records_2_seconds_to_a_file() {
    use std::time::Duration;
    use std::thread::sleep;
    use app_lib::screen_recorder::ScreenRecorder;

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let displays = app_lib::screen_recorder::recorder::list_displays().expect("list");
        assert!(!displays.is_empty(), "needs at least one display + screen recording permission");
        let primary = displays.iter().find(|d| d.is_primary).unwrap_or(&displays[0]);

        let tmp = std::env::temp_dir().join("meetily_screen_recorder_smoke.mp4");
        let _ = std::fs::remove_file(&tmp);
        let recorder = ScreenRecorder::new();
        recorder.start(primary.id, &tmp, 30, 3000).expect("start");
        sleep(Duration::from_secs(2));
        let meta = recorder.stop().expect("stop");
        assert!(meta.duration_ms >= 1500, "duration too short: {}", meta.duration_ms);
        let size = std::fs::metadata(&tmp).expect("file exists").len();
        assert!(size > 100_000, "recording too small: {}", size);
    });
}
```

- [ ] **Step 2: Run with `--ignored`** (manual, requires permission)

```bash
cd frontend/src-tauri && cargo test --test screen_recorder_smoke -- --ignored --nocapture 2>&1 | tail -20
```
Expected: 2-second recording produces a valid mp4. If permission missing, grant and retry.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/tests/screen_recorder_smoke.rs
git commit -m "test(screen_recorder): end-to-end record-2s smoke (ignored)"
```

---

## Task 11: Frontend dev page

**Files:**
- Create: `frontend/src/app/dev/screen-recorder/page.tsx`

- [ ] **Step 1: Write the page**

```tsx
"use client";

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Display = { id: number; name: string; width: number; height: number; is_primary: boolean };
type RecordingMeta = { file_path: string; fps: number; codec: string; display_id: number; duration_ms: number };

export default function ScreenRecorderDevPage() {
  const [displays, setDisplays] = useState<Display[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [recording, setRecording] = useState(false);
  const [recordingId, setRecordingId] = useState<string | null>(null);
  const [meta, setMeta] = useState<RecordingMeta | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [meetingId] = useState<string>("dev-" + Date.now());

  useEffect(() => {
    invoke<Display[]>("screen_list_displays")
      .then((d) => {
        setDisplays(d);
        const primary = d.find((x) => x.is_primary) ?? d[0];
        if (primary) setSelected(primary.id);
      })
      .catch((e) => setError(String(e)));
    invoke<boolean>("screen_is_recording").then(setRecording);
  }, []);

  async function start() {
    setError(null); setMeta(null);
    if (selected == null) return;
    try {
      const id = await invoke<string>("screen_start_recording", {
        meetingId, displayId: selected, fps: 30, bitrateKbps: 3000,
      });
      setRecordingId(id); setRecording(true);
    } catch (e) { setError(String(e)); }
  }

  async function stop() {
    setError(null);
    try {
      const m = await invoke<RecordingMeta>("screen_stop_recording");
      setMeta(m); setRecording(false); setRecordingId(null);
    } catch (e) { setError(String(e)); }
  }

  return (
    <div style={{ padding: 24, fontFamily: "system-ui" }}>
      <h1>Screen Recorder Dev</h1>
      <p>Meeting ID: <code>{meetingId}</code></p>
      <h2>Displays</h2>
      <ul>
        {displays.map((d) => (
          <li key={d.id}>
            <label>
              <input type="radio" name="display" value={d.id} checked={selected === d.id} onChange={() => setSelected(d.id)} />
              {" "}{d.name} ({d.width}×{d.height}{d.is_primary ? ", primary" : ""})
            </label>
          </li>
        ))}
      </ul>
      <div style={{ marginTop: 16 }}>
        {!recording ? <button onClick={start} disabled={selected == null}>Start recording</button>
                    : <button onClick={stop}>Stop recording</button>}
      </div>
      {recordingId && <p>Recording ID: <code>{recordingId}</code></p>}
      {meta && <pre style={{ background: "#f4f4f4", padding: 12, marginTop: 16 }}>{JSON.stringify(meta, null, 2)}</pre>}
      {error && <pre style={{ color: "crimson", marginTop: 16 }}>{error}</pre>}
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/app/dev/screen-recorder/page.tsx
git commit -m "feat(frontend): dev page to exercise screen recorder"
```

---

## Task 12: Final cleanup

- [ ] **Step 1: Full test suite**

```bash
cd frontend/src-tauri && cargo test 2>&1 | tail -20
```
Expected: all `--lib` and non-ignored tests pass.

- [ ] **Step 2: Manual smoke**

```bash
cd frontend/src-tauri && cargo test --test screen_recorder_smoke -- --ignored --nocapture 2>&1 | tail -20
```
Expected: 2-second recording, ≥100 KB mp4.

- [ ] **Step 3: Clippy**

```bash
cd frontend/src-tauri && cargo clippy --all-targets -- -D warnings 2>&1 | tail -20
```
Fix warnings introduced on this branch.

- [ ] **Step 4: Commit cleanup**

```bash
git add -u && git commit -m "chore: clippy + cleanup after Phase 1A" || true
```

---

## Phase 1A complete

- ScreenCaptureKit recorder via cidre, accessible from Rust.
- Four Tauri commands: `screen_list_displays`, `screen_is_recording`, `screen_start_recording`, `screen_stop_recording`.
- Three new tables (`meeting_recordings`, `meeting_bookmarks`, `meeting_screenshots`) and repositories for each.
- Dev page at `/dev/screen-recorder` to exercise the stack manually.

**Deferred to later phases:**
- Microphone audio in the mp4 (system audio only in Phase 1A).
- Permission UX (empty array on missing permission; nicer prompt in Phase 1B).
- Auto-start when meeting starts (dev page makes up its own `meeting_id`).

**Next phases (separate plans):**
- **Phase 1B** — Bookmarks + global hotkey + local HTTP API + permission UX + mic muxing.
- **Phase 2** — Screenshot pipeline + Scribe-style review UI + embedding.
- **Phase 3** — Cloud vision opt-in + StreamDeck/Companion docs.

---

## Notes for the implementer

- cidre's exact API surface evolves; if a method like `add_recording_output` isn't found by that name, grep the cidre checkout (`~/.cargo/git/checkouts/cidre-*`) for the actual symbol and adjust.
- `tokio::task::block_in_place` is required because cidre's async APIs need a Tokio runtime, but Tauri commands aren't always called from one. If `block_in_place` panics ("can be called only from within a multi-threaded runtime"), wrap calls in `tokio::runtime::Runtime::new().unwrap().block_on(...)` instead.
- The dev page is intentionally minimal; production UX is Phase 1B.
- If cidre's `sc::RecordingOutput` is missing from a feature combination, check that the Cargo features include `["av", "sc", "cm", "vt", "macos_15_0"]`.
