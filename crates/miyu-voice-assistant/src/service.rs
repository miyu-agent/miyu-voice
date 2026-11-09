//! 跑着的助手（设计 20 第九节）：读配置、开关（`miyu listen`，记在包的状态目录里，重启照旧）、一直收着麦交给听的那一套；
//! 听到唤醒词响一声，唤醒词后面那句（或者唤醒以后 8 秒内说的下一句）交总线上的 `stt` 认成字。认出来的字交给会话、念回答
//! 等核心的 VA-1（施工 VO-11下）。
//!
//! 装上默认是关着的：一直开着的麦克风不该因为装了一个包就自己打开，主人敲 `miyu listen` 才开。

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use miyu_ext::{Core, Method, MethodError};
use serde_json::{Map, Value, json};
use voice_common::{audio, blobs, speaker::Speaker};

use crate::capture::{self, Switches};
use crate::chime::{self, Chime};
use crate::keywords;
use crate::listen::{Event, Listener, RATE, Sensitivity};
use crate::models::Models;

/// 唤醒以后等下一句多久（只说了唤醒词的）。
const AWAIT: Duration = Duration::from_secs(8);
/// 默认的唤醒词（设计 20 第九节：Miyu 的名字的几种念法）。
pub const DEFAULT_WAKE_WORDS: [&str; 4] = ["未有未有", "密友密友", "miyumiyu", "みゆみゆ"];

/// 包的配置（清单里的 `voice-assistant.*`）。
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// 唤醒词。
    pub wake_words: Vec<String>,
    /// 灵敏度：唤醒词判中的门槛。
    pub threshold: f32,
    /// 麦克风的设备名；空的用默认。
    pub microphone: String,
}

