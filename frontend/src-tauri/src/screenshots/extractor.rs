use std::path::Path;
use std::process::Command;

use crate::audio::ffmpeg::find_ffmpeg_path;

/// Pixel-space crop rectangle. Coordinates are in the source video's full
/// resolution. Out-of-bounds values are clamped by ffmpeg's `crop` filter.
#[derive(Debug, Clone, Copy)]
pub struct CropRect {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("ffmpeg binary not found")]
    FfmpegMissing,
    #[error("video not found: {0}")]
    VideoMissing(String),
    #[error("ffmpeg failed: {0}")]
    FfmpegFailed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Extracts a single PNG frame from `video_path` at `timestamp_ms` and
/// returns the image bytes. If `crop` is `Some`, applies a pixel-space crop
/// before encoding.
///
/// Uses ffmpeg's `-ss <seconds> -i <input>` pattern — placing `-ss` before
/// `-i` enables fast keyframe seeking, which is what we want for scrubbing
/// (the few-frames-of-imprecision is acceptable for screenshot anchoring).
pub fn extract_frame(
    video_path: &Path,
    timestamp_ms: i64,
    crop: Option<CropRect>,
) -> Result<Vec<u8>, ExtractError> {
    if !video_path.exists() {
        return Err(ExtractError::VideoMissing(
            video_path.to_string_lossy().to_string(),
        ));
    }

    let ffmpeg = find_ffmpeg_path().ok_or(ExtractError::FfmpegMissing)?;
    let seconds = timestamp_ms as f64 / 1000.0;

    // Use a temp file for the PNG output rather than piping bytes — robust
    // to ffmpeg's progress chatter on stderr/stdout, and the temp churn is
    // negligible for the size of one frame.
    let tmp = tempfile::Builder::new()
        .prefix("meetily_frame_")
        .suffix(".png")
        .tempfile()?;
    let tmp_path = tmp.path().to_path_buf();
    drop(tmp); // we want the path; ffmpeg will create the file

    let mut cmd = Command::new(&ffmpeg);
    cmd.arg("-y")
        .arg("-ss")
        .arg(format!("{:.3}", seconds))
        .arg("-i")
        .arg(video_path)
        .arg("-frames:v")
        .arg("1");

    if let Some(c) = crop {
        cmd.arg("-vf")
            .arg(format!("crop={}:{}:{}:{}", c.w.max(1), c.h.max(1), c.x.max(0), c.y.max(0)));
    }

    cmd.arg("-loglevel").arg("error").arg(&tmp_path);

    let output = cmd.output()?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(ExtractError::FfmpegFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    let bytes = std::fs::read(&tmp_path)?;
    let _ = std::fs::remove_file(&tmp_path);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_video_returns_videomissing() {
        let err = extract_frame(
            std::path::Path::new("/nonexistent/path/that/should/not/exist.mp4"),
            0,
            None,
        )
        .unwrap_err();
        match err {
            ExtractError::VideoMissing(_) => {}
            other => panic!("expected VideoMissing, got {:?}", other),
        }
    }
}
