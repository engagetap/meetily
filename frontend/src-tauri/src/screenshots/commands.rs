use std::path::PathBuf;

use base64::Engine as _;
use tauri::State;

use crate::database::models::MeetingScreenshot;
use crate::database::repositories::setting::SettingsRepository;
use crate::database::repositories::{RecordingsRepository, ScreenshotsRepository};
use crate::screenshots::extractor::{extract_frame, CropRect};
use crate::screenshots::picker::{generate_candidates_from_bookmarks, generate_frame_diff_candidates};
use crate::screenshots::vision::score_with_anthropic;
use crate::state::AppState;

#[derive(Debug, thiserror::Error, serde::Serialize)]
#[serde(tag = "type", content = "message")]
pub enum ScreenshotCommandError {
    #[error("db error: {0}")]
    Db(String),
    #[error("no recording found for meeting {0}")]
    NoRecording(String),
    #[error("ffmpeg error: {0}")]
    Ffmpeg(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("not found: {0}")]
    NotFound(String),
}

fn screenshots_dir(meeting_id: &str) -> Result<PathBuf, ScreenshotCommandError> {
    let base = dirs::data_local_dir()
        .ok_or_else(|| ScreenshotCommandError::Io("no data dir".into()))?;
    let dir = base.join("Meetily").join("screenshots").join(meeting_id);
    std::fs::create_dir_all(&dir).map_err(|e| ScreenshotCommandError::Io(e.to_string()))?;
    Ok(dir)
}

/// Generate candidate screenshots for a meeting from its bookmarks (always)
/// and from frame-diff scanning of the latest recording (when one is
/// available). Returns the total number of new rows created.
#[tauri::command]
pub async fn screenshots_generate(
    meeting_id: String,
    app_state: State<'_, AppState>,
) -> Result<usize, ScreenshotCommandError> {
    let pool = app_state.db_manager.pool();
    let mut total = 0;

    total += generate_candidates_from_bookmarks(pool, &meeting_id)
        .await
        .map_err(|e| ScreenshotCommandError::Db(e.to_string()))?;

    // If we have a finalized recording, also run frame-diff. Defaults
    // (sample_fps=1, max=12, dedup ±1s) match the spec; refining these is a
    // settings-panel concern for later.
    if let Some(rec) = RecordingsRepository::latest_finalized_for_meeting(pool, &meeting_id)
        .await
        .map_err(|e| ScreenshotCommandError::Db(e.to_string()))?
    {
        total += generate_frame_diff_candidates(
            pool,
            &meeting_id,
            std::path::Path::new(&rec.file_path),
            1,
            12,
            1_000,
        )
        .await
        .map_err(|e| ScreenshotCommandError::Db(e.to_string()))?;
    }

    Ok(total)
}

/// List all screenshot rows (accepted + pending review) for a meeting.
#[tauri::command]
pub async fn screenshots_list(
    meeting_id: String,
    app_state: State<'_, AppState>,
) -> Result<Vec<MeetingScreenshot>, ScreenshotCommandError> {
    let pool = app_state.db_manager.pool();
    ScreenshotsRepository::list_for_meeting(pool, &meeting_id)
        .await
        .map_err(|e| ScreenshotCommandError::Db(e.to_string()))
}

/// Extract a frame at `(timestamp_ms, optional crop_rect)` from the meeting's
/// most recent recording and return a data URL the frontend can render.
/// Used by the review UI's scrub + crop preview.
#[tauri::command]
pub async fn screenshots_preview_frame(
    meeting_id: String,
    timestamp_ms: i64,
    crop_x: Option<i64>,
    crop_y: Option<i64>,
    crop_w: Option<i64>,
    crop_h: Option<i64>,
    app_state: State<'_, AppState>,
) -> Result<String, ScreenshotCommandError> {
    let pool = app_state.db_manager.pool();
    let rec = RecordingsRepository::latest_finalized_for_meeting(pool, &meeting_id)
        .await
        .map_err(|e| ScreenshotCommandError::Db(e.to_string()))?
        .ok_or_else(|| ScreenshotCommandError::NoRecording(meeting_id.clone()))?;

    let crop = match (crop_x, crop_y, crop_w, crop_h) {
        (Some(x), Some(y), Some(w), Some(h)) if w > 0 && h > 0 => Some(CropRect { x, y, w, h }),
        _ => None,
    };

    let bytes = tokio::task::spawn_blocking(move || {
        extract_frame(std::path::Path::new(&rec.file_path), timestamp_ms, crop)
    })
    .await
    .map_err(|e| ScreenshotCommandError::Ffmpeg(format!("join: {e}")))?
    .map_err(|e| ScreenshotCommandError::Ffmpeg(e.to_string()))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:image/png;base64,{}", b64))
}