impl Settings {
    /// 照核心交来的配置读。
    #[must_use]
    pub fn from_config(config: &Map<String, Value>) -> Settings {
        let wake_words: Vec<String> = config
            .get("voice-assistant.wake_words")
            .and_then(Value::as_array)
            .map(|words| {
                words
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|word| !word.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .filter(|words: &Vec<String>| !words.is_empty())
            .unwrap_or_else(|| DEFAULT_WAKE_WORDS.map(str::to_string).to_vec());
        #[expect(clippy::cast_possible_truncation, reason = "门槛在 0 到 1 之间")]
        let threshold = config
            .get("voice-assistant.sensitivity")
            .and_then(Value::as_f64)
            .map_or(0.25, |value| value as f32);
        Settings {
            wake_words,
            threshold,
            microphone: config
                .get("voice-assistant.microphone")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string(),
        }
    }
}

/// 跑着的助手。
pub struct Service {
    cache: PathBuf,
    state: PathBuf,
    switches: Switches,
    speaker: Speaker,
    settings: Mutex<Settings>,
}

impl Service {
    /// `cache`、`state` 是这个包的缓存、状态目录。
    #[must_use]
    pub fn new(cache: PathBuf, state: PathBuf) -> Arc<Service> {
        let on =
            std::fs::read_to_string(state.join("listening")).is_ok_and(|text| text.trim() == "on");
        let service = Service {
            cache,
            state,
            switches: Switches::default(),
            speaker: Speaker::default(),
            settings: Mutex::new(Settings::from_config(&Map::new())),
        };
        service.switches.listening.store(on, Ordering::SeqCst);
        Arc::new(service)
    }

    /// 登记给自己的命令行用的 `listen`（不上总线）。
    pub fn register(self: &Arc<Service>, ext: &mut miyu_ext::Extension) {
        let service = Arc::clone(self);
        ext.method_with(Method::new("listen"), move |params, _, _| {
            service.switch(params)
        });
    }

    /// 配置变了：重读（唤醒词变了的，下一次开麦时照新的建）。
    pub fn reload(&self, core: &Core) {
        *self.settings.lock().unwrap_or_else(PoisonError::into_inner) =
            Settings::from_config(&core.config());
    }

    /// 起来以后：下模型，建听的那一套，一直收着麦（另开线程）；认字也另开线程，免得认的时候漏了麦克风的声音。
    pub fn start(self: &Arc<Service>, core: &Core) {
        let models = Models::in_cache(&self.cache);
        if let Err(why) = models.ensure() {
            eprintln!("miyu-voice-assistant: {why}");
            return;
        }
        let settings = self
            .settings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        // 听的那一套（两个模型，约 40 MB）第一次真的开麦收到声音时才建：装上没开的助手只占几 MB。
        let mut listener: Option<Listener> = None;
        let mut unbuildable = false;
        let (utterances, heard) = mpsc::channel::<Vec<f32>>();
        let worker_core = core.clone();
        std::thread::spawn(move || {
            for audio in heard {
                understand(&worker_core, &audio);
            }
        });
        let service = Arc::clone(self);
        let microphone = Some(settings.microphone.clone()).filter(|name| !name.is_empty());
        std::thread::spawn(move || {
            let mut awaiting = None::<Instant>;
            capture::run(microphone.as_deref(), &service.switches, |frame| {
                if listener.is_none() && !unbuildable {
                    match build_listener(&models, &settings) {
                        Ok(built) => listener = Some(built),
                        Err(why) => {
                            eprintln!("miyu-voice-assistant: {why}");
                            unbuildable = true;
                        }
                    }
                }
                let Some(listener) = listener.as_mut() else {
                    return;
                };
                if awaiting.is_some_and(|since| since.elapsed() > AWAIT) {
                    awaiting = None;
                    listener.set_window(false);
                }
                for event in listener.feed(frame) {
                    match event {
                        Event::Wake { rest, .. } => {
                            chime::play(&service.speaker, Chime::Wake);
                            if rest.is_empty() {
                                awaiting = Some(Instant::now());
                                listener.set_window(true);
                            } else {
                                let _ = utterances.send(rest);
                            }
                        }
                        Event::Speech(audio) => {
                            awaiting = None;
                            listener.set_window(false);
                            let _ = utterances.send(audio);
                        }
                    }
                }
            });
        });
    }
}

/// 认一句：存成 wav 的 blob，交总线上的 `stt`。认出来的交给会话等 VA-1；现在记进日志。
fn understand(core: &Core, audio: &[f32]) {
    let wav = audio::write_wav(audio, u32::try_from(RATE).unwrap_or(16_000));
    let heard = blobs::put(core, &wav, "heard.wav", "audio/wav")
        .map_err(|error| error.to_string())
        .and_then(|blob| {
            core.call(
                "bus.call",
                json!({"name": "stt", "method": "transcribe", "params": {"audio": blob}}),
            )
            .map_err(|error| error.to_string())
        });
    match heard {
        Ok(answer) if answer["clear"] == true => {
            eprintln!(
                "miyu-voice-assistant: heard \"{}\"",
                answer["text"].as_str().unwrap_or_default()
            );
        }
        Ok(_) => eprintln!("miyu-voice-assistant: did not catch that"),
        Err(why) => eprintln!("miyu-voice-assistant: stt: {why}"),
    }
}

impl Service {
    /// `listen {to: on|off|toggle}`：开关，记下来，响一声，交回现在开没开。
    fn switch(&self, params: &Value) -> Result<Value, MethodError> {
        let now = self.switches.listening.load(Ordering::SeqCst);
        let on = match params["to"].as_str() {
            None | Some("toggle") => !now,
            Some("on") => true,
            Some("off") => false,
            Some(other) => {
                return Err(MethodError::new(
                    "bad_params",
                    format!("Unknown switch {other}."),
                ));
            }
        };
        self.switches.listening.store(on, Ordering::SeqCst);
        let _ = std::fs::write(
            self.state.join("listening"),
            if on { "on\n" } else { "off\n" },
        );
        chime::play(&self.speaker, if on { Chime::On } else { Chime::Off });
        Ok(json!({"listening": on}))
    }
}

/// 照配置的唤醒词建听的那一套：编不了的跳过（说一句），一个都编不了的不建。
fn build_listener(models: &Models, settings: &Settings) -> Result<Listener, String> {
    let [.., tokens] = models.kws();
    let vocabulary =
        keywords::tokens(&std::fs::read_to_string(tokens).map_err(|error| error.to_string())?);
    let mut lines = Vec::new();
    for word in &settings.wake_words {
        match keywords::encode(word, &vocabulary) {
            Ok(encoded) => lines.extend(encoded),
            Err(why) => eprintln!(
                "miyu-voice-assistant: wake word \"{word}\" skipped: {}",
                why.0
            ),
        }
    }
    Listener::new(
        models,
        &lines,
        Sensitivity {
            threshold: settings.threshold,
            ..Sensitivity::default()
        },
    )
}

#[cfg(test)]
mod tests;
