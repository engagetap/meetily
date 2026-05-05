use tauri::State;

use crate::local_api::config::{ApiConfig, ApiConfigState};

#[derive(Debug, thiserror::Error, serde::Serialize)]
#[serde(tag = "type", content = "message")]
pub enum LocalApiError {
    #[error("io error: {0}")]
    Io(String),
}

#[tauri::command]
pub async fn local_api_get_config(
    state: State<'_, ApiConfigState>,
) -> Result<ApiConfig, LocalApiError> {
    Ok(state.snapshot().await)
}

#[tauri::command]
pub async fn local_api_regenerate_token(
    state: State<'_, ApiConfigState>,
) -> Result<ApiConfig, LocalApiError> {
    state
        .rotate_token()
        .await
        .map_err(|e| LocalApiError::Io(e.to_string()))
}
