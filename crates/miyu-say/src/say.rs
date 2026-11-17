//! 合成一段（设计 20 第三节「形状」`tts.synthesize`）：Markdown 先转成念得出来的话（[`voice_common::speakable`]），转完是
//! 空的不合成（`empty_text`），超过 5000 字的不收（`too_long`），余下整段交给选的引擎，一次请求。要首音快的调的一方
//! 自己按句切开一句一句调。

use voice_common::speakable;

use crate::engine::{Audio, Engine, EngineError, Format, Request};

/// 最多收多少字（转完以后的）。
pub const LONGEST_CHARS: usize = 5000;

/// 合成不了。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SayError {
    /// 转完没有能念的（`empty_text`）。
    EmptyText,
    /// 太长（`too_long`）。
    TooLong,
    /// 引擎的错（`not_configured`、`engine_failed`）。
    Engine(EngineError),
}

impl SayError {
    /// 错误码（设计 20 第三节「形状」）。
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            SayError::EmptyText => "empty_text",
            SayError::TooLong => "too_long",
            SayError::Engine(error) => error.code(),
        }
    }

    /// 给人看的一句。
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            SayError::EmptyText => "There is nothing to say in this text.".to_string(),
            SayError::TooLong => format!("The text is longer than {LONGEST_CHARS} characters."),
            SayError::Engine(error) => error.message().to_string(),
        }
    }
}

/// 把 `markdown` 合成一段音频。
///
/// # Errors
///
/// 没有能念的；太长；引擎没配好或者答错。
pub fn synthesize(
    engine: &dyn Engine,
    markdown: &str,
    voice: &str,
    speed: f32,
    format: Format,
) -> Result<Audio, SayError> {
    let text = speakable::to_speech(markdown);
    if text.is_empty() {
        return Err(SayError::EmptyText);
    }
    if text.chars().count() > LONGEST_CHARS {
        return Err(SayError::TooLong);
    }
    let mut audio = engine
        .synthesize(&Request {
            text,
            voice: voice.to_string(),
            speed,
            format,
        })
        .map_err(SayError::Engine)?;
    if let Some(real) = sniffed(&audio.bytes) {
        audio.media_type = real.to_string();
    }
    Ok(audio)
}

/// 照开头几个字节认真实的格式（施工 VO-35）：几家引擎照「要的格式」报类型，可本机的 GPT-SoVITS 这类不管要什么都回 wav，
/// 照报的写，存出来的文件叫 .mp3、交给场所的 `media_type` 也是错的。认不出的交回空的，照引擎报的。
#[must_use]
pub fn sniffed(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE".as_slice()) {
        Some("audio/wav")
    } else if bytes.starts_with(b"OggS") {
        Some("audio/ogg")
    } else if bytes.starts_with(b"ID3")
        || (bytes.len() > 1 && bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0)
    {
        Some("audio/mpeg")
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
