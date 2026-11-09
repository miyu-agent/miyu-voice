//! MiniMax（设计 20 第六节）：`POST <地址>/v1/t2a_v2`，`Authorization: Bearer <key>`，非流式、`output_format = hex`，
//! 音频在 `data.audio`（十六进制），时长在 `extra_info.audio_length`，`base_resp.status_code` 不是 0 的是错。
//! 国内 `https://api.minimaxi.com`，国际 `https://api.minimax.io`（官方文档，2026-11-10 查）。

use serde_json::{Value, json};

use crate::engine::{Audio, Engine, EngineError, Format, Request, Voice};

/// 国内的地址。
pub const MAINLAND: &str = "https://api.minimaxi.com";
/// 国际的地址。
pub const INTERNATIONAL: &str = "https://api.minimax.io";
/// 默认的模型：turbo 比 hd 快、便宜，念回复够用；配置里能换。
pub const MODEL: &str = "speech-2.8-turbo";
/// 没配音色时用的。
pub const DEFAULT_VOICE: &str = "female-tianmei";

/// 配好的 MiniMax。
pub struct MiniMax {
    /// `MAINLAND` 或 `INTERNATIONAL`，或者人填的。
    pub base_url: String,
    /// 密钥（配置里的 `secret`，交给扩展时是真值）。
    pub key: String,
    /// 模型。
    pub model: String,
}

impl Engine for MiniMax {
    fn synthesize(&self, request: &Request) -> Result<Audio, EngineError> {
        if self.key.is_empty() {
            return Err(EngineError::NotConfigured(
                "The MiniMax API key is not set.".to_string(),
            ));
        }
        let url = format!("{}/v1/t2a_v2", self.base_url.trim_end_matches('/'));
        let auth = format!("Bearer {}", self.key);
        let answer = crate::http::post_json(
            &url,
            &[("Authorization", &auth)],
            &body(&self.model, request),
        )?;
        parse(&answer, request.format)
    }

    fn voices(&self) -> Vec<Voice> {
        // 系统音色的一部分（官方音色表），配置页的下拉照它；克隆出来的音色人手填编号。
        [
            ("female-tianmei", "甜美女声"),
            ("female-shaonv", "少女"),
            ("female-chengshu", "成熟女声"),
            ("male-qn-qingse", "青涩青年"),
            ("male-qn-jingying", "精英青年"),
            ("presenter_female", "女性主持人"),
            ("presenter_male", "男性主持人"),
        ]
        .into_iter()
        .map(|(id, name)| Voice {
            id: id.to_string(),
            name: name.to_string(),
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
        "text": request.text,
        "stream": false,
        "output_format": "hex",
        "voice_setting": {"voice_id": voice, "speed": request.speed.clamp(0.5, 2.0), "vol": 1, "pitch": 0},
        "audio_setting": {"sample_rate": 32000, "bitrate": 128_000, "format": request.format.as_str(), "channel": 1},
    })
}

/// 读回应。
pub(crate) fn parse(answer: &[u8], format: Format) -> Result<Audio, EngineError> {
    let answer: Value = serde_json::from_slice(answer).map_err(|error| {
        EngineError::Failed(format!("MiniMax answered something unreadable: {error}."))
    })?;
    let status = answer["base_resp"]["status_code"].as_i64().unwrap_or(-1);
    if status != 0 {
        let message = answer["base_resp"]["status_msg"]
            .as_str()
            .unwrap_or("unknown error");
        return Err(EngineError::Failed(format!(
            "MiniMax refused ({status}): {message}."
        )));
    }
    let hex_audio = answer["data"]["audio"].as_str().unwrap_or_default();
    let bytes = hex::decode(hex_audio)
        .map_err(|error| EngineError::Failed(format!("MiniMax's audio is not hex: {error}.")))?;
    if bytes.is_empty() {
        return Err(EngineError::Failed(
            "MiniMax answered with no audio.".to_string(),
        ));
    }
    Ok(Audio {
        bytes,
        media_type: format.media_type().to_string(),
        duration_ms: answer["extra_info"]["audio_length"].as_u64(),
    })
}

#[cfg(test)]
mod tests;
