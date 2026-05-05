use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use cidre::{arc, cm, define_obj_type, ns, objc, sc};

use crate::screen_recorder::types::{DisplayInfo, RecordingMeta, ScreenRecorderError};

// Minimal SCRecordingOutputDelegate that swallows all callbacks. The protocol's three methods
// (start / fail / finish) are all `#[objc::optional]`, so an empty implementation is valid.
#[repr(C)]
struct SilentDelegateInner;

define_obj_type!(
    SilentDelegate + sc::RecordingOutputDelegateImpl,
    SilentDelegateInner,
    MEETILY_SILENT_DELEGATE_CLS
);

impl sc::RecordingOutputDelegate for SilentDelegate {}

#[objc::add_methods]
impl sc::RecordingOutputDelegateImpl for SilentDelegate {}

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
        Self {
            active: Mutex::new(None),
        }
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
        let mut guard = self
            .active
            .lock()
            .map_err(|_| ScreenRecorderError::Internal("mutex poisoned".into()))?;
        if guard.is_some() {
            return Err(ScreenRecorderError::AlreadyRecording);
        }

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ScreenRecorderError::Io(e.to_string()))?;
        }
        let _ = std::fs::remove_file(output_path);

        let path_str = output_path
            .to_str()
            .ok_or_else(|| ScreenRecorderError::Io("non-UTF8 path".into()))?
            .to_string();

        let session = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let content = sc::ShareableContent::current()
                    .await
                    .map_err(|e| ScreenRecorderError::Internal(format!("shareable content: {:?}", e)))?;

                let displays = content.displays();
                let display = displays
                    .iter()
                    .find(|d| d.display_id().0 == display_id)
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
                let filter = sc::ContentFilter::with_display_excluding_windows(&display, &windows);
                let mut stream = sc::Stream::new(&filter, &cfg);

                let url = ns::Url::with_fs_path_str(&path_str, false);
                let mut output_cfg = sc::RecordingOutputCfg::new();
                output_cfg.set_output_url(&url);

                let delegate = SilentDelegate::with(SilentDelegateInner);
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
                    output_path: PathBuf::from(&path_str),
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

        *guard = Some(session);
        Ok(())
    }

    pub fn stop(&self) -> Result<RecordingMeta, ScreenRecorderError> {
        let mut guard = self
            .active
            .lock()
            .map_err(|_| ScreenRecorderError::Internal("mutex poisoned".into()))?;
        let session = guard.take().ok_or(ScreenRecorderError::NotRecording)?;

        let mut session = session;
        let stream_for_async = session.stream.clone();
        let output_for_async = session.output.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let _ = stream_for_async.stop().await;
            });
        });
        let _ = session.stream.remove_recording_output(&output_for_async);

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
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let content = sc::ShareableContent::current()
                .await
                .map_err(|e| ScreenRecorderError::Internal(format!("shareable content: {:?}", e)))?;
            let primary_id = unsafe { core_graphics::display::CGMainDisplayID() };
            let mut out = Vec::new();
            for d in content.displays().iter() {
                let id_u32: u32 = d.display_id().0;
                out.push(DisplayInfo {
                    id: id_u32,
                    name: format!("Display {}", id_u32),
                    width: d.width() as u32,
                    height: d.height() as u32,
                    is_primary: id_u32 == primary_id,
                });
            }
            Ok::<_, ScreenRecorderError>(out)
        })
    })
}
