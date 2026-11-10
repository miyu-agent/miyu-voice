//! 跑着的助手（设计 20 第九节）：读配置、一直收着麦交给听的那一套；听到唤醒词（或者有人敲了 `miyu listen`、按了绑它的
//! 热键）响一声，唤醒词后面那句（或者唤醒以后 8 秒内说的下一句）交总线上的 `stt` 认成字，交给场所会话里的她，她的回答
//! 一句句念出来（[`crate::conversation`]）；念完 30 秒内接着说不用再叫她。
//!
//! 包装着、没停用就一直听唤醒词（项目主人 2026-11-10 定）：不想让它听的，在软件包里把它停用。`miyu listen` 不是开关，是
//! 「现在就叫她」，同喊了唤醒词。

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
use crate::conversation::Conversation;
use crate::keywords;
use crate::listen::{Event, Listener, RATE, Sensitivity};
use crate::models::Models;
use crate::reply::Reply;

/// 唤醒以后等下一句多久（只说了唤醒词的、敲了 `miyu listen` 的）：这么久还没开口就不等了，开了口的等这一句说完。
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
    /// 全双工（戴耳机时开）：她说话时也收麦，开口就打断她。
    pub full_duplex: bool,
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
            full_duplex: config
                .get("voice-assistant.full_duplex")
                .and_then(Value::as_bool)
                .unwrap_or(false),
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
    switches: Switches,
    speaker: Speaker,
    settings: Mutex<Settings>,
    conversation: Mutex<Option<Arc<Conversation>>>,
    reply: Mutex<Reply>,
}

impl Service {
    /// `cache` 是这个包的缓存目录。
    #[must_use]
    pub fn new(cache: PathBuf) -> Arc<Service> {
        Arc::new(Service {
            cache,
            switches: Switches::default(),
            speaker: Speaker::default(),
            settings: Mutex::new(Settings::from_config(&Map::new())),
            conversation: Mutex::new(None),
            reply: Mutex::new(Reply::default()),
        })
    }

    /// 登记给自己的命令行用的 `listen`（不上总线）。
    pub fn register(self: &Arc<Service>, ext: &mut miyu_ext::Extension) {
        let service = Arc::clone(self);
        ext.method_with(Method::new("listen"), move |_, _, core| {
            service.wake(core);
            Ok::<_, MethodError>(json!({}))
        });
        // 会话推来的事件（订阅了 events 流）：她的回答一句句念。
        let service = Arc::clone(self);
        ext.on("event", move |params, _| {
            if let Some(conversation) = service.conversation() {
                let mut reply = service.reply.lock().unwrap_or_else(PoisonError::into_inner);
                let session = params["session"].as_str().unwrap_or_default();
                conversation.event(&mut reply, session, &params["event"]);
            }
            Value::Null
        });
    }

