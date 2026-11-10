//! 总线上的 `stt`（设计 20 第三节「形状」，核心 `docs/reference/bus.md`）：`stt.transcribe`、`stt.record`、`stt.stop`。
//! 模型下好、校验过才登记（20 第五节；67 的 B8 照登记算 ready，头查到 ready 才画麦克风）。配置里选了社区的
//! `stt.engine`（`voice-input.engine` 写那个包的编号）的，转好 16 kHz 的 wav 交给它认，过滤还是这边做。

use std::path::PathBuf;
use std::sync::{Arc, PoisonError, RwLock};

use miyu_ext::{Core, Extension, Method, MethodError};
use serde_json::{Map, Value, json};
use voice_common::audio::{self, Pcm};
use voice_common::blobs;

use crate::filter;
use crate::recognize::{Heard, LANGUAGES, Refused};
use crate::record::{self, RecordError, Stopper};
use crate::worker::Workers;

/// 三个方法的全名。
pub const METHODS: [&str; 3] = ["stt.transcribe", "stt.record", "stt.stop"];

/// 包的配置（清单里的 `voice-input.*`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// 默认的识别语言。
    pub language: String,
    /// 麦克风的设备名；空的用默认。
    pub microphone: String,
    /// 社区 `stt.engine` 的包编号；空的用自带的 SenseVoice。
    pub engine: String,
}

impl Settings {
    /// 照核心交来的配置读。
    #[must_use]
    pub fn from_config(config: &Map<String, Value>) -> Settings {
        let text = |key: &str, default: &str| {
            config
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or(default)
                .trim()
                .to_string()
        };
        Settings {
            language: text("voice-input.language", "zh"),
            microphone: text("voice-input.microphone", ""),
            engine: text("voice-input.engine", ""),
        }
    }
}

/// 跑着的语音输入。
pub struct Service {
    settings: RwLock<Settings>,
    workers: Workers,
    stopper: Stopper,
}

impl Service {
    /// 模型在 `cache`。
    #[must_use]
    pub fn new(cache: PathBuf) -> Arc<Service> {
        Arc::new(Service {
            settings: RwLock::new(Settings::default()),
            workers: Workers::new(cache),
            stopper: Stopper::default(),
        })
    }

    /// 配置变了：重读。
    pub fn reload(&self, core: &Core) {
        *self
            .settings
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Settings::from_config(&core.config());
    }

