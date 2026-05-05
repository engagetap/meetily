pub mod bookmark;
pub mod commands;
pub mod types;

#[cfg(target_os = "macos")]
pub mod recorder;

pub use bookmark::{drop_bookmark, BookmarkError, BookmarkSource, DroppedBookmark};
#[cfg(target_os = "macos")]
pub use recorder::ScreenRecorder;
pub use types::{DisplayInfo, RecordingMeta, ScreenRecorderError};
