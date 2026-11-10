//! 和她说话（设计 20 第九节「一个本机场所」「规程」第 3～5 步）：助手是 `voice` 平台的桥（清单 `[connection] platform`），
//! 场所 `voice:local` 里只有主人一个人（核心 VA-1：`voice` 平台的 `owner` 天然算主人，core1 11-10 定 A）。会话一直续下去，
//! 终端、网页里看得到；`images=false`（说话看不到图）。
//!
//! 听到的一句照主人说话交过去（`session.send` 带 `as`）；她的回答照会话推来的增量一句句念（[`crate::reply`]）：一句句交
//! 总线上的 `tts.synthesize`，放这一句时下一句已经在合，合好的交 `tts.play`；念的时候半双工不收麦（第 7 步）。一轮完了、念完了，开 30 秒追问窗口（第 5 步）：窗口开着说话不用再叫她；
//! 她想、说的时候窗口一直开着，不计时。总线上没有 `tts`（没装、没配朗读）的只听不说，回答照样在各个头里看得到。
//!
//! 开口就打断（第 6 步，施工 VO-12）：戴耳机开了全双工的，她念的时候你一开口，排着没念的丢掉、`tts.stop` 停掉正在念的、
//! 打断这一轮，会话里补一块事实写明你听到哪一句为止、后面的没念出来（她下一轮知道你没听到后半截）。

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use miyu_ext::Core;
use serde_json::{Value, json};

use crate::capture::Switches;
use crate::notify::{Notice, Notifier, Step};
use crate::reply::Reply;

/// 场所编号。
pub const VENUE: &str = "voice:local";
/// 主人在这个平台上的身份：带平台前缀（core1 11-10，VA-1：核心只认 `voice:` 场所里的 `voice:owner` 是主人）。
pub const OWNER: &str = "voice:owner";
/// 念完以后追问窗口开多久（第 5 步）。
pub const FOLLOW_UP: Duration = Duration::from_secs(30);

/// 打断时补的事实，`{heard}` 换成念完的最后一句（一句没念完的写 nothing）。记在 `docs/prompt-ledger.md`。
const INTERRUPTED_FACT: &str = "The person cut in while you were speaking. They heard your reply up to: {heard} The rest was not read aloud.";

/// 告诉她这一场是念出来的（第九节「告诉她这一轮是念出来的」）：客观写明，不指使；记在 `docs/prompt-ledger.md`。
const SPOKEN_FACT: &str = "This conversation is spoken: what you write is read aloud to the person, and they don't see the text.";

/// 窗口：开着的时候说话不用唤醒词。
#[derive(Debug, Default)]
pub struct Window {
    /// 她这一轮还在想、在说。
    in_turn: bool,
    /// 念完以后开到什么时候。
    until: Option<Instant>,
    /// 这一轮半路关了对话（`miyu listen off`）：这一轮完了不开追问窗口。
    closed: bool,
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
    /// 打断一次加一：排着的句子带着交出去时的数，对不上的不念。
    generation: Arc<AtomicU64>,
    /// 这一轮念完了的句子。
    played: Arc<Mutex<Vec<String>>>,
    /// 这一轮她说了的（一句句交出去念的连起来）：通知里跟着写。
    said: Mutex<String>,
    notifier: Arc<Notifier>,
}

/// 交给念的那个线程的。
enum Spoken {
    /// 一句，和交出去时的打断次数。
    Sentence(String, u64),
    /// 这一轮完了：念完剩下的就开窗口。
    TurnEnded,
}

impl Conversation {
    /// 找回或者造这个场所的会话，订阅它的事件；第一次造的记一条「这一场是念出来的」。
    ///
    /// # Errors
    ///
    /// 核心拒了（没批 `act_for_external`、`events.read`）。
    pub fn open(
        core: &Core,
        switches: &Switches,
        notifier: &Arc<Notifier>,
    ) -> Result<Arc<Conversation>, String> {
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
        let subscribed = core
            .call("subscribe", json!({"stream": "events", "session": session}))
            .map_err(|error| format!("subscribe: {error}"))?;
        // 照会话的人格念（朗读的配置里每个人格一种音色，设计 20 第六节）；没有人格的用默认音色。
        let persona = subscribed["persona"].as_str().map(str::to_string);
        let window = Arc::new(Mutex::new(Window::default()));
        let (sentences, queue) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let played = Arc::new(Mutex::new(Vec::new()));
        speak_in_turn(
            core.clone(),
            switches.clone(),
            Turn {
                window: Arc::clone(&window),
                generation: Arc::clone(&generation),
                played: Arc::clone(&played),
                persona,
            },
            queue,
            Arc::clone(notifier),
        );
        Ok(Arc::new(Conversation {
            session,
            window,
            sentences,
            sent: AtomicU64::new(0),
            generation,
            played,
            said: Mutex::new(String::new()),
            notifier: Arc::clone(notifier),
        }))
    }