    fn settings(&self) -> Settings {
        self.settings
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// 登记 `stt.*` 三个方法（先不登记，模型下好了 `enable_methods`）和配置页用的 `microphones`。
    pub fn register(self: &Arc<Service>, ext: &mut Extension) {
        let service = Arc::clone(self);
        ext.method_with(
            Method::new("stt.transcribe").timeout_ms(120_000).later(),
            move |params, _, core| service.transcribe(params, core),
        );
        let service = Arc::clone(self);
        ext.method_with(
            Method::new("stt.record").timeout_ms(310_000).later(),
            move |params, _, core| service.record(params, core),
        );
        // 配置页麦克风下拉的选项（施工 VO-21）：不上总线、不等模型，装着就答。
        ext.method_with(Method::new("microphones"), move |params, _, core| {
            only(params, &[])?;
            Ok(voice_common::mic::choices(&core.language()))
        });
        let service = Arc::clone(self);
        ext.method_with(
            Method::new("stt.stop").timeout_ms(5_000).later(),
            move |params, _, _| {
                only(params, &[])?;
                Ok(json!({"stopped": service.stopper.stop()}))
            },
        );
    }

    fn transcribe(&self, params: &Value, core: &Core) -> Result<Value, MethodError> {
        only(params, &["audio", "language"])?;
        let blob = params["audio"].as_str().ok_or_else(|| bad("Give audio."))?;
        let bytes = blobs::get(core, blob).map_err(|why| MethodError::new("bad_audio", why))?;
        let language = self.language(params)?;
        // 自带的引擎：wav 原样交给识别子进程，主进程不解码（子进程解不了的照 bad_audio 回）。
        if self.settings().engine.is_empty() {
            if let Err(why) = audio::check_wav(&bytes) {
                return Err(MethodError::new("bad_audio", why.0));
            }
            let heard = self
                .workers
                .transcribe_wav(&bytes, &language)
                .map_err(refused)?;
            return Ok(answer(&heard, None));
        }
        let pcm = audio::read_wav(&bytes).map_err(|why| MethodError::new("bad_audio", why.0))?;
        Ok(answer(&self.recognize(core, &pcm, &language)?, None))
    }

    fn record(&self, params: &Value, core: &Core) -> Result<Value, MethodError> {
        only(params, &["language", "max_ms", "silence_ms"])?;
        let language = self.language(params)?;
        let max_ms = number(params, "max_ms", 60_000, 1_000, 300_000)?;
        let silence_ms = number(params, "silence_ms", 1_200, 300, 5_000)?;
        let settings = self.settings();
        let microphone = Some(settings.microphone.as_str()).filter(|name| !name.is_empty());
        let (pcm, ended) =
            record::record(microphone, silence_ms, max_ms, &self.stopper).map_err(|error| {
                match error {
                    RecordError::MicBusy => {
                        MethodError::new("mic_busy", "Another dictation is using the microphone.")
                    }
                    RecordError::NoMicrophone(why) => MethodError::new("no_microphone", why),
                }
            })?;
        Ok(answer(
            &self.recognize(core, &pcm, &language)?,
            Some(ended.as_str()),
        ))
    }

    /// 认：自带的交识别子进程；选了社区引擎的交给它，过滤照样这边做。
    fn recognize(&self, core: &Core, pcm: &Pcm, language: &str) -> Result<Heard, MethodError> {
        let engine = self.settings().engine;
        if engine.is_empty() {
            return self.workers.transcribe(pcm, language).map_err(refused);
        }
        let duration_ms = pcm.duration_ms();
        if !filter::long_enough(duration_ms) {
            return Ok(Heard {
                text: String::new(),
                language: language.to_string(),
                duration_ms,
                clear: false,
            });
        }
        let samples = crate::recognize::resampled(pcm).map_err(refused)?;
        let wav = audio::write_wav(&samples, crate::recognize::RATE);
        let blob = blobs::put(core, &wav, "speech.wav", "audio/wav")
            .map_err(|error| MethodError::new("engine_failed", error.to_string()))?;
        let answer = core
            .call(
                "bus.call",
                json!({"name": "stt.engine", "package": engine, "method": "transcribe",
                       "params": {"audio": blob, "language": language}}),
            )
            .map_err(|error| MethodError::new("engine_failed", format!("{engine}: {error}")))?;
        let text = filter::kept(answer["text"].as_str().unwrap_or_default());
        Ok(Heard {
            clear: text.is_some(),
            text: text.unwrap_or_default(),
            language: language.to_string(),
            duration_ms,
        })
    }

    fn language(&self, params: &Value) -> Result<String, MethodError> {
        let language = params["language"]
            .as_str()
            .map_or_else(|| self.settings().language, str::to_string);
        if LANGUAGES.contains(&language.as_str()) {
            Ok(language)
        } else {
            Err(bad(&format!("Unknown language {language}.")))
        }
    }
}

fn answer(heard: &Heard, ended: Option<&str>) -> Value {
    let mut answer = json!({
        "text": heard.text,
        "language": heard.language,
        "duration_ms": heard.duration_ms,
        "clear": heard.clear,
    });
    if let Some(ended) = ended {
        answer["ended"] = ended.into();
    }
    answer
}

fn refused(refused: Refused) -> MethodError {
    match refused {
        Refused::TooLong => MethodError::new("too_long", "The audio is longer than 10 minutes."),
        Refused::Language(language) => bad(&format!("Unknown language {language}.")),
        Refused::Engine(why) => MethodError::new("engine_failed", why),
    }
}

/// 一个整数参数：没写用 `default`，要在 `min` 到 `max` 之间。
fn number(params: &Value, key: &str, default: u64, min: u64, max: u64) -> Result<u64, MethodError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(value) => value
            .as_u64()
            .filter(|n| (min..=max).contains(n))
            .ok_or_else(|| bad(&format!("{key} must be {min} to {max}."))),
    }
}

/// 参数里只能有这几格（不认识的报 `bad_params`）。
fn only(params: &Value, allowed: &[&str]) -> Result<(), MethodError> {
    let Some(fields) = params.as_object() else {
        return Ok(());
    };
    match fields.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(unknown) => Err(bad(&format!("Unknown field {unknown}."))),
        None => Ok(()),
    }
}

fn bad(message: &str) -> MethodError {
    MethodError::new("bad_params", message)
}

#[cfg(test)]
mod tests;
