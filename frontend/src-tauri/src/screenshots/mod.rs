//! Phase 2: post-recording screenshot pipeline + review surface.
//!
//! - `extractor`: extract a PNG frame from a recorded mp4 at a given
//!   `(timestamp_ms, optional crop_rect)` using the bundled ffmpeg binary.
//! - `picker`: produce candidate `meeting_screenshots` rows from a meeting's
//!   bookmarks (Phase 2 first pass — transcript cues + frame-diff land later).
//! - `commands`: Tauri commands driving the review UI.

pub mod commands;
pub mod extractor;
pub mod picker;

pub use extractor::{extract_frame, CropRect, ExtractError};
pub use picker::{generate_candidates_from_bookmarks, PickerError};