    /// 关了对话（`miyu listen off`）：追问窗口收掉；她还在想、在说的，打断这一轮（同开口就打断），完了也不再开窗口。
    pub fn close(&self, core: &Core) {
        let in_turn = {
            let mut window = self.window.lock().unwrap_or_else(PoisonError::into_inner);
            window.until = None;
            window.closed = window.in_turn;
            window.in_turn
        };
        if in_turn {
            self.barge_in(core);
        }
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

    /// 她说的一句：记进这一轮说了的、通知跟着写，交出去念。
    fn sentence(&self, text: String) -> Spoken {
        let mut said = self.said.lock().unwrap_or_else(PoisonError::into_inner);
        said.push_str(&text);
        self.notifier.show(Notice::Reply {
            text: said.clone(),
            done: false,
        });
        Spoken::Sentence(text, self.generation.load(Ordering::SeqCst))
    }

    /// 开口就打断（第 6 步）：排着的不念、停掉正在念的、打断这一轮（排着队的话留着，你说的这句认出来接着交），补一块事实写明听到
    /// 哪一句为止。没在念的什么都不做（她还在想时说话，照回合中途来的消息排）。
    pub fn barge_in(&self, core: &Core) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        let _ = core.call("bus.call", json!({"name": "tts", "method": "stop"}));
        // 一轮已经完了的回 not_running：照样补事实。
        let _ = core.call(
            "session.interrupt",
            json!({"session": self.session, "queued": "keep"}),
        );
        let heard = self
            .played
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .last()
            .map_or_else(
                || "nothing.".to_string(),
                |last| format!("\u{201c}{last}\u{201d}"),
            );
        let text = INTERRUPTED_FACT.replace("{heard}", &heard);
        if let Err(error) = core.call(
            "session.note",
            json!({"session": self.session, "facts": [{"kind": "voice", "text": text}]}),
        ) {
            eprintln!("miyu-voice-assistant: session.note: {error}");
        }
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
                    let _ = self.sentences.send(self.sentence(sentence));
                }
            }
            // 一次模型响应写完：块的编号下一次从头数。
            Some("message.assistant") => {
                if let Some(rest) = reply.finish() {
                    let _ = self.sentences.send(self.sentence(rest));
                }
            }
            Some("turn.ended") => {
                if let Some(rest) = reply.finish() {
                    let _ = self.sentences.send(self.sentence(rest));
                }
                let said =
                    std::mem::take(&mut *self.said.lock().unwrap_or_else(PoisonError::into_inner));
                if !said.is_empty() {
                    self.notifier.show(Notice::Reply {
                        text: said,
                        done: true,
                    });
                }
                let _ = self.sentences.send(Spoken::TurnEnded);
            }
            _ => {}
        }
    }
}

/// 念的那个线程共用的：窗口、打断次数、念完了的句子。
struct Turn {
    window: Arc<Mutex<Window>>,
    generation: Arc<AtomicU64>,
    played: Arc<Mutex<Vec<String>>>,
    /// 会话的人格：念的时候交给 `tts`，照它挑音色。
    persona: Option<String>,
}

/// 合好、排着放的。
enum Ready {
    /// 一句：合好的音频（blob）、原文、交出去时的打断次数。
    Sentence(String, String, u64),
    /// 这一轮完了。
    TurnEnded,
}

/// 念的那两个线程（施工 VO-15）：合成的一个照先后交总线上的 `tts.synthesize`，合好的排给放的一个交 `tts.play`；放这一句
/// 的时候下一句已经在合，句子之间不再等合成（原来一句放完才去合下一句，云端合成一两秒，句与句之间断一下）。
///
/// 还有没念完的句子（在合的、排着的、在放的）就算她在说话：半双工不收麦，全双工照收，好听得见打断；一轮完了、念完了开
/// 追问窗口。合不了、放不了（没装、没配朗读，没有音箱）的这一轮只听不说，说一次就不再试，下一轮重新试。
fn speak_in_turn(
    core: Core,
    switches: Switches,
    turn: Turn,
    queue: mpsc::Receiver<Spoken>,
    notifier: Arc<Notifier>,
) {
    let (ready, to_play) = mpsc::channel::<Ready>();
    let speaking = Arc::new(Speaking {
        notifier,
        switches,
        unsaid: AtomicUsize::new(0),
        mute: AtomicBool::new(false),
    });
    let synthesizing = (core.clone(), Arc::clone(&speaking));
    let (generation, persona) = (Arc::clone(&turn.generation), turn.persona.clone());
    std::thread::spawn(move || {
        synthesize_ahead(
            &synthesizing.0,
            &synthesizing.1,
            &generation,
            persona.as_deref(),
            queue,
            &ready,
        );
    });
    std::thread::spawn(move || play_in_order(&core, &speaking, &turn, to_play));
}

