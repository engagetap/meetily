use std::time::Instant;

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::local_api::config::ApiConfigState;
use crate::screen_recorder::bookmark::{drop_bookmark, BookmarkSource};

/// Shared per-request context. Holds an `AppHandle` so handlers can look up
/// managed Tauri state (DB pool, recorder, API config) without forcing the
/// server to own concrete `Arc`s.
pub struct ApiContext<R: tauri::Runtime = tauri::Wry> {
    pub app: tauri::AppHandle<R>,
}

impl<R: tauri::Runtime> Clone for ApiContext<R> {
    fn clone(&self) -> Self {
        Self { app: self.app.clone() }
    }
}

/// Binds an axum server on `127.0.0.1:0` (random port) and spawns it.
/// Returns the actually-bound port. The server runs until the app exits.
pub async fn start_server<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> std::io::Result<u16> {
    let ctx = ApiContext { app };
    let router = build_router(ctx);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            log::error!("local_api: server exited with error: {e}");
        }
    });
    Ok(port)
}

fn build_router<R: tauri::Runtime>(ctx: ApiContext<R>) -> Router {
    Router::new()
        .route("/bookmark", post(post_bookmark::<R>))
        .route("/status", get(get_status::<R>))
        .route("/record/start", post(post_record_start::<R>))
        .route("/record/stop", post(post_record_stop::<R>))
        .with_state(ctx)
}

// ---------- auth ----------

async fn check_auth<R: tauri::Runtime>(
    headers: &HeaderMap,
    ctx: &ApiContext<R>,
) -> Result<(), Response> {
    let cfg_state = ctx.app.state::<ApiConfigState>();
    let cfg = cfg_state.snapshot().await;
    let want = format!("Bearer {}", cfg.token);
    let got = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if got == want {
        Ok(())
    } else {
        Err((StatusCode::UNAUTHORIZED, "invalid bearer token").into_response())
    }
}

// ---------- handlers ----------

#[derive(Debug, Deserialize, Default)]
struct BookmarkRequest {
    label: Option<String>,
}

#[derive(Debug, Serialize)]
struct BookmarkResponse {
    id: String,
    meeting_id: String,
    timestamp_ms: i64,
}

async fn post_bookmark<R: tauri::Runtime>(
    State(ctx): State<ApiContext<R>>,
    headers: HeaderMap,
    body: Option<Json<BookmarkRequest>>,
) -> Response {
    if let Err(r) = check_auth(&headers, &ctx).await {
        return r;
    }

    let recorder_state = ctx
        .app
        .state::<crate::screen_recorder::commands::ScreenRecorderState>();
    let info = match recorder_state.snapshot().await {
        Some(i) => i,
        None => return (StatusCode::CONFLICT, "not currently recording").into_response(),
    };

    let label = body.and_then(|Json(b)| b.label);
    let app_state = ctx.app.state::<crate::state::AppState>();
    let pool = app_state.db_manager.pool();
    match drop_bookmark(
        pool,
        &info.meeting_id,
        info.started_at,
        label.as_deref(),
        BookmarkSource::Api,
    )
    .await
    {
        Ok(d) => Json(BookmarkResponse {
            id: d.id,
            meeting_id: d.meeting_id,
            timestamp_ms: d.timestamp_ms,
        })
        .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    }
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    recording: bool,
    elapsed_ms: Option<u64>,
    recording_id: Option<String>,
    meeting_id: Option<String>,
}

async fn get_status<R: tauri::Runtime>(
    State(ctx): State<ApiContext<R>>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = check_auth(&headers, &ctx).await {
        return r;
    }
    let recorder_state = ctx
        .app
        .state::<crate::screen_recorder::commands::ScreenRecorderState>();
    let snap = recorder_state.snapshot().await;
    let resp = match snap {
        Some(info) => StatusResponse {
            recording: true,
            elapsed_ms: Some(elapsed_ms(info.started_at)),
            recording_id: Some(info.recording_id),
            meeting_id: Some(info.meeting_id),
        },
        None => StatusResponse {
            recording: false,
            elapsed_ms: None,
            recording_id: None,
            meeting_id: None,
        },
    };
    Json(resp).into_response()
}

