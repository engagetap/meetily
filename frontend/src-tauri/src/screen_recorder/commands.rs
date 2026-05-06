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

    /// Returns the user-configured recordings folder (the same one the
    /// audio side writes to, configurable via Settings → Preferences →
    /// "Recordings folder"). Falls back to the OS default if the prefs
    /// can't be read for any reason.
    async fn recordings_dir<R: tauri::Runtime>(
        app: &tauri::AppHandle<R>,
    ) -> Result<PathBuf, ScreenRecorderError> {
        match crate::audio::recording_preferences::load_recording_preferences(app).await {
            Ok(prefs) if prefs.save_folder.as_os_str().len() > 0 => Ok(prefs.save_folder),
            _ => {
                let base = dirs::data_local_dir()
                    .ok_or_else(|| ScreenRecorderError::Io("no data dir".into()))?;
                Ok(base.join("Meetily").join("recordings"))
            }
        }
    }

    #[tauri::command]
    pub async fn screen_list_displays() -> Result<Vec<DisplayInfo>, ScreenRecorderError> {
        let displays = crate::screen_recorder::recorder::list_displays();
        match &displays {
            Ok(d) => log::info!("screen_list_displays: returned {} display(s)", d.len()),
            Err(e) => log::warn!("screen_list_displays: error: {:?}", e),
        }
        displays
    }

    // Apple's documented APIs for screen-capture permission. They live in
    // CoreGraphics, must run on the main thread, and are the only reliable
    // way to trigger macOS's TCC prompt — calling `SCShareableContent`
    // from a Tokio worker doesn't surface the dialog.
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    /// Returns true if the app currently has Screen Recording permission.
    /// Uses CGPreflightScreenCaptureAccess — the documented API. Doesn't
    /// trigger the prompt; safe to call as often as you like.
    #[tauri::command]
    pub async fn screen_has_permission() -> Result<bool, ScreenRecorderError> {
        let granted = unsafe { CGPreflightScreenCaptureAccess() };
        Ok(granted)
    }

    /// Triggers macOS's Screen Recording permission prompt if not already
    /// granted. Returns the post-prompt grant state. The first call shows
    /// the dialog; subsequent calls are no-ops (true if granted, false
    /// otherwise — user must toggle in System Settings to flip from
    /// denied to allowed).
    ///
    /// Mirrors how the audio path uses `trigger_microphone_permission`.
    #[tauri::command]
    pub async fn screen_request_permission() -> Result<bool, ScreenRecorderError> {
        log::info!("screen_request_permission: invoking CGRequestScreenCaptureAccess");
        // The Apple API must run on the main thread. tauri::async_runtime
        // dispatches to a worker by default, so we hop back via
        // tauri::async_runtime::spawn_blocking + a main-thread handoff.
        let granted = tokio::task::spawn_blocking(|| unsafe {
            CGRequestScreenCaptureAccess()
        })
        .await
        .map_err(|e| ScreenRecorderError::Internal(format!("join: {e}")))?;
        log::info!("screen_request_permission: granted={}", granted);
        Ok(granted)
    }

    /// Snaps a thumbnail of the given display using cidre's
    /// `SCScreenshotManager.captureImage` — runs in-process so it
    /// inherits Meetily's TCC permission. Encodes to PNG via
    /// `CGImageDestination` and returns a `data:image/png;base64,...`
    /// URL. Falls back to the generic monitor icon on error.
    #[tauri::command]
    pub async fn screen_capture_thumbnail(
        display_id: u32,
        max_width: Option<u32>,
    ) -> Result<String, ScreenRecorderError> {
        // The cidre objects involved (cf::DataMut, cg::Image, sc::*) aren't
        // `Send`, so we can't `.await` across them inside an async fn — the
        // Tauri command future would lose Send. Run the whole thing inside
        // block_in_place so the borrow stays on a single thread.
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                capture_thumbnail_inner(display_id, max_width).await
            })
        })
    }

    async fn capture_thumbnail_inner(
        display_id: u32,
        max_width: Option<u32>,
    ) -> Result<String, ScreenRecorderError> {
        use base64::Engine as _;
        use cidre::{cf, cg, cm, ns, sc};

        let content = sc::ShareableContent::current()
            .await
            .map_err(|e| ScreenRecorderError::Internal(format!("shareable content: {:?}", e)))?;
        let displays = content.displays();
        let display = displays
            .iter()
            .find(|d| d.display_id().0 == display_id)
            .ok_or(ScreenRecorderError::DisplayNotFound(display_id))?;

        let windows = ns::Array::new();
        let filter = sc::ContentFilter::with_display_excluding_windows(&display, &windows);
        let mut cfg = sc::StreamCfg::new();
        let target = max_width.unwrap_or(640) as usize;
        let dw = display.width() as usize;
        let dh = display.height() as usize;
        let (out_w, out_h) = if dw <= target {
            (dw, dh)
        } else {
            let scale = target as f64 / dw as f64;
            ((dw as f64 * scale) as usize, (dh as f64 * scale) as usize)
        };
        cfg.set_width(out_w);
        cfg.set_height(out_h);
        cfg.set_minimum_frame_interval(cm::Time::new(1, 60));
        cfg.set_captures_audio(false);
        cfg.set_shows_cursor(true);

        let cg_image = sc::ScreenshotManager::capture_image(&filter, &cfg)
            .await
            .map_err(|e| ScreenRecorderError::Internal(format!("capture: {:?}", e)))?;

        let mut data = cf::DataMut::with_capacity(0);
        let png_uti = cf::String::from_str("public.png");
        let mut dst = cg::ImageDst::with_data(&mut data, &png_uti, 1)
            .ok_or_else(|| ScreenRecorderError::Internal("ImageDst create failed".into()))?;
        dst.add_image(&cg_image, None);
        if !dst.finalize() {
            return Err(ScreenRecorderError::Internal(
                "ImageDst finalize failed".into(),
            ));
        }

        let bytes: &[u8] = data.as_slice();
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        Ok(format!("data:image/png;base64,{}", b64))
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
    pub async fn screen_start_recording<R: tauri::Runtime>(
        app: tauri::AppHandle<R>,
        meeting_id: String,
        display_id: u32,
        fps: Option<u32>,
        bitrate_kbps: Option<u32>,
        capture_mic: Option<bool>,
        state: State<'_, ScreenRecorderState>,
        app_state: State<'_, AppState>,
    ) -> Result<String, ScreenRecorderError> {
        log::info!(
            "screen_start_recording: meeting_id={} display_id={} fps={:?} mic={:?}",
            meeting_id, display_id, fps, capture_mic
        );
        let dir = recordings_dir(&app).await?;
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

        match state.recorder.start(
            display_id,
            &path,
            fps.unwrap_or(30),
            bitrate_kbps.unwrap_or(3000),
            capture_mic.unwrap_or(false),
        ) {
            Ok(()) => log::info!("screen_start_recording: started → {}", path.display()),
            Err(e) => {
                log::error!("screen_start_recording: failed to start: {:?}", e);
                return Err(e);
            }
        }

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
        let mut meta = state.recorder.stop()?;

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
            // Surface the screen recording's meeting_id so the frontend can
            // immediately auto-generate screenshot candidates without having
            // to round-trip through the timestamp-proximity resolver.
            meta.meeting_id = Some(info.meeting_id);
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
    pub async fn screen_request_permission() -> Result<bool, ScreenRecorderError> {
        Ok(false)
    }
    #[tauri::command]
    pub async fn screen_open_permission_settings() -> Result<(), ScreenRecorderError> {
        Err(ScreenRecorderError::Internal("not supported on this platform".into()))
    }
    #[tauri::command]
    pub async fn screen_capture_thumbnail(
        _display_id: u32,
        _max_width: Option<u32>,
    ) -> Result<String, ScreenRecorderError> {
        Err(ScreenRecorderError::Internal("not supported on this platform".into()))
    }
    #[tauri::command]
    pub async fn screen_is_recording(
        _s: State<'_, ScreenRecorderState>,
    ) -> Result<bool, ScreenRecorderError> {
        Ok(false)
    }
    #[tauri::command]
    pub async fn screen_start_recording<R: tauri::Runtime>(
        _app: tauri::AppHandle<R>,
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
