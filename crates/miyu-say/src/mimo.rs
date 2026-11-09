//! MiMo（小米，设计 20 第六节）：OpenAI 兼容的 `POST <地址>/chat/completions`，模型 `mimo-v2.5-tts`，要念的字放在
//! `role: assistant` 的消息里（不是 user），`audio: {format, voice}`；音频是 base64，在
//! `choices[0].message.audio.data`（官方文档，2026-11-10 查）。不收语速。

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use crate::engine::{Audio, Engine, EngineError, Format, Request, Voice};

/// 官方地址。
pub const BASE_URL: &str = "https://api.xiaomimimo.com/v1";
/// 默认的模型。
pub const MODEL: &str = "mimo-v2.5-tts";
/// 没配音色时用的。
pub const DEFAULT_VOICE: &str = "mimo_default";

/// 配好的 MiMo。
pub struct MiMo {
    /// 地址，默认 [`BASE_URL`]。
    pub base_url: String,
    /// 密钥。
    pub key: String,
    /// 模型。
    pub model: String,
}

impl Engine for MiMo {
    fn synthesize(&self, request: &Request) -> Result<Audio, EngineError> {
        if self.key.is_empty() {
            return Err(EngineError::NotConfigured(
                "The MiMo API key is not set.".to_string(),
            ));
        }
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let auth = format!("Bearer {}", self.key);
        let answer = crate::http::post_json(
            &url,
            &[("Authorization", &auth)],
            &body(&self.model, request),
        )?;
        parse(&answer, request.format)
    }

    fn voices(&self) -> Vec<Voice> {
        // 官方的内置音色。
        [
            "mimo_default",
            "冰糖",
            "茉莉",
            "苏打",
            "白桦",
            "Mia",
            "Chloe",
            "Milo",
            "Dean",
        ]
        .into_iter()
        .map(|id| Voice {
            id: id.to_string(),
            name: id.to_string(),
        })
        .collect()
    }
}

/// 请求体。
pub(crate) fn body(model: &str, request: &Request) -> Value {
    let voice = if request.voice.is_empty() {
        DEFAULT_VOICE
    } else {
        &request.voice
    };
    json!({
        "model": model,
        "messages": [{"role": "assistant", "content": request.text}],
        "audio": {"format": request.format.as_str(), "voice": voice},
    })
}

/// 读回应。
pub(crate) fn parse(answer: &[u8], format: Format) -> Result<Audio, EngineError> {
    let answer: Value = serde_json::from_slice(answer).map_err(|error| {
        EngineError::Failed(format!("MiMo answered something unreadable: {error}."))
    })?;
    if let Some(message) = answer["error"]["message"].as_str() {
        return Err(EngineError::Failed(format!("MiMo refused: {message}")));
    }
    let data = answer["choices"][0]["message"]["audio"]["data"]
        .as_str()
        .unwrap_or_default();
    let bytes = STANDARD
        .decode(data)
        .map_err(|error| EngineError::Failed(format!("MiMo's audio is not base64: {error}.")))?;
    if bytes.is_empty() {
        return Err(EngineError::Failed(
            "MiMo answered with no audio.".to_string(),
        ));
    }
    Ok(Audio {
        bytes,
        media_type: format.media_type().to_string(),
        duration_ms: None,
    })
}

#[cfg(test)]
mod tests;