#[derive(Debug, serde::Deserialize)]
pub struct CropArg {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

/// Update the editable fields of a screenshot row (timestamp + crop + caption).
/// Does not touch `accepted` or `image_path`.
#[tauri::command]
pub async fn screenshots_update(
    id: String,
    timestamp_ms: i64,
    crop: Option<CropArg>,
    caption: Option<String>,
    app_state: State<'_, AppState>,
) -> Result<(), ScreenshotCommandError> {
    let pool = app_state.db_manager.pool();
    let crop_tuple = crop.map(|c| (c.x, c.y, c.w, c.h));
    ScreenshotsRepository::update_edit_fields(
        pool,
        &id,
        timestamp_ms,
        crop_tuple,
        caption.as_deref(),
    )
    .await
    .map_err(|e| ScreenshotCommandError::Db(e.to_string()))
}

/// Accept a screenshot: extract the final PNG from the source mp4 using the
/// row's current `(timestamp_ms, crop)`, write it to disk, mark the row
/// accepted with the new `image_path`.
#[tauri::command]
pub async fn screenshots_accept(
    id: String,
    app_state: State<'_, AppState>,
) -> Result<MeetingScreenshot, ScreenshotCommandError> {
    let pool = app_state.db_manager.pool();
    let row = ScreenshotsRepository::get(pool, &id)
        .await
        .map_err(|e| ScreenshotCommandError::Db(e.to_string()))?
        .ok_or_else(|| ScreenshotCommandError::NotFound(id.clone()))?;

    let rec = RecordingsRepository::latest_finalized_for_meeting(pool, &row.meeting_id)
        .await
        .map_err(|e| ScreenshotCommandError::Db(e.to_string()))?
        .ok_or_else(|| ScreenshotCommandError::NoRecording(row.meeting_id.clone()))?;

    let crop = match (row.crop_x, row.crop_y, row.crop_w, row.crop_h) {
        (Some(x), Some(y), Some(w), Some(h)) if w > 0 && h > 0 => Some(CropRect { x, y, w, h }),
        _ => None,
    };

    let bytes = {
        let path = rec.file_path.clone();
        let ts = row.timestamp_ms;
        tokio::task::spawn_blocking(move || extract_frame(std::path::Path::new(&path), ts, crop))
            .await
            .map_err(|e| ScreenshotCommandError::Ffmpeg(format!("join: {e}")))?
            .map_err(|e| ScreenshotCommandError::Ffmpeg(e.to_string()))?
    };

    let dir = screenshots_dir(&row.meeting_id)?;
    let filename = format!("{}.png", row.id);
    let path = dir.join(filename);
    std::fs::write(&path, &bytes).map_err(|e| ScreenshotCommandError::Io(e.to_string()))?;

    ScreenshotsRepository::set_accepted(pool, &row.id, Some(&path.to_string_lossy()))
        .await
        .map_err(|e| ScreenshotCommandError::Db(e.to_string()))?;

    let updated = ScreenshotsRepository::get(pool, &row.id)
        .await
        .map_err(|e| ScreenshotCommandError::Db(e.to_string()))?
        .ok_or_else(|| ScreenshotCommandError::NotFound(row.id.clone()))?;
    Ok(updated)
}

#[tauri::command]
pub async fn screenshots_reject(
    id: String,
    app_state: State<'_, AppState>,
) -> Result<(), ScreenshotCommandError> {
    let pool = app_state.db_manager.pool();
    ScreenshotsRepository::set_rejected(pool, &id)
        .await
        .map_err(|e| ScreenshotCommandError::Db(e.to_string()))
}

#[derive(Debug, serde::Serialize)]
pub struct EnrichmentResult {
    pub processed: usize,
    pub updated: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

/// Enrich pending (not-yet-accepted) screenshot candidates by sending each
/// extracted frame to Anthropic's vision API and storing the returned score
/// + caption. Skips silently if the user has not configured a Claude API
/// key. Caption + confidence are written; `accepted` is left untouched so
/// the user still reviews them.
#[tauri::command]
pub async fn screenshots_enrich_with_vision(
    meeting_id: String,
    app_state: State<'_, AppState>,
) -> Result<EnrichmentResult, ScreenshotCommandError> {
    let pool = app_state.db_manager.pool();

    let api_key = SettingsRepository::get_api_key(pool, "claude")
        .await
        .map_err(|e| ScreenshotCommandError::Db(e.to_string()))?
        .filter(|k| !k.is_empty());

    let api_key = match api_key {
        Some(k) => k,
        None => {
            return Ok(EnrichmentResult {
                processed: 0,
                updated: 0,
                skipped: 0,
                errors: vec!["no Anthropic API key configured (Settings → AI provider)".into()],
            });
        }
    };

    let rec = RecordingsRepository::latest_finalized_for_meeting(pool, &meeting_id)
        .await
        .map_err(|e| ScreenshotCommandError::Db(e.to_string()))?
        .ok_or_else(|| ScreenshotCommandError::NoRecording(meeting_id.clone()))?;

    let shots = ScreenshotsRepository::list_for_meeting(pool, &meeting_id)
        .await
        .map_err(|e| ScreenshotCommandError::Db(e.to_string()))?;

    // Only enrich rows the user hasn't already curated (no caption + not accepted).
    let pending: Vec<_> = shots
        .into_iter()
        .filter(|s| s.accepted == 0 && s.caption.as_deref().unwrap_or("").is_empty())
        .collect();

    let mut updated = 0;
    let mut errors: Vec<String> = Vec::new();
    let processed = pending.len();

    for s in &pending {
        let video_path = rec.file_path.clone();
        let ts = s.timestamp_ms;
        let crop = match (s.crop_x, s.crop_y, s.crop_w, s.crop_h) {
            (Some(x), Some(y), Some(w), Some(h)) if w > 0 && h > 0 => Some(CropRect { x, y, w, h }),
            _ => None,
        };

        let bytes = tokio::task::spawn_blocking(move || {
            extract_frame(std::path::Path::new(&video_path), ts, crop)
        })
        .await
        .map_err(|e| ScreenshotCommandError::Ffmpeg(format!("join: {e}")))?;

        let bytes = match bytes {
            Ok(b) => b,
            Err(e) => {
                errors.push(format!("{}: extract: {}", s.id, e));
                continue;
            }
        };

        match score_with_anthropic(&api_key, &bytes, None).await {
            Ok(score) => {
                // Persist by re-using update_edit_fields (timestamp/crop unchanged, caption replaced)
                let crop_tuple = match (s.crop_x, s.crop_y, s.crop_w, s.crop_h) {
                    (Some(x), Some(y), Some(w), Some(h)) => Some((x, y, w, h)),
                    _ => None,
                };
                if let Err(e) = ScreenshotsRepository::update_edit_fields(
                    pool,
                    &s.id,
                    s.timestamp_ms,
                    crop_tuple,
                    Some(&score.caption),
                )
                .await
                {
                    errors.push(format!("{}: db: {}", s.id, e));
                    continue;
                }
                // Confidence updates aren't covered by update_edit_fields; do a focused write.
                let _ = sqlx::query("UPDATE meeting_screenshots SET confidence=? WHERE id=?")
                    .bind(score.score)
                    .bind(&s.id)
                    .execute(pool)
                    .await;
                updated += 1;
            }
            Err(e) => {
                errors.push(format!("{}: vision: {}", s.id, e));
            }
        }
    }

    Ok(EnrichmentResult {
        processed,
        updated,
        skipped: processed - updated - errors.len(),
        errors,
    })
}

/// Read an accepted screenshot's PNG from disk and return it as a data URL.
/// Used by the highlights gallery so it doesn't have to expose filesystem
/// paths to the frontend.
#[tauri::command]
pub async fn screenshots_read_image(
    id: String,
    app_state: State<'_, AppState>,
) -> Result<String, ScreenshotCommandError> {
    let pool = app_state.db_manager.pool();
    let row = ScreenshotsRepository::get(pool, &id)
        .await
        .map_err(|e| ScreenshotCommandError::Db(e.to_string()))?
        .ok_or_else(|| ScreenshotCommandError::NotFound(id.clone()))?;
    let path = row
        .image_path
        .ok_or_else(|| ScreenshotCommandError::NotFound(format!("{} not yet accepted", id)))?;
    let bytes = std::fs::read(&path).map_err(|e| ScreenshotCommandError::Io(e.to_string()))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:image/png;base64,{}", b64))
}
