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
    /// The meeting_id this recording was bound to. Set on stop. Optional
    /// for backward compatibility with older serializations.
    #[serde(default)]
    pub meeting_id: Option<String>,
}

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "type", content = "message")]
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
