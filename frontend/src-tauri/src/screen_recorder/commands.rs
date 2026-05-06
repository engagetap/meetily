#[cfg(target_os = "macos")]
mod imp {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Instant;

    use tauri::State;

    use crate::database::repositories::RecordingsRepository;
    use crate::screen_recorder::bookmark::{drop_bookmark, BookmarkError, BookmarkSource};
    use crate::screen_recorder::types::{DisplayInfo, RecordingMeta, ScreenRecorderError};
    use crate::screen_recorder::ScreenRecorder;
    use crate::state::AppState;

    /// Information about the currently-active recording, used by the
    /// bookmark + status flows. Held inside `ScreenRecorderState` and
    /// populated/cleared on start/stop.
    #[derive(Debug, Clone)]
    pub struct ActiveRecordingInfo {
        pub recording_id: String,
        pub meeting_id: String,
        pub started_at: Instant,
    }

    pub struct ScreenRecorderState {
        pub recorder: Arc<ScreenRecorder>,
        pub active: tokio::sync::Mutex<Option<ActiveRecordingInfo>>,
    }

    impl ScreenRecorderState {
        pub fn new() -> Self {
            Self {
                recorder: Arc::new(ScreenRecorder::new()),
                active: tokio::sync::Mutex::new(None),
            }
        }

