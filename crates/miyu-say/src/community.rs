//! 社区的合成引擎（核心 `docs/reference/bus.md` 的 `tts.engine`，施工 VO-29）：配置里「引擎」选「社区引擎」、再挑一个接进
//! `tts.engine` 的包，照总线调它。它一次只收一句、最多 500 字，长的这边按句切开挨个调，拼回一段。

use miyu_ext::Core;
use serde_json::{Value, json};
use voice_common::{audio, blobs, speakable};

use crate::engine::{Audio, Engine, EngineError, Request, Voice};

/// `tts.engine.synthesize` 一次最多收多少字。
pub const LONGEST_PIECE: usize = 500;

/// 接进 `tts.engine` 的一家。
pub struct Community {
    /// 包的编号。
    pub package: String,
    /// 经它调总线、存取 blob。
    pub core: Core,
}

impl Engine for Community {
    fn synthesize(&self, request: &Request) -> Result<Audio, EngineError> {
        // 音色是必写的；没配的用它列的头一个，列不出的交空的、它自己挑默认。
        let voice = if request.voice.is_empty() {
            self.voices()
                .into_iter()
                .next()
                .map(|voice| voice.id)
                .unwrap_or_default()
        } else {
            request.voice.clone()
        };
        let parts = pieces(&request.text)
            .iter()
            .map(|piece| {
                self.one(&json!({
                    "text": piece,
                    "voice": voice,
                    "speed": request.speed,
                    "format": request.format.as_str(),
                }))
            })
            .collect::<Result<Vec<_>, _>>()?;
        joined(parts).map_err(|why| EngineError::Failed(format!("{}: {why}", self.package)))
    }

    fn voices(&self) -> Vec<Voice> {
        self.call("voices", &json!({}))
            .map(|answer| voices_in(&answer))
            .unwrap_or_default()
    }
}

impl Community {
    fn call(&self, method: &str, params: &Value) -> Result<Value, EngineError> {
        self.core
            .call(
                "bus.call",
                json!({"name": "tts.engine", "package": self.package, "method": method, "params": params}),
            )
            .map_err(|error| EngineError::Failed(format!("{}: {error}", self.package)))
    }

    /// 合成一句，读回音频的字节。
    fn one(&self, params: &Value) -> Result<Audio, EngineError> {
        let answer = self.call("synthesize", params)?;
        let blob = answer["audio"].as_str().ok_or_else(|| {
            EngineError::Failed(format!("{}: the answer has no audio.", self.package))
        })?;
        let bytes = blobs::get(&self.core, blob)
            .map_err(|why| EngineError::Failed(format!("{}: {why}", self.package)))?;
        Ok(Audio {
            bytes,
            media_type: answer["media_type"]
                .as_str()
                .unwrap_or("audio/mpeg")
                .to_string(),
            duration_ms: answer["duration_ms"].as_u64(),
        })
    }
}

/// `tts.engine.voices` 的回应读成音色。
#[must_use]
pub fn voices_in(answer: &Value) -> Vec<Voice> {
    answer["voices"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|voice| {
            let id = voice["id"].as_str()?.to_string();
            let name = voice["name"].as_str().unwrap_or(&id).to_string();
            Some(Voice { id, name })
        })
        .collect()
}

/// 按句切成一块块，每块不超过 [`LONGEST_PIECE`] 个字；一句就超过的硬切。
#[must_use]
pub fn pieces(text: &str) -> Vec<String> {
    let mut pieces: Vec<String> = Vec::new();
    let mut current = String::new();
    for sentence in speakable::sentences(text) {
        for part in hard_split(&sentence) {
            if current.chars().count() + part.chars().count() > LONGEST_PIECE {
                pieces.push(std::mem::take(&mut current));
            }
            current.push_str(&part);
        }
    }
    if !current.trim().is_empty() {
        pieces.push(current);
    }
    pieces.retain(|piece| !piece.trim().is_empty());
    pieces
}

fn hard_split(sentence: &str) -> Vec<String> {
    let chars: Vec<char> = sentence.chars().collect();
    chars
        .chunks(LONGEST_PIECE)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

/// 几段拼成一段：mp3 照字节接上；wav 读成样本接上、重写一个 wav（采样率要一样）。
///
/// # Errors
///
/// 几段的格式、采样率不一样；wav 读不出来。
pub fn joined(mut parts: Vec<Audio>) -> Result<Audio, String> {
    if parts.len() <= 1 {
        return parts.pop().ok_or_else(|| "no audio came back.".to_string());
    }
    let media_type = parts[0].media_type.clone();
    if parts.iter().any(|part| part.media_type != media_type) {
        return Err("the pieces came back in different formats.".to_string());
    }
    let duration_ms = parts
        .iter()
        .map(|part| part.duration_ms)
        .sum::<Option<u64>>();
    let bytes = if media_type == "audio/wav" {
        let mut rate = None;
        let mut samples = Vec::new();
        for part in &parts {
            let pcm = audio::read_wav(&part.bytes).map_err(|error| error.0)?;
            if *rate.get_or_insert(pcm.rate) != pcm.rate {
                return Err("the pieces came back at different sample rates.".to_string());
            }
            samples.extend(pcm.samples);
        }
        audio::write_wav(&samples, rate.unwrap_or(16_000))
    } else {
        parts.into_iter().flat_map(|part| part.bytes).collect()
    };
    Ok(Audio {
        bytes,
        media_type,
        duration_ms,
    })
}

#[cfg(test)]
mod tests;
