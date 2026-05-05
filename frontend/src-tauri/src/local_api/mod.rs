//! Local HTTP control API for StreamDeck / Companion / scripts.
//!
//! Bound to 127.0.0.1 on a random port at app startup. Bearer-token auth.
//! The token + port are written to `~/Library/Application Support/Meetily/api.json`
//! (mode 0600) so external callers can discover the endpoint.
//!
//! Endpoints (Phase 1B):
//!   POST /bookmark            -> drop a bookmark at the current recording's elapsed offset
//!   GET  /status              -> current recording state (recording, elapsed_ms, ...)
//!   POST /record/start        -> start a recording (requires meeting_id, display_id)
//!   POST /record/stop         -> stop the active recording

pub mod commands;
pub mod config;
pub mod server;

pub use config::{ApiConfig, ApiConfigState};
pub use server::start_server;