        /// Snapshot of the active recording, if any. Used by the local HTTP
        /// API status endpoint and the bookmark flow.
        pub async fn snapshot(&self) -> Option<ActiveRecordingInfo> {
            self.active.lock().await.clone()
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

    /// Returns true if the app currently has Screen Recording permission.
    /// Implemented by trying to enumerate displays — ScreenCaptureKit
    /// returns an empty array when permission is missing.
    #[tauri::command]
    pub async fn screen_has_permission() -> Result<bool, ScreenRecorderError> {
        match crate::screen_recorder::recorder::list_displays() {
            Ok(displays) => Ok(!displays.is_empty()),
            Err(_) => Ok(false),
        }
    }

    /// Opens macOS Privacy & Security → Screen & System Audio Recording so
    /// the user can grant the app permission. After they grant it, the
    /// app must be restarted for the new permission to take effect (this
    /// is a macOS limitation, not ours).
    #[tauri::command]
    pub async fn screen_open_permission_settings() -> Result<(), ScreenRecorderError> {
        let url = "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| ScreenRecorderError::Io(format!("open settings: {e}")))?;
        Ok(())
    }

    #[tauri::command]
    pub async fn screen_is_recording(
        state: State<'_, ScreenRecorderState>,
    ) -> Result<bool, ScreenRecorderError> {
        Ok(state.recorder.is_recording())
    }

    #[tauri::command]
    pub async fn screen_start_recording(
        meeting_id: String,
        display_id: u32,
        fps: Option<u32>,
        bitrate_kbps: Option<u32>,
        capture_mic: Option<bool>,
        state: State<'_, ScreenRecorderState>,
        app_state: State<'_, AppState>,
    ) -> Result<String, ScreenRecorderError> {
        let dir = recordings_dir()?;
        std::fs::create_dir_all(&dir).map_err(|e| ScreenRecorderError::Io(e.to_string()))?;
        let filename = format!(
            "{}-{}.mp4",
            meeting_id,
            chrono::Utc::now().timestamp_millis()
        );
        let path = dir.join(filename);

        let pool = app_state.db_manager.pool();
        let row = RecordingsRepository::create(
            pool,
            &meeting_id,
            &path.to_string_lossy(),
            Some(display_id as i64),
        )
        .await
        .map_err(|e| ScreenRecorderError::Internal(format!("db: {}", e)))?;

        state.recorder.start(
            display_id,
            &path,
            fps.unwrap_or(30),
            bitrate_kbps.unwrap_or(3000),
            capture_mic.unwrap_or(false),
        )?;

        let mut cur = state.active.lock().await;
        *cur = Some(ActiveRecordingInfo {
            recording_id: row.id.clone(),
            meeting_id: meeting_id.clone(),
            started_at: Instant::now(),
        });
        Ok(row.id)
    }

    #[tauri::command]
    pub async fn screen_stop_recording(
        state: State<'_, ScreenRecorderState>,
        app_state: State<'_, AppState>,
    ) -> Result<RecordingMeta, ScreenRecorderError> {
        let meta = state.recorder.stop()?;

        let mut cur = state.active.lock().await;
        if let Some(info) = cur.take() {
            let pool = app_state.db_manager.pool();
            let _ = RecordingsRepository::finalize(
                pool,
                &info.recording_id,
                chrono::Utc::now().timestamp_millis(),
                Some(meta.width as i64),
                Some(meta.height as i64),
                Some(meta.fps as i64),
                Some(&meta.codec),
            )
            .await;
        }
        Ok(meta)
    }

    /// Drops a bookmark at the current recording's elapsed offset.
    ///
    /// `source` must be one of "hotkey" / "api" / "ui". Returns the new
    /// bookmark's id and timestamp_ms (relative to the recording start).
    #[tauri::command]
    pub async fn bookmark_now(
        label: Option<String>,
        source: String,
        state: State<'_, ScreenRecorderState>,
        app_state: State<'_, AppState>,
    ) -> Result<BookmarkResult, BookmarkError> {
        let parsed_source = match source.as_str() {
            "hotkey" => BookmarkSource::Hotkey,
            "api" => BookmarkSource::Api,
            "ui" => BookmarkSource::Ui,
            other => {
                return Err(BookmarkError::Db(format!("invalid source: {}", other)));
            }
        };

        let info = state
            .snapshot()
            .await
            .ok_or(BookmarkError::NotRecording)?;

        let pool = app_state.db_manager.pool();
        let dropped = drop_bookmark(
            pool,
            &info.meeting_id,
            info.started_at,
            label.as_deref(),
            parsed_source,
        )
        .await?;
        Ok(BookmarkResult {
            id: dropped.id,
            meeting_id: dropped.meeting_id,
            timestamp_ms: dropped.timestamp_ms,
        })
    }

    #[derive(Debug, Clone, serde::Serialize)]
    pub struct BookmarkResult {
        pub id: String,
        pub meeting_id: String,
        pub timestamp_ms: i64,
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use tauri::State;

    use crate::screen_recorder::bookmark::BookmarkError;
    use crate::screen_recorder::types::{DisplayInfo, RecordingMeta, ScreenRecorderError};
    use crate::state::AppState;

    pub struct ScreenRecorderState;
    impl ScreenRecorderState {
        pub fn new() -> Self {
            Self
        }
        pub async fn snapshot(&self) -> Option<()> {
            None
        }
    }

    #[derive(Debug, Clone, serde::Serialize)]
    pub struct BookmarkResult {
        pub id: String,
        pub meeting_id: String,
        pub timestamp_ms: i64,
    }

    #[tauri::command]
    pub async fn screen_list_displays() -> Result<Vec<DisplayInfo>, ScreenRecorderError> {
        Ok(vec![])
    }
    #[tauri::command]
    pub async fn screen_has_permission() -> Result<bool, ScreenRecorderError> {
        Ok(false)
    }
    #[tauri::command]
    pub async fn screen_open_permission_settings() -> Result<(), ScreenRecorderError> {
        Err(ScreenRecorderError::Internal("not supported on this platform".into()))
    }
    #[tauri::command]
    pub async fn screen_is_recording(
        _s: State<'_, ScreenRecorderState>,
    ) -> Result<bool, ScreenRecorderError> {
        Ok(false)
    }
    #[tauri::command]
    pub async fn screen_start_recording(
        _meeting_id: String,
        _display_id: u32,
        _fps: Option<u32>,
        _bitrate_kbps: Option<u32>,
        _capture_mic: Option<bool>,
        _s: State<'_, ScreenRecorderState>,
        _a: State<'_, AppState>,
    ) -> Result<String, ScreenRecorderError> {
        Err(ScreenRecorderError::Internal(
            "not supported on this platform".into(),
        ))
    }
    #[tauri::command]
    pub async fn screen_stop_recording(
        _s: State<'_, ScreenRecorderState>,
        _a: State<'_, AppState>,
    ) -> Result<RecordingMeta, ScreenRecorderError> {
        Err(ScreenRecorderError::Internal(
            "not supported on this platform".into(),
        ))
    }
    #[tauri::command]
    pub async fn bookmark_now(
        _label: Option<String>,
        _source: String,
        _s: State<'_, ScreenRecorderState>,
        _a: State<'_, AppState>,
    ) -> Result<BookmarkResult, BookmarkError> {
        Err(BookmarkError::NotRecording)
    }
}

pub use imp::*;
