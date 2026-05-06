//! Phase 3: optional cloud-vision enrichment.
//!
//! For each pending screenshot candidate, send the extracted frame to
//! Anthropic Claude (vision) and ask it to return:
//!   - a usefulness score in [0.0, 1.0]
//!   - a one-line caption suitable for a meeting note
//!
//! Falls back to a no-op when the user hasn't configured an API key.

use base64::Engine as _;
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum VisionError {
    #[error("no API key configured for provider")]
    NoApiKey,
    #[error("http error: {0}")]
    Http(String),
    #[error("api error: {0}")]
    Api(String),
    #[error("invalid response: {0}")]
    InvalidResponse(String),
}

#[derive(Debug, Clone)]
pub struct VisionScore {
    pub score: f64,
    pub caption: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct ParsedScore {
    score: f64,
    caption: String,
}

/// Sends one PNG to Claude and parses the structured score+caption back.
/// Uses claude-3-5-sonnet which supports vision.
pub async fn score_with_anthropic(
    api_key: &str,
    png_bytes: &[u8],
    transcript_context: Option<&str>,
) -> Result<VisionScore, VisionError> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(png_bytes);

    let prompt = format!(
        "You are scoring a single screenshot from a meeting recording for inclusion in the \
         meeting notes. Reply with JSON only, no markdown fences, exactly: \
         {{\"score\": <0.0-1.0>, \"caption\": \"<one short sentence>\"}}. \
         The score reflects how visually informative this frame is — slides, dashboards, \
         diagrams, code, important text → high; webcam tiles, blank screens, transitions, \
         loading states → low. The caption should describe what is shown in <=12 words.\n\n\
         {}",
        match transcript_context {
            Some(c) if !c.is_empty() => format!("Transcript context around this moment:\n{}", c),
            _ => String::new(),
        }
    );

    let body = serde_json::json!({
        "model": "claude-3-5-sonnet-latest",
        "max_tokens": 200,
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": "image/png",
                            "data": b64
                        }
                    },
                    { "type": "text", "text": prompt }
                ]
            }
        ]
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| VisionError::Http(e.to_string()))?;

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| VisionError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(VisionError::Api(format!("{}: {}", status, text)));
    }

    let parsed: AnthropicResponse = resp
        .json()
        .await
        .map_err(|e| VisionError::Http(e.to_string()))?;

    let text_block = parsed
        .content
        .into_iter()
        .find(|c| c.kind == "text")
        .ok_or_else(|| VisionError::InvalidResponse("no text content".into()))?;

    let trimmed = strip_code_fence(&text_block.text);
    let score: ParsedScore = serde_json::from_str(&trimmed)
        .map_err(|e| VisionError::InvalidResponse(format!("parse: {} body: {}", e, trimmed)))?;

    Ok(VisionScore {
        score: score.score.clamp(0.0, 1.0),
        caption: score.caption,
    })
}

/// Strips ```json ... ``` fences if Claude wraps its output despite being told not to.
fn strip_code_fence(s: &str) -> String {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("```json") {
        return rest.trim_end_matches("```").trim().to_string();
    }
    if let Some(rest) = s.strip_prefix("```") {
        return rest.trim_end_matches("```").trim().to_string();
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_stripping_handles_plain_json() {
        let s = "{\"score\": 0.9, \"caption\": \"foo\"}";
        assert_eq!(strip_code_fence(s), s);
    }

    #[test]
    fn fence_stripping_handles_json_fence() {
        let s = "```json\n{\"score\": 0.9, \"caption\": \"foo\"}\n```";
        assert!(strip_code_fence(s).starts_with("{\"score\""));
    }

    #[test]
    fn fence_stripping_handles_bare_fence() {
        let s = "```\n{\"score\": 0.9, \"caption\": \"foo\"}\n```";
        assert!(strip_code_fence(s).starts_with("{\"score\""));
    }
}