#[derive(Debug, Deserialize)]
struct RecordStartRequest {
    meeting_id: String,
    display_id: u32,
    fps: Option<u32>,
    bitrate_kbps: Option<u32>,
    capture_mic: Option<bool>,
}

#[derive(Debug, Serialize)]
struct RecordStartResponse {
    recording_id: String,
}

async fn post_record_start<R: tauri::Runtime>(
    State(ctx): State<ApiContext<R>>,
    headers: HeaderMap,
    Json(body): Json<RecordStartRequest>,
) -> Response {
    if let Err(r) = check_auth(&headers, &ctx).await {
        return r;
    }
    let recorder_state = ctx
        .app
        .state::<crate::screen_recorder::commands::ScreenRecorderState>();
    if recorder_state.snapshot().await.is_some() {
        return (StatusCode::CONFLICT, "already recording").into_response();
    }

    let dir = match recordings_dir() {
        Ok(d) => d,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("io: {e}")).into_response();
    }
    let filename = format!(
        "{}-{}.mp4",
        body.meeting_id,
        chrono::Utc::now().timestamp_millis()
    );
    let path = dir.join(filename);

    let app_state = ctx.app.state::<crate::state::AppState>();
    let pool = app_state.db_manager.pool();
    let row = match crate::database::repositories::RecordingsRepository::create(
        pool,
        &body.meeting_id,
        &path.to_string_lossy(),
        Some(body.display_id as i64),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("db: {e}")).into_response(),
    };

    #[cfg(target_os = "macos")]
    {
        if let Err(e) = recorder_state.recorder.start(
            body.display_id,
            &path,
            body.fps.unwrap_or(30),
            body.bitrate_kbps.unwrap_or(3000),
            body.capture_mic.unwrap_or(false),
        ) {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:?}")).into_response();
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        return (StatusCode::NOT_IMPLEMENTED, "screen recording is macOS-only").into_response();
    }

    let mut active = recorder_state.active.lock().await;
    *active = Some(crate::screen_recorder::commands::ActiveRecordingInfo {
        recording_id: row.id.clone(),
        meeting_id: body.meeting_id,
        started_at: Instant::now(),
    });

    Json(RecordStartResponse {
        recording_id: row.id,
    })
    .into_response()
}

async fn post_record_stop<R: tauri::Runtime>(
    State(ctx): State<ApiContext<R>>,
    headers: HeaderMap,
) -> Response {
    if let Err(r) = check_auth(&headers, &ctx).await {
        return r;
    }
    #[cfg(target_os = "macos")]
    {
        let recorder_state = ctx
            .app
            .state::<crate::screen_recorder::commands::ScreenRecorderState>();
        let meta = match recorder_state.recorder.stop() {
            Ok(m) => m,
            Err(e) => return (StatusCode::CONFLICT, format!("{e:?}")).into_response(),
        };
        let mut active = recorder_state.active.lock().await;
        if let Some(info) = active.take() {
            let app_state = ctx.app.state::<crate::state::AppState>();
            let pool = app_state.db_manager.pool();
            let _ = crate::database::repositories::RecordingsRepository::finalize(
                pool,
                &info.recording_id,
                chrono::Utc::now().timestamp_millis(),
                Some(meta.width as i64),
                Some(meta.height as i64),
                Some(meta.fps as i64),
                Some(&meta.codec),
            )
            .await;
        }
        Json(meta).into_response()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = ctx;
        (StatusCode::NOT_IMPLEMENTED, "screen recording is macOS-only").into_response()
    }
}

// ---------- helpers ----------

fn elapsed_ms(t: Instant) -> u64 {
    t.elapsed().as_millis() as u64
}

pub(crate) fn recordings_dir() -> Result<std::path::PathBuf, String> {
    let base = dirs::data_local_dir().ok_or_else(|| "no data dir".to_string())?;
    Ok(base.join("Meetily").join("recordings"))
}
