//! 跑着的助手（设计 20 第九节）：读配置、一直收着麦交给听的那一套；听到唤醒词响一声，唤醒词后面那句（或者唤醒以后 8 秒内
//! 说的下一句）交总线上的 `stt` 认成字，交给场所会话里的她，她的回答一句句念出来（[`crate::conversation`]）；念完 30 秒内
//! 接着说不用再叫她。
//!
//! 包装着、没停用就一直听唤醒词（项目主人 2026-11-10 定）：不想让它听的，在软件包里把它停用。`miyu listen` 开关的是对话：
//! 开了同喊了唤醒词，响一声、等 30 秒你开口；关了响另一声，她还在想、在说的打断，回到只等唤醒词。对话自己也会结束：
//! 叫醒了一直没开口、她答完 30 秒没再说，响结束那一声、弹一条「对话结束」（施工 VO-18，主人：「什么时候off啊？」）。
//!
//! 配置改了当场生效（施工 VO-44，项目主人 2026-11-20 验收第 1、3 条）：唤醒词、门槛下一帧照新的重建听的那一套
//! （[`crate::ears`]），换了麦克风关了重开；换了人格、预设的，这一场对话完了另开一个语音会话（[`crate::choice`]），不打断正在说的。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use miyu_ext::{Core, Method, MethodError};
use serde_json::{Map, Value, json};
use voice_common::{audio, blobs, newest, speaker::Speaker};

use crate::capture::{self, Switches};
use crate::choice::Choice;
use crate::conversation::Conversation;
use crate::ears::{self, Ears, Tuning};
use crate::listen::{Event, Listener, RATE};
use crate::models::Models;
use crate::notify::{Notice, Notifier, Step};
use crate::reply::Reply;
use voice_common::chime::{self, Chime};

/// 喊了唤醒词以后等下一句多久（只说了唤醒词的）：这么久还没开口就不等了，开了口的等这一句说完。
const AWAIT: Duration = Duration::from_secs(8);
/// `miyu listen` 开了对话以后等多久：同她答完以后的追问窗口（敲完命令、按完热键要一会儿才开口）。
const AWAIT_COMMAND: Duration = crate::conversation::FOLLOW_UP;
/// 默认的唤醒词（项目主人 2026-11-20 验收第 2 条：只留「你好你好」；原来是 Miyu 的名字的几种念法）。
pub const DEFAULT_WAKE_WORDS: [&str; 1] = ["你好你好"];
/// 切出来、等着认字的人声最多留几段（施工 VO-49）：认字那头卡住的，再多的丢最旧的。
const UTTERANCES_KEPT: usize = 30;

/// 包的配置（清单里的 `voice-assistant.*`）。
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// 唤醒词。
    pub wake_words: Vec<String>,
    /// 灵敏度：唤醒词判中的门槛。
    pub threshold: f32,
    /// 麦克风的设备名；空的用默认。
    pub microphone: String,
    /// 说话时可以打断 AI（默认开）：她说话时也收麦（过回声消除），你一开口她就停。
    pub full_duplex: bool,
    /// 语音会话用的人格、预设（施工 VO-44）。
    pub choice: Choice,
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
                .unwrap_or(true),
            microphone: config
                .get("voice-assistant.microphone")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string(),
            choice: Choice::from_config(config),
        }
    }
}

/// 跑着的助手。
pub struct Service {
    cache: PathBuf,
    /// 包的状态目录（`MIYU_STATE_DIR`）：记语音会话上一次照哪个人格、预设开的。
    state: Option<PathBuf>,
    switches: Switches,
    speaker: Speaker,
    notifier: Arc<Notifier>,
    settings: Mutex<Settings>,
    conversation: Mutex<Option<Arc<Conversation>>>,
    /// 现在的语音会话照哪一份人格、预设开的（还没开的是空的）。
    applied: Mutex<Option<Choice>>,
    /// 人格、预设改了，还没另开会话（等这一场对话完了）。
    reopen_session: AtomicBool,
    /// 一次只开一个会话。
    opening: Mutex<()>,
    reply: Mutex<Reply>,
}

