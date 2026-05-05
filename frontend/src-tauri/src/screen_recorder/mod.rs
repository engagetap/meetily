pub mod commands;
pub mod types;

#[cfg(target_os = "macos")]
pub mod recorder;

#[cfg(target_os = "macos")]
pub use recorder::ScreenRecorder;
pub use types::{DisplayInfo, RecordingMeta, ScreenRecorderError};
