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
                pool,
                &id,
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
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use tauri::State;

    use crate::screen_recorder::types::{DisplayInfo, RecordingMeta, ScreenRecorderError};
    use crate::state::AppState;

    pub struct ScreenRecorderState;
    impl ScreenRecorderState {
        pub fn new() -> Self {
            Self
        }
    }

    #[tauri::command]
    pub async fn screen_list_displays() -> Result<Vec<DisplayInfo>, ScreenRecorderError> {
        Ok(vec![])
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
}

pub use imp::*;