    fn conversation(&self) -> Option<Arc<Conversation>> {
        self.conversation
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// 配置变了：重读（唤醒词变了的，下一次开麦时照新的建）。
    pub fn reload(&self, core: &Core) {
        let settings = Settings::from_config(&core.config());
        self.switches
            .full_duplex
            .store(settings.full_duplex, Ordering::SeqCst);
        *self.settings.lock().unwrap_or_else(PoisonError::into_inner) = settings;
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
        match Conversation::open(core, &self.switches) {
            Ok(conversation) => {
                *self
                    .conversation
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(conversation);
            }
            Err(why) => eprintln!("miyu-voice-assistant: {why}"),
        }
        let (utterances, heard) = mpsc::channel::<Vec<f32>>();
        self.understand_in_turn(core, heard);
        let service = Arc::clone(self);
        let barge_core = core.clone();
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
                // 敲了 `miyu listen`：同听到唤醒词、后面没跟话。
                if service.switches.wake.swap(false, Ordering::SeqCst) {
                    woken(&barge_core, &service.speaker);
                    awaiting = Some(Instant::now());
                }
                if awaiting.is_some_and(|since| since.elapsed() > AWAIT) && !listener.hearing() {
                    awaiting = None;
                }
                // 窗口：唤醒以后等下一句的 8 秒，她在想、在说，念完以后的追问窗口。
                let follow_up = service
                    .conversation()
                    .is_some_and(|conversation| conversation.window_open());
                listener.set_window(awaiting.is_some() || follow_up);
                for event in listener.feed(frame) {
                    match event {
                        Event::Wake { rest, .. } => {
                            woken(&barge_core, &service.speaker);
                            if rest.is_empty() {
                                awaiting = Some(Instant::now());
                            } else {
                                let _ = utterances.send(rest);
                            }
                        }
                        Event::Speech(audio) => {
                            awaiting = None;
                            let _ = utterances.send(audio);
                        }
                        // 她正念着、你开口了（只有全双工收得到）：打断。打断要等核心回，另开线程，不耽误收麦。
                        Event::Onset => {
                            if service.switches.speaking.load(Ordering::SeqCst)
                                && let Some(conversation) = service.conversation()
                            {
                                let core = barge_core.clone();
                                std::thread::spawn(move || conversation.barge_in(&core));
                            }
                        }
                    }
                }
            });
        });
    }

    /// 认字的那个线程：一句句交 `stt`、交给会话（认的时候收麦照常，不漏声音）。
    fn understand_in_turn(self: &Arc<Service>, core: &Core, heard: mpsc::Receiver<Vec<f32>>) {
        let worker_core = core.clone();
        let service = Arc::clone(self);
        std::thread::spawn(move || {
            // 量时延（施工 VO-13下）：设了 MIYU_VOICE_MEASURE 的，一句话认字、交给会话各用了多久印进日志。
            let measure = std::env::var_os("MIYU_VOICE_MEASURE").is_some();
            for audio in heard {
                let started = Instant::now();
                let Some(text) = understand(&worker_core, &audio) else {
                    continue;
                };
                let heard_at = started.elapsed();
                match service.conversation() {
                    Some(conversation) => {
                        if let Err(why) = conversation.say(&worker_core, &text) {
                            eprintln!("miyu-voice-assistant: {why}");
                        }
                        if measure {
                            eprintln!(
                                "miyu-voice-assistant: measure: {} ms of speech, stt {} ms, sent {} ms",
                                audio.len() * 1000 / RATE,
                                heard_at.as_millis(),
                                started.elapsed().as_millis()
                            );
                        }
                    }
                    None => {
                        eprintln!("miyu-voice-assistant: heard \"{text}\" but there is no session");
                    }
                }
            }
        });
    }
}

/// 叫醒的那一刻先让识别把模型加载起来：交一段 0.6 秒、几乎没声音的录音（过得了「短于 0.5 秒不认」，认不出字）。语音输入的
/// 识别在子进程里，闲了就退，叫醒以后头一句要先等它加载（施工 VO-13下实测：3 秒的话认了 2.1 秒，热的时候 0.2 秒）；你说那
/// 句话的工夫它就加载好了。没装、没起来的不管。
fn warm_up(core: &Core) {
    let hush: Vec<f32> = (0..RATE * 6 / 10)
        .map(|n| if n % 2 == 0 { 0.0001 } else { -0.0001 })
        .collect();
    let wav = audio::write_wav(&hush, u32::try_from(RATE).unwrap_or(16_000));
    if let Ok(blob) = blobs::put(core, &wav, "warm-up.wav", "audio/wav") {
        let _ = core.call(
            "bus.call",
            json!({"name": "stt", "method": "transcribe", "params": {"audio": blob}}),
        );
    }
}

/// 认一句：存成 wav 的 blob，交总线上的 `stt`；听清了交回字（第 3 步：太短的、没把握的不交）。
fn understand(core: &Core, audio: &[f32]) -> Option<String> {
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
        Ok(answer) if answer["clear"] == true => answer["text"].as_str().map(str::to_string),
        Ok(_) => None,
        Err(why) => {
            eprintln!("miyu-voice-assistant: stt: {why}");
            None
        }
    }
}

impl Service {
    /// `listen`（`miyu listen`、绑它的热键）：现在就叫她，同喊了唤醒词。她正念着的先停下、打断这一轮（同开口就打断）；
    /// 半双工下她停了才开麦，开了麦下一帧就当唤醒。
    fn wake(&self, core: &Core) {
        if self.switches.speaking.load(Ordering::SeqCst)
            && let Some(conversation) = self.conversation()
        {
            let core = core.clone();
            std::thread::spawn(move || conversation.barge_in(&core));
        }
        self.switches.wake.store(true, Ordering::SeqCst);
    }
}

/// 叫醒了（唤醒词、`miyu listen`）：响一声，识别先加载模型。
fn woken(core: &Core, speaker: &Speaker) {
    let core = core.clone();
    std::thread::spawn(move || warm_up(&core));
    chime::play(speaker, Chime::Wake);
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
