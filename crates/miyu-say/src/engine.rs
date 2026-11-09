//! 合成引擎（设计 20 第六节）：自带 MiniMax、MiMo、OpenAI 兼容三家，社区的照 `tts.engine` 接（VO-10）。一家一个
//! [`Engine`]：收一句念得出来的纯文字（[`voice_common::speakable`] 转好的），交回一段音频。请求怎么拼、回应怎么读
//! 写成不连网的函数，测试照各家文档的样子对；连网那一下在 [`crate::http`]。

/// 要的音频格式（设计 20 第三节「形状」`format`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// 默认：小，IM 发语音、存文件用。
    Mp3,
    /// 本机放、交给社区引擎时用。
    Wav,
}

impl Format {
    /// 写法：`mp3`、`wav`。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Mp3 => "mp3",
            Format::Wav => "wav",
        }
    }

    /// 媒体类型。
    #[must_use]
    pub fn media_type(self) -> &'static str {
        match self {
            Format::Mp3 => "audio/mpeg",
            Format::Wav => "audio/wav",
        }
    }
}

/// 合成一句。
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// 念得出来的纯文字，一句。
    pub text: String,
    /// 引擎自己的音色编号；空的用引擎的默认音色。
    pub voice: String,
    /// 语速，0.5 到 2.0。
    pub speed: f32,
    /// 要的格式。
    pub format: Format,
}

/// 合成出来的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Audio {
    /// 音频文件的字节。
    pub bytes: Vec<u8>,
    /// 实际的格式（引擎出不了要的那种时照实写）。
    pub media_type: String,
    /// 多长（毫秒），引擎说了才有。
    pub duration_ms: Option<u64>,
}

/// 一种音色（`tts.engine.voices`、配置页列的）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Voice {
    /// 引擎自己的编号。
    pub id: String,
    /// 给人看的名字。
    pub name: String,
}

/// 合成不了（`tts.synthesize` 回的错，设计 20 第三节「形状」）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// 没配好：没填 key、地址（`not_configured`）。
    NotConfigured(String),
    /// 云端答错、超时、额度用完（`engine_failed`），带对方说的原因；不带 key。
    Failed(String),
}

impl EngineError {
    /// 错误码。
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            EngineError::NotConfigured(_) => "not_configured",
            EngineError::Failed(_) => "engine_failed",
        }
    }

    /// 给人看的一句。
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            EngineError::NotConfigured(message) | EngineError::Failed(message) => message,
        }
    }
}

/// 一家引擎。
pub trait Engine: Send + Sync {
    /// 合成一句。
    ///
    /// # Errors
    ///
    /// 没配好，或者云端答错。
    fn synthesize(&self, request: &Request) -> Result<Audio, EngineError>;

    /// 有哪些音色；列不出来的交回空的，配置页让人手填。
    fn voices(&self) -> Vec<Voice>;
}