/// 两个线程共用的：还有几句没念完（在合的、排着的、在放的，有就算她在说话），这一轮还念不念。
struct Speaking {
    notifier: Arc<Notifier>,
    switches: Switches,
    unsaid: AtomicUsize,
    mute: AtomicBool,
}

impl Speaking {
    /// 要念一句了：算她在说话。
    fn begin(&self) {
        self.unsaid.fetch_add(1, Ordering::SeqCst);
        self.switches.speaking.store(true, Ordering::SeqCst);
    }

    /// 念不成（合不了、放不了）：这一轮只听不说，通知里说一次为什么。
    fn fail(&self, why: String) {
        eprintln!("miyu-voice-assistant: tts: {why}");
        if !self.mute.swap(true, Ordering::SeqCst) {
            self.notifier.show(Notice::Trouble(Step::Speech, why));
        }
    }

    /// 一句念完、丢掉、念不成：最后一句的话她不说了。
    fn end(&self) {
        if self.unsaid.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.switches.speaking.store(false, Ordering::SeqCst);
        }
    }
}

/// 合成的线程：一句句交 `tts.synthesize`（wav），合好的交给放的线程；打断过的（打断次数对不上）、这一轮不念了的跳过。
fn synthesize_ahead(
    core: &Core,
    speaking: &Speaking,
    generation: &AtomicU64,
    persona: Option<&str>,
    queue: mpsc::Receiver<Spoken>,
    ready: &mpsc::Sender<Ready>,
) {
    for spoken in queue {
        match spoken {
            Spoken::Sentence(sentence, mine) => {
                if speaking.mute.load(Ordering::SeqCst) || mine != generation.load(Ordering::SeqCst)
                {
                    continue;
                }
                speaking.begin();
                let synthesized = core.call(
                    "bus.call",
                    json!({"name": "tts", "method": "synthesize",
                           "params": {"text": sentence, "persona": persona, "format": "wav"}}),
                );
                match synthesized {
                    Ok(answer) if answer["audio"].is_string() => {
                        let audio = answer["audio"].as_str().unwrap_or_default().to_string();
                        let _ = ready.send(Ready::Sentence(audio, sentence, mine));
                    }
                    other => {
                        let why = other
                            .map_or_else(|error| error.to_string(), |answer| answer.to_string());
                        speaking.fail(why);
                        speaking.end();
                    }
                }
            }
            Spoken::TurnEnded => {
                speaking.mute.store(false, Ordering::SeqCst);
                let _ = ready.send(Ready::TurnEnded);
            }
        }
    }
}

/// 放的线程：照先后交 `tts.play`；放完的记下（打断时写明听到哪一句）；一轮完了开追问窗口。
fn play_in_order(core: &Core, speaking: &Speaking, turn: &Turn, to_play: mpsc::Receiver<Ready>) {
    for next in to_play {
        match next {
            Ready::Sentence(audio, sentence, mine) => {
                if !speaking.mute.load(Ordering::SeqCst)
                    && mine == turn.generation.load(Ordering::SeqCst)
                {
                    if std::env::var_os("MIYU_VOICE_MEASURE").is_some() {
                        eprintln!("miyu-voice-assistant: measure: speaking \"{sentence}\"");
                    }
                    let played = core.call(
                        "bus.call",
                        json!({"name": "tts", "method": "play", "params": {"audio": audio}}),
                    );
                    match played {
                        Ok(answer) if answer["finished"] == true => turn
                            .played
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .push(sentence),
                        Ok(_) => {}
                        Err(error) => speaking.fail(error.to_string()),
                    }
                }
                speaking.end();
            }
            Ready::TurnEnded => {
                turn.played
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clear();
                let mut window = turn.window.lock().unwrap_or_else(PoisonError::into_inner);
                window.in_turn = false;
                window.until =
                    (!std::mem::take(&mut window.closed)).then(|| Instant::now() + FOLLOW_UP);
            }
        }
    }
}

#[cfg(test)]
mod tests;
