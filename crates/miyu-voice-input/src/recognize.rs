//! 本机识别（设计 20 第五节）：sherpa-onnx 跑 SenseVoice-Small，CPU 就够。模型第一次认的时候才加载；空闲卸掉靠整个子进程
//! 退出（[`crate::worker`]）。一种语言一个识别器（SenseVoice 的语言在建的时候定）。长的照安静处切成一段段再拼
//! （[`crate::segment`]）。

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use sherpa_onnx::{
    LinearResampler, OfflineModelConfig, OfflineRecognizer, OfflineRecognizerConfig,
    OfflineSenseVoiceModelConfig,
};
use voice_common::audio::Pcm;

use crate::filter;
use crate::model::Model;
use crate::segment;

/// 识别要的采样率。
pub const RATE: u32 = 16_000;
/// 最长收多少（毫秒）：10 分钟（设计 20 第三节「形状」）。
pub const LONGEST_MS: u64 = 10 * 60 * 1000;
/// 识别用几个线程：旧版实测两个核和十六个核差不多快（设计 20 第五节）。
const THREADS: i32 = 2;

/// 认的语言（设计 20 第三节「形状」）。
pub const LANGUAGES: [&str; 6] = ["zh", "yue", "en", "ja", "ko", "auto"];

/// 识别出的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heard {
    /// 字；没听清的是空的。
    pub text: String,
    /// 照哪种语言认的。
    pub language: String,
    /// 声音多长（毫秒）。
    pub duration_ms: u64,
    /// 听清了没有（第五节的过滤）。
    pub clear: bool,
}

/// 认不了的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// 超过 10 分钟。
    TooLong,
    /// 不认识的语言。
    Language(String),
    /// 模型加载不起来（文件坏了）。
    Engine(String),
}

/// 识别器们，一种语言一个，用到才建。
pub struct Recognizer {
    model: Model,
    loaded: Mutex<HashMap<String, OfflineRecognizer>>,
}

impl Recognizer {
    /// 用 `model`（要已经下好）。
    #[must_use]
    pub fn new(model: Model) -> Recognizer {
        Recognizer {
            model,
            loaded: Mutex::new(HashMap::new()),
        }
    }

    /// 把一段声音认成字：先过滤太短的，转成 16 kHz，长的切段，一段段认完拼起来，再看听没听清。
    ///
    /// # Errors
    ///
    /// 太长、语言不认识、模型加载不起来。
    pub fn transcribe(&self, pcm: &Pcm, language: &str) -> Result<Heard, Refused> {
        if !LANGUAGES.contains(&language) {
            return Err(Refused::Language(language.to_string()));
        }
        let duration_ms = pcm.duration_ms();
        if duration_ms > LONGEST_MS {
            return Err(Refused::TooLong);
        }
        let unclear = Heard {
            text: String::new(),
            language: language.to_string(),
            duration_ms,
            clear: false,
        };
        if !filter::long_enough(duration_ms) {
            return Ok(unclear);
        }
        let samples = resampled(pcm)?;
        let mut loaded = self.loaded.lock().unwrap_or_else(PoisonError::into_inner);
        if !loaded.contains_key(language) {
            let engine = self.load(language)?;
            loaded.insert(language.to_string(), engine);
        }
        let Some(engine) = loaded.get(language) else {
            return Ok(unclear);
        };
        let mut text = String::new();
        for range in segment::split(&samples, RATE) {
            let stream = engine.create_stream();
            stream.accept_waveform(rate_i32(), &samples[range]);
            engine.decode(&stream);
            if let Some(result) = stream.get_result() {
                text.push_str(result.text.trim());
            }
        }
        Ok(match filter::kept(&text) {
            Some(text) => Heard {
                text,
                clear: true,
                ..unclear
            },
            None => unclear,
        })
    }

    fn load(&self, language: &str) -> Result<OfflineRecognizer, Refused> {
        let config = OfflineRecognizerConfig {
            model_config: OfflineModelConfig {
                sense_voice: OfflineSenseVoiceModelConfig {
                    model: Some(self.model.model_file().display().to_string()),
                    language: Some(language.to_string()),
                    // 逆文本正则：数字写成阿拉伯数字、带标点。
                    use_itn: true,
                },
                tokens: Some(self.model.tokens_file().display().to_string()),
                num_threads: THREADS,
                // 查问题时设 MIYU_VOICE_DEBUG=1：sherpa-onnx 把收到的配置、每段的结果印在标准错误上。
                debug: std::env::var_os("MIYU_VOICE_DEBUG").is_some(),
                ..OfflineModelConfig::default()
            },
            ..OfflineRecognizerConfig::default()
        };
        OfflineRecognizer::create(&config)
            .ok_or_else(|| Refused::Engine("The speech model could not be loaded.".to_string()))
    }
}

/// 转成 16 kHz。
pub fn resampled(pcm: &Pcm) -> Result<Vec<f32>, Refused> {
    if pcm.rate == RATE {
        return Ok(pcm.samples.clone());
    }
    let rate = i32::try_from(pcm.rate)
        .map_err(|_| Refused::Engine(format!("Unsupported sample rate {}.", pcm.rate)))?;
    let resampler = LinearResampler::create(rate, rate_i32())
        .ok_or_else(|| Refused::Engine(format!("Cannot resample from {} Hz.", pcm.rate)))?;
    Ok(resampler.resample(&pcm.samples, true))
}

fn rate_i32() -> i32 {
    // 16000 放得进 i32。
    i32::try_from(RATE).unwrap_or(16_000)
}
