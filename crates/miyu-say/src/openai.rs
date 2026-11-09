//! OpenAI 兼容（设计 20 第六节）：`POST <地址>/audio/speech`，`{model, input, voice, response_format, speed}`，回的就是
//! 音频的字节。OpenAI 自己、硅基流动上的 CosyVoice 和 Fish-Speech、本机起的 GPT-SoVITS、Kokoro 服务大多提供它；本机的
//! 不要 key，key 空着就不带鉴权头。

use serde_json::{Value, json};

use crate::engine::{Audio, Engine, EngineError, Request, Voice};

/// 配好的 OpenAI 兼容服务。
pub struct OpenAiCompatible {
    /// 地址，带到 `/v1` 为止，例如 `https://api.openai.com/v1`。
    pub base_url: String,
    /// 密钥；本机服务可以空着。
    pub key: String,
    /// 模型，例如 `gpt-4o-mini-tts`。
    pub model: String,
}

impl Engine for OpenAiCompatible {
    fn synthesize(&self, request: &Request) -> Result<Audio, EngineError> {
        if self.base_url.is_empty() || self.model.is_empty() {
            return Err(EngineError::NotConfigured(
                "The address and model of the OpenAI-compatible service are not set.".to_string(),
            ));
        }
        let url = format!("{}/audio/speech", self.base_url.trim_end_matches('/'));
        let auth = format!("Bearer {}", self.key);
        let headers: Vec<(&str, &str)> = if self.key.is_empty() {
            Vec::new()
        } else {
            vec![("Authorization", &auth)]
        };
        let bytes = crate::http::post_json(&url, &headers, &body(&self.model, request))?;
        if bytes.is_empty() {
            return Err(EngineError::Failed(
                "The service answered with no audio.".to_string(),
            ));
        }
        Ok(Audio {
            bytes,
            media_type: request.format.media_type().to_string(),
            duration_ms: None,
        })
    }

    fn voices(&self) -> Vec<Voice> {
        // 各家的音色不一样，人手填。
        Vec::new()
    }
}

/// 请求体。
pub(crate) fn body(model: &str, request: &Request) -> Value {
    let voice = if request.voice.is_empty() {
        "alloy"
    } else {
        &request.voice
    };
    json!({
        "model": model,
        "input": request.text,
        "voice": voice,
        "response_format": request.format.as_str(),
        "speed": request.speed.clamp(0.5, 2.0),
    })
}

#[cfg(test)]
mod tests;
