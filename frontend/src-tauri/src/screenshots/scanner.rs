use std::path::Path;
use std::process::Command;

use crate::audio::ffmpeg::find_ffmpeg_path;
use crate::screenshots::extractor::ExtractError;

/// Walks a recording at low fps + low resolution and returns timestamps
/// (in ms from start) of frames that are *stable* (similar to the previous
/// frame, so not in the middle of a transition) and *novel* (visually
/// different from every previously-kept frame).
///
/// This is the cheap, privacy-pure heuristic the spec calls "frame-diff
/// filter" — no transcription, no model. The output feeds into the
/// screenshot picker as `source = 'frame_diff'` candidates.
///
/// `sample_fps` controls how many frames per second of source we inspect
/// (1 is plenty for slide-style content). `max_candidates` caps the
/// returned set so a long meeting doesn't explode into hundreds.
pub fn scan_for_novel_frames(
    video_path: &Path,
    sample_fps: u32,
    max_candidates: usize,
) -> Result<Vec<i64>, ExtractError> {
    if !video_path.exists() {
        return Err(ExtractError::VideoMissing(
            video_path.to_string_lossy().to_string(),
        ));
    }
    if sample_fps == 0 {
        return Ok(vec![]);
    }

    let ffmpeg = find_ffmpeg_path().ok_or(ExtractError::FfmpegMissing)?;

    // 32x18 grayscale (luma) ≈ 16:9 thumbnail, 576 bytes per frame.
    const W: usize = 32;
    const H: usize = 18;
    const FRAME_BYTES: usize = W * H;

    // Tuneable thresholds. Sum-of-absolute-differences over 576 luma bytes
    // ranges 0..146880; a slide change is typically tens of thousands.
    const STABILITY_MAX: u64 = 4_000; // similar to previous frame (no transition)
    const NOVELTY_MIN: u64 = 6_000; // distinct from every kept frame

    let output = Command::new(&ffmpeg)
        .arg("-i")
        .arg(video_path)
        .arg("-vf")
        .arg(format!("fps={},scale={}:{}:flags=area,format=gray", sample_fps, W, H))
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("gray")
        .arg("-loglevel")
        .arg("error")
        .arg("pipe:1")
        .output()
        .map_err(ExtractError::Io)?;

    if !output.status.success() {
        return Err(ExtractError::FfmpegFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    let mut kept_thumbs: Vec<[u8; FRAME_BYTES]> = Vec::new();
    let mut kept_ts: Vec<i64> = Vec::new();
    let mut prev_thumb: Option<[u8; FRAME_BYTES]> = None;
    let frame_dur_ms = 1000_i64 / sample_fps as i64;

    for (i, chunk) in output.stdout.chunks_exact(FRAME_BYTES).enumerate() {
        let mut buf = [0u8; FRAME_BYTES];
        buf.copy_from_slice(chunk);
        let ts_ms = (i as i64) * frame_dur_ms;

        let stable = match &prev_thumb {
            Some(p) => sad(p, &buf) <= STABILITY_MAX,
            // The very first frame has no predecessor; treat as stable so a
            // genuinely-static opening frame can be kept.
            None => true,
        };
        let novel = kept_thumbs.iter().all(|k| sad(k, &buf) >= NOVELTY_MIN);

        if stable && novel {
            kept_thumbs.push(buf);
            kept_ts.push(ts_ms);
            if kept_ts.len() >= max_candidates {
                break;
            }
        }
        prev_thumb = Some(buf);
    }

    Ok(kept_ts)
}

#[inline]
fn sad(a: &[u8], b: &[u8]) -> u64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as u64)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_video_returns_videomissing() {
        let err = scan_for_novel_frames(
            std::path::Path::new("/nonexistent/path.mp4"),
            1,
            10,
        )
        .unwrap_err();
        match err {
            ExtractError::VideoMissing(_) => {}
            other => panic!("expected VideoMissing, got {:?}", other),
        }
    }

    #[test]
    fn sad_zero_for_identical_buffers() {
        let a = [0u8; 10];
        let b = [0u8; 10];
        assert_eq!(sad(&a, &b), 0);
    }

    #[test]
    fn sad_sums_absolute_differences() {
        let a = [10u8, 20, 30];
        let b = [12u8, 18, 35];
        assert_eq!(sad(&a, &b), 2 + 2 + 5);
    }
}
