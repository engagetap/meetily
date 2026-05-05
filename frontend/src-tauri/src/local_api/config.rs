use std::path::{Path, PathBuf};

use rand::distributions::Alphanumeric;
use rand::Rng;
use serde::{Deserialize, Serialize};

/// On-disk view of the local API config. Kept stable so external tools can
/// rely on the schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub host: String,
    pub port: u16,
    pub token: String,
    pub url: String,
}

impl ApiConfig {
    pub fn new(host: &str, port: u16, token: String) -> Self {
        Self {
            host: host.to_string(),
            port,
            token,
            url: format!("http://{}:{}", host, port),
        }
    }
}

/// Generates a 40-char URL-safe token suitable for a Bearer header.
pub fn generate_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(40)
        .map(char::from)
        .collect()
}

/// `~/Library/Application Support/Meetily/api.json` (or platform equivalent).
pub fn config_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("Meetily").join("api.json"))
}

/// Persists the config to `api.json` with file mode 0600 on Unix.
pub fn write_config(path: &Path, cfg: &ApiConfig) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(path, json)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(path)?.permissions();
        perm.set_mode(0o600);
        std::fs::set_permissions(path, perm)?;
    }
    Ok(())
}

/// State holder shared with Tauri commands and the axum router so the running
/// app can return / regenerate the token without restarting the server.
pub struct ApiConfigState {
    inner: tokio::sync::RwLock<ApiConfig>,
}

impl ApiConfigState {
    pub fn new(cfg: ApiConfig) -> Self {
        Self {
            inner: tokio::sync::RwLock::new(cfg),
        }
    }

    pub async fn snapshot(&self) -> ApiConfig {
        self.inner.read().await.clone()
    }

    /// Replaces the token, persists to disk, and returns the new config.
    pub async fn rotate_token(&self) -> std::io::Result<ApiConfig> {
        let mut guard = self.inner.write().await;
        guard.token = generate_token();
        if let Some(p) = config_path() {
            write_config(&p, &guard)?;
        }
        Ok(guard.clone())
    }

    /// Updates the port (called once after the listener binds to its random
    /// port), persisting the new value to `api.json`.
    pub async fn update_port(&self, port: u16) {
        let mut guard = self.inner.write().await;
        guard.port = port;
        guard.url = format!("http://{}:{}", guard.host, port);
        if let Some(p) = config_path() {
            let _ = write_config(&p, &guard);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn token_is_alphanumeric_and_40_chars() {
        let t = generate_token();
        assert_eq!(t.len(), 40);
        assert!(t.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn write_then_read_roundtrip_preserves_fields() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("api.json");
        let cfg = ApiConfig::new("127.0.0.1", 51234, "tok".into());
        write_config(&path, &cfg).unwrap();
        let read: ApiConfig = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(read.host, "127.0.0.1");
        assert_eq!(read.port, 51234);
        assert_eq!(read.token, "tok");
        assert_eq!(read.url, "http://127.0.0.1:51234");
    }

    #[cfg(unix)]
    #[test]
    fn write_sets_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().unwrap();
        let path = dir.path().join("api.json");
        let cfg = ApiConfig::new("127.0.0.1", 1, "tok".into());
        write_config(&path, &cfg).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
