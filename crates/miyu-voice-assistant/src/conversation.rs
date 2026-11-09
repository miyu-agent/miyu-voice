//! 和她说话（设计 20 第九节「一个本机场所」「规程」第 3～5 步）：助手是 `voice` 平台的桥（清单 `[connection] platform`），
//! 场所 `voice:local` 里只有主人一个人（核心 VA-1：`voice` 平台的 `owner` 天然算主人，core1 11-10 定 A）。会话一直续下去，
//! 终端、网页里看得到；`images=false`（说话看不到图）。
//!
//! 听到的一句照主人说话交过去（`session.send` 带 `as`）；她的回答照会话推来的增量一句句念（[`crate::reply`]），念交总线上的
//! `tts.play`；念的时候半双工不收麦（第 7 步）。一轮完了、念完了，开 30 秒追问窗口（第 5 步）：窗口开着说话不用再叫她；
//! 她想、说的时候窗口一直开着，不计时。总线上没有 `tts`（没装、没配朗读）的只听不说，回答照样在各个头里看得到。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use miyu_ext::Core;
use serde_json::{Value, json};

use crate::capture::Switches;
use crate::reply::Reply;

/// 场所编号。
pub const VENUE: &str = "voice:local";
/// 主人在这个平台上的身份：带平台前缀（core1 11-10，VA-1：核心只认 `voice:` 场所里的 `voice:owner` 是主人）。
pub const OWNER: &str = "voice:owner";
/// 念完以后追问窗口开多久（第 5 步）。
pub const FOLLOW_UP: Duration = Duration::from_secs(30);

/// 告诉她这一场是念出来的（第九节「告诉她这一轮是念出来的」）：客观写明，不指使；记在 `docs/prompt-ledger.md`。
const SPOKEN_FACT: &str = "This conversation is spoken: what you write is read aloud to the person, and they don't see the text.";

/// 窗口：开着的时候说话不用唤醒词。
#[derive(Debug, Default)]
pub struct Window {
    /// 她这一轮还在想、在说。
    in_turn: bool,
    /// 念完以后开到什么时候。
    until: Option<Instant>,
}

impl Window {
    /// 现在开着没有。
    #[must_use]
    pub fn open(&self) -> bool {
        self.in_turn || self.until.is_some_and(|until| Instant::now() < until)
    }
}

/// 一场谈话：会话编号、窗口、要念的句子。
pub struct Conversation {
    session: String,
    window: Arc<Mutex<Window>>,
    sentences: mpsc::Sender<Spoken>,
    sent: AtomicU64,
}

/// 交给念的那个线程的。
enum Spoken {
    Sentence(String),
    /// 这一轮完了：念完剩下的就开窗口。
    TurnEnded,
}

impl Conversation {
    /// 找回或者造这个场所的会话，订阅它的事件；第一次造的记一条「这一场是念出来的」。
    ///
    /// # Errors
    ///
    /// 核心拒了（没批 `act_for_external`、`events.read`）。
    pub fn open(core: &Core, switches: &Switches) -> Result<Arc<Conversation>, String> {
        let opened = core
            .call(
                "venue.session",
                json!({"venue": VENUE, "kind": "private", "peer": OWNER, "images": false}),
            )
            .map_err(|error| format!("venue.session: {error}"))?;
        let session = opened["session"].as_str().unwrap_or_default().to_string();
        if opened["created"] == true {
            core.call(
                "session.note",
                json!({"session": session, "facts": [{"kind": "voice", "text": SPOKEN_FACT}]}),
            )
            .map_err(|error| format!("session.note: {error}"))?;
        }
        core.call("subscribe", json!({"stream": "events", "session": session}))
            .map_err(|error| format!("subscribe: {error}"))?;
        let window = Arc::new(Mutex::new(Window::default()));
        let (sentences, queue) = mpsc::channel();
        speak_in_turn(core.clone(), switches.clone(), Arc::clone(&window), queue);
        Ok(Arc::new(Conversation {
            session,
            window,
            sentences,
            sent: AtomicU64::new(0),
        }))
    }

    /// 窗口现在开着没有（收麦的线程照它决定要不要唤醒词）。
    #[must_use]
    pub fn window_open(&self) -> bool {
        self.window
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .open()
    }

    /// 听到的一句交给她（照主人说的）。她正忙的，核心照回合中途来的消息排（第 6 步后半）。
    ///
    /// # Errors
    ///
    /// 核心拒了。
    pub fn say(&self, core: &Core, text: &str) -> Result<(), String> {
        let n = self.sent.fetch_add(1, Ordering::Relaxed);
        let msg = format!("{}-{n}", std::process::id());
        core.call(
            "session.send",
            json!({"session": self.session, "text": text,
                   "as": {"external": OWNER}, "venue": {"msg": msg}}),
        )
        .map(drop)
        .map_err(|error| format!("session.send: {error}"))
    }

    /// 会话推来的一条事件：她的回答一句句交出去念；一轮开始、结束照着开关窗口。
    pub fn event(&self, reply: &mut Reply, session: &str, event: &Value) {
        if session != self.session {
            return;
        }
        let body = &event["body"];
        match event["kind"].as_str() {
            Some("turn.started") => {
                let mut window = self.window.lock().unwrap_or_else(PoisonError::into_inner);
                window.in_turn = true;
            }
            Some("model.delta") => {
                let index = body["index"].as_u64().unwrap_or(0);
                if let Some(kind) = body["start"].as_str() {
                    reply.start(index, kind);
                } else if let Some(text) = body["text"].as_str()
                    && let Some(sentence) = reply.text(index, text)
                {
                    let _ = self.sentences.send(Spoken::Sentence(sentence));
                }
            }
            // 一次模型响应写完：块的编号下一次从头数。
            Some("message.assistant") => {
                if let Some(rest) = reply.finish() {
                    let _ = self.sentences.send(Spoken::Sentence(rest));
                }
            }
            Some("turn.ended") => {
                if let Some(rest) = reply.finish() {
                    let _ = self.sentences.send(Spoken::Sentence(rest));
                }
                let _ = self.sentences.send(Spoken::TurnEnded);
            }
            _ => {}
        }
    }
}

/// 念的那个线程：一句句交总线上的 `tts.play`，念的时候半双工不收麦；一轮完了、念完了开追问窗口。
fn speak_in_turn(
    core: Core,
    switches: Switches,
    window: Arc<Mutex<Window>>,
    queue: mpsc::Receiver<Spoken>,
) {
    std::thread::spawn(move || {
        let mut mute = false;
        for spoken in queue {
            match spoken {
                Spoken::Sentence(sentence) => {
                    if mute {
                        continue;
                    }
                    switches.speaking.store(true, Ordering::SeqCst);
                    let played = core.call(
                        "bus.call",
                        json!({"name": "tts", "method": "play", "params": {"text": sentence}}),
                    );
                    switches.speaking.store(false, Ordering::SeqCst);
                    if let Err(error) = played {
                        // 没装、没配朗读：只听不说，说一次就不再试（下一轮重新试，配好了就能念）。
                        eprintln!("miyu-voice-assistant: tts: {error}");
                        mute = true;
                    }
                }
                Spoken::TurnEnded => {
                    mute = false;
                    let mut window = window.lock().unwrap_or_else(PoisonError::into_inner);
                    window.in_turn = false;
                    window.until = Some(Instant::now() + FOLLOW_UP);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests;