impl Service {
    /// `cache` 是这个包的缓存目录，`state` 是状态目录。
    #[must_use]
    pub fn new(cache: PathBuf, state: Option<PathBuf>) -> Arc<Service> {
        let switches = Switches::default();
        switches.full_duplex.store(true, Ordering::SeqCst);
        Arc::new(Service {
            cache,
            state,
            // 助手放的声音（提示音、她念的话）都让回声消除旁听（施工 VO-20）。
            speaker: Speaker::tapped(switches.played.tap()),
            switches,
            notifier: Arc::new(Notifier::start()),
            settings: Mutex::new(Settings::from_config(&Map::new())),
            conversation: Mutex::new(None),
            applied: Mutex::new(None),
            reopen_session: AtomicBool::new(false),
            opening: Mutex::new(()),
            reply: Mutex::new(Reply::default()),
        })
    }

    /// 登记给自己的命令行用的 `listen`（不上总线）。
    pub fn register(self: &Arc<Service>, ext: &mut miyu_ext::Extension) {
        let service = Arc::clone(self);
        ext.method_with(Method::new("listen"), move |params, _, core| {
            service.switch(params, core)
        });
        // 配置页麦克风下拉的选项（施工 VO-21）。
        ext.method_with(Method::new("microphones"), |_, _, core| {
            Ok(voice_common::mic::choices(&core.language()))
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

    fn settings(&self) -> Settings {
        self.settings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// 配置变了：重读。唤醒词、门槛收麦的线程下一帧就照新的（[`Ears`]）；换了麦克风的关了重开；换了人格、预设的另开会话，
    /// 正在对话的等这一场完了。
    pub fn reload(self: &Arc<Service>, core: &Core) {
        self.notifier.set_language(&core.language());
        let settings = Settings::from_config(&core.config());
        self.switches
            .full_duplex
            .store(settings.full_duplex, Ordering::SeqCst);
        let before = std::mem::replace(
            &mut *self.settings.lock().unwrap_or_else(PoisonError::into_inner),
            settings.clone(),
        );
        if before.microphone != settings.microphone {
            self.switches.reopen.store(true, Ordering::SeqCst);
        }
        let applied = self
            .applied
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if applied.is_some_and(|applied| applied != settings.choice) {
            self.reopen_session.store(true, Ordering::SeqCst);
            self.session_between_conversations(core);
        }
    }

    /// 人格、预设改过、现在没在对话：另开会话（另开线程，不耽误收麦、配置推送）。
    fn session_between_conversations(self: &Arc<Service>, core: &Core) {
        let talking = self.switches.open.load(Ordering::SeqCst)
            || self
                .conversation()
                .is_some_and(|conversation| conversation.window_open());
        if !talking && self.reopen_session.swap(false, Ordering::SeqCst) {
            let service = Arc::clone(self);
            let core = core.clone();
            std::thread::spawn(move || service.open_session(&core));
        }
    }

    /// 照现在的人格、预设找回或者另开语音会话：和上一次照着开的（[`Choice::load`]）不一样的另开一个（`fresh`）。开好的换上、
    /// 记下，原来的退订；开不了的说一声，原来的接着用。
    fn open_session(&self, core: &Core) {
        let _one = self.opening.lock().unwrap_or_else(PoisonError::into_inner);
        let choice = self.settings().choice;
        let last = self
            .applied
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .or_else(|| self.state.as_deref().map(Choice::load))
            .unwrap_or_default();
        let fresh = choice != last;
        if !fresh && self.conversation().is_some() {
            return;
        }
        let opened = Conversation::open(
            core,
            &choice,
            fresh,
            &self.switches,
            &self.notifier,
            &self.speaker,
        );
        match opened {
            Ok(conversation) => {
                if let Some(Err(why)) = self.state.as_deref().map(|dir| choice.save(dir)) {
                    eprintln!("miyu-voice-assistant: {why}");
                }
                *self.applied.lock().unwrap_or_else(PoisonError::into_inner) = Some(choice);
                let old = self
                    .conversation
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .replace(conversation);
                if let Some(old) = old {
                    old.unsubscribe(core);
                }
            }
            Err(why) => {
                eprintln!("miyu-voice-assistant: {why}");
                self.notifier.show(Notice::Trouble(Step::Sending, why));
            }
        }
    }

    /// 起来以后：下模型，建听的那一套，一直收着麦（另开线程）；认字也另开线程，免得认的时候漏了麦克风的声音。
    pub fn start(self: &Arc<Service>, core: &Core) {
        let models = Models::in_cache(&self.cache);
        if let Err(why) = models.ensure() {
            eprintln!("miyu-voice-assistant: {why}");
            return;
        }
        // 听的那一套（两个模型，约 40 MB）第一次真的开麦收到声音时才建：装上没开的助手只占几 MB。
        let mut ears = Ears::<Listener>::default();
        self.open_session(core);
        let (mut utterances, heard) = utterance_queue();
        self.understand_in_turn(core, heard);
        let service = Arc::clone(self);
        let barge_core = core.clone();
        std::thread::spawn(move || {
            // 等下一句：从什么时候起、等多久。
            let mut awaiting = None::<(Instant, Duration)>;
            let mut was_open = false;
            // 每次开麦时问现在配置的是哪个麦克风。
            let device = || Some(service.settings().microphone).filter(|name| !name.is_empty());
            let say = |notice| service.notifier.show(notice);
            capture::run(&device, &service.switches, &say, |frame| {
                let tuning = Tuning::of(&service.settings());
                let Some(listener) = ears.tuned(&tuning, |tuning| ears::build(&models, tuning))
                else {
                    return;
                };
                let switches = &service.switches;
                if switches.wake.swap(false, Ordering::SeqCst) {
                    awaiting = Some((Instant::now(), AWAIT_COMMAND));
                }
                if switches.ended.swap(false, Ordering::SeqCst)
                    || awaiting.is_some_and(|(since, wait)| since.elapsed() > wait)
                        && !listener.hearing()
                {
                    awaiting = None;
                }
                // 窗口：叫醒以后等下一句，她在想、在说，念完以后的追问窗口。
                let follow_up = service
                    .conversation()
                    .is_some_and(|conversation| conversation.window_open());
                let open = awaiting.is_some() || follow_up;
                switches.open.store(open, Ordering::SeqCst);
                listener.set_window(open);
                // 对话自己结束了（`miyu listen off` 关的那边已经响过、说过）。
                if was_open && !open {
                    if !switches.silenced.swap(false, Ordering::SeqCst) {
                        chime::play(&service.speaker, Chime::Off);
                        service.notifier.show(Notice::ConversationOff);
                    }
                    // 对话时改了人格、预设的，这会儿另开会话。
                    service.session_between_conversations(&barge_core);
                }
                was_open = open;
                for event in listener.feed(frame) {
                    match event {
                        Event::Wake { rest, .. } => {
                            woken(&barge_core, &service.speaker);
                            service.notifier.show(Notice::Listening);
                            // 唤醒词后面紧跟着说了的，照样留一段窗口，等认字、交给她、她开始答。
                            awaiting = Some((Instant::now(), AWAIT));
                            if !rest.is_empty() {
                                utterances.send(rest);
                            }
                        }
                        // 一句：交去认字。窗口再留一会儿，认字、交给她这段不关；没听清的这会儿里再说一次就行。
                        Event::Speech(audio) => {
                            awaiting = Some((Instant::now(), AWAIT));
                            utterances.send(audio);
                        }
                        // 她正念着、你开口了：打断。麦克风里主要是她自己的声音时（回声没消干净）不算。打断要等核心回，
                        // 另开线程，不耽误收麦。
                        Event::Onset => {
                            if service.switches.speaking.load(Ordering::SeqCst)
                                && !service.switches.mostly_echo.load(Ordering::SeqCst)
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
    fn understand_in_turn(self: &Arc<Service>, core: &Core, heard: newest::Receiver<Vec<f32>>) {
        let worker_core = core.clone();
        let service = Arc::clone(self);
        std::thread::spawn(move || {
            // 量时延（施工 VO-13下）：设了 MIYU_VOICE_MEASURE 的，一句话认字、交给会话各用了多久印进日志。
            let measure = std::env::var_os("MIYU_VOICE_MEASURE").is_some();
            while let Some(audio) = heard.recv() {
                let started = Instant::now();
                let notifier = &service.notifier;
                let text = match understand(&worker_core, &audio) {
                    Ok(Some(text)) => text,
                    Ok(None) => {
                        notifier.show(Notice::NotHeard);
                        continue;
                    }
                    Err(why) => {
                        eprintln!("miyu-voice-assistant: stt: {why}");
                        notifier.show(Notice::Trouble(Step::Recognition, why));
                        continue;
                    }
                };
                let heard_at = started.elapsed();
                let Some(conversation) = service.conversation() else {
                    let why = format!("heard \"{text}\" but there is no session");
                    eprintln!("miyu-voice-assistant: {why}");
                    notifier.show(Notice::Trouble(Step::Sending, why));
                    continue;
                };
                notifier.show(Notice::Heard(text.clone()));
                if let Err(why) = conversation.say(&worker_core, &text) {
                    eprintln!("miyu-voice-assistant: {why}");
                    notifier.show(Notice::Trouble(Step::Sending, why));
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
        });
    }
}

/// 收麦线程交给认字线程的那一条：最多 [`UTTERANCES_KEPT`] 段。
fn utterance_queue() -> (newest::Sender<Vec<f32>>, newest::Receiver<Vec<f32>>) {
    newest::channel(UTTERANCES_KEPT, |_| 1, "miyu-voice-assistant: utterances")
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

/// 认一句：存成 wav 的 blob，交总线上的 `stt`；听清了交回字，太短的、没把握的交回 `None`（第 3 步：不交给她）。
///
/// # Errors
///
/// 存不了、`stt` 拒了（没装、没起来、出错）：为什么。
fn understand(core: &Core, audio: &[f32]) -> Result<Option<String>, String> {
    let wav = audio::write_wav(audio, u32::try_from(RATE).unwrap_or(16_000));
    let heard = blobs::put(core, &wav, "heard.wav", "audio/wav")
        .map_err(|error| error.to_string())
        .and_then(|blob| {
            core.call(
                "bus.call",
                json!({"name": "stt", "method": "transcribe", "params": {"audio": blob}}),
            )
            .map_err(|error| voice_common::refusal::why(&error))
        });
    heard.map(|answer| {
        (answer["clear"] == true)
            .then(|| answer["text"].as_str().map(str::to_string))
            .flatten()
    })
}

impl Service {
    /// `listen {to: on|off|toggle}`（`miyu listen`、绑它的热键）：开、关对话，交回现在开没开。开了同喊了唤醒词（响一声，
    /// 等 30 秒你开口）；在对话里（喊唤醒词叫来的也算）切一下是关：响另一声，收掉窗口，她还在想、在说的打断这一轮。
    fn switch(&self, params: &Value, core: &Core) -> Result<Value, MethodError> {
        let switches = &self.switches;
        let now = switches.open.load(Ordering::SeqCst);
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
        if on && !now {
            switches.wake.store(true, Ordering::SeqCst);
            switches.open.store(true, Ordering::SeqCst);
            woken(core, &self.speaker);
            self.notifier.show(Notice::ConversationOn);
        } else if !on && now {
            switches.silenced.store(true, Ordering::SeqCst);
            switches.ended.store(true, Ordering::SeqCst);
            switches.open.store(false, Ordering::SeqCst);
            if let Some(conversation) = self.conversation() {
                let core = core.clone();
                std::thread::spawn(move || conversation.close(&core));
            }
            chime::play(&self.speaker, Chime::Off);
            self.notifier.show(Notice::ConversationOff);
        }
        Ok(json!({"listening": on}))
    }
}

/// 叫醒了（唤醒词、`miyu listen` 开了对话）：响一声，识别先加载模型。
fn woken(core: &Core, speaker: &Speaker) {
    let core = core.clone();
    std::thread::spawn(move || warm_up(&core));
    chime::play(speaker, Chime::Wake);
}

#[cfg(test)]
mod tests;
