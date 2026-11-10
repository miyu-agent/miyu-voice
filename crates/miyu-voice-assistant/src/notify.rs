//! 系统通知（设计 20 第九节第 2 步「响一声提示音、弹一条通知」，施工 VO-16）：一路告诉人现在在干什么：在听、认成了什么、
//! 在想、她答了什么，没听清、哪一步出了错。项目主人 2026-11-10 真机验收：「缺少通知。用户完全不知道中途是什么情况」。
//!
//! 一个线程照先后弹（D-Bus、WinRT 要一会儿，不耽误收麦）；弹不出来的（没有通知服务）说一次，别的照旧。三个平台做得到的不一样（关在这里）：
//! - Linux（freedesktop 的通知服务）：每一条换掉上一条（记着上一条的编号，`replaces_id`），一路只有一条，跟着变。
//! - Windows、macOS：换不掉上一条，每变一次多弹一条；所以「在听」、回答说到一半的不弹，只弹认成了什么、回答完了、没听清、
//!   出错、开关对话。
//!
//! 测试用：设了 `MIYU_VOICE_TEST_NOTICES`（一个文件）的不弹，一条一行 JSON 记进那个文件（端到端测试不往主人桌面上弹）。

use std::io::Write as _;
use std::sync::mpsc;
use std::sync::{Mutex, PoisonError};

use serde_json::json;

/// 回答在通知里最多留几个字：长的留后面这些，跟着她说到哪儿。
const REPLY_CHARS: usize = 200;

/// 弹一条什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// 喊了唤醒词：在听，说吧。
    Listening,
    /// `miyu listen` 开了对话。
    ConversationOn,
    /// 关了对话。
    ConversationOff,
    /// 认成了这句、交给她了：在想。
    Heard(String),
    /// 听到有人说话，认不出字（太短、没把握）。
    NotHeard,
    /// 她的回答，到现在说了的；`done` 是这一轮答完了。
    Reply { text: String, done: bool },
    /// 哪一步出了错、为什么。
    Trouble(Step, String),
}

/// 出错的那一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// 开麦。
    Microphone,
    /// 认字（`stt`）。
    Recognition,
    /// 交给她（会话）。
    Sending,
    /// 念（`tts`）：这一轮只在通知、各个头里看得到回答。
    Speech,
}

impl Notice {
    /// Windows、macOS 换不掉上一条：这几种不弹，免得一句话弹好几条。
    fn passing(&self) -> bool {
        matches!(self, Notice::Listening | Notice::Reply { done: false, .. })
    }
}

/// 标题和正文，照人的语言（`zh`、`ja`，别的用英语）。
#[must_use]
pub fn text(notice: &Notice, language: &str) -> (String, String) {
    let pick = |zh: &str, ja: &str, en: &str| -> String {
        match language {
            "zh" => zh,
            "ja" => ja,
            _ => en,
        }
        .to_string()
    };
    match notice {
        Notice::Listening => (
            pick("在听", "聞いています", "Listening"),
            pick("说吧", "どうぞ", "Go ahead"),
        ),
        Notice::ConversationOn => (
            pick("对话开着", "会話中", "Conversation on"),
            pick(
                "直接说，不用叫她；miyu listen 关掉",
                "そのまま話してください。miyu listen で終了",
                "Just talk, no wake word; miyu listen to end it",
            ),
        ),
        Notice::ConversationOff => (
            pick("对话结束", "会話を終了しました", "Conversation ended"),
            pick(
                "叫她的名字，或者 miyu listen 再开",
                "名前を呼ぶか、miyu listen でまた始められます",
                "Say her name, or miyu listen to start again",
            ),
        ),
        Notice::Heard(heard) => (
            pick("在想…", "考えています…", "Thinking…"),
            format!("{}{heard}", pick("你：", "あなた：", "You: ")),
        ),
        Notice::NotHeard => (
            pick("没听清", "聞き取れませんでした", "Didn't catch that"),
            pick("再说一次", "もう一度どうぞ", "Say it again"),
        ),
        Notice::Reply { text, done } => (
            if *done {
                "Miyu".to_string()
            } else {
                pick("Miyu 在说…", "Miyu が話しています…", "Miyu is speaking…")
            },
            tail(text, REPLY_CHARS),
        ),
        Notice::Trouble(step, why) => {
            let title = match step {
                Step::Microphone => pick(
                    "麦克风打不开",
                    "マイクを開けません",
                    "Can't open the microphone",
                ),
                Step::Recognition => pick("认不了字", "文字にできません", "Can't recognize speech"),
                Step::Sending => pick("交不给她", "Miyu に届きません", "Can't reach Miyu"),
                Step::Speech => pick("念不出来", "読み上げできません", "Can't speak"),
            };
            let body = if *step == Step::Speech {
                format!(
                    "{why}\n{}",
                    pick(
                        "这一轮的回答看通知和各个界面",
                        "この回答は通知と各画面で見られます",
                        "Read this reply in the notification or any Miyu window",
                    )
                )
            } else {
                why.clone()
            };
            (title, body)
        }
    }
}

/// 长的留最后 `chars` 个字，前面写省略号。
fn tail(text: &str, chars: usize) -> String {
    let count = text.chars().count();
    if count <= chars {
        return text.to_string();
    }
    let kept: String = text.chars().skip(count - chars).collect();
    format!("…{kept}")
}

/// 弹通知的：交给它的照先后弹。
pub struct Notifier {
    queue: Mutex<mpsc::Sender<(Notice, String)>>,
    /// 人的语言（核心握手、配置变了时给的）。
    language: Mutex<String>,
}

impl Notifier {
    /// 起弹通知的线程。
    #[must_use]
    pub fn start() -> Notifier {
        let (queue, notices) = mpsc::channel::<(Notice, String)>();
        let record = std::env::var_os("MIYU_VOICE_TEST_NOTICES").map(std::path::PathBuf::from);
        std::thread::spawn(move || {
            let mut shown = platform::Shown::default();
            for (notice, language) in notices {
                if record.is_none() && !platform::REPLACES && notice.passing() {
                    continue;
                }
                let (title, body) = text(&notice, &language);
                match &record {
                    Some(path) => {
                        let line = json!({"title": title, "body": body}).to_string();
                        if let Ok(mut file) = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(path)
                        {
                            let _ = writeln!(file, "{line}");
                        }
                    }
                    None => shown.show(&title, &body),
                }
            }
        });
        Notifier {
            queue: Mutex::new(queue),
            language: Mutex::new("en".to_string()),
        }
    }

    /// 换人的语言（`zh`、`en`、`ja`）。
    pub fn set_language(&self, language: &str) {
        *self.language.lock().unwrap_or_else(PoisonError::into_inner) = language.to_string();
    }

    /// 弹一条（排在后面，马上回）。
    pub fn show(&self, notice: Notice) {
        let language = self
            .language
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let _ = self
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .send((notice, language));
    }
}

/// Linux 这一类：freedesktop 的通知服务，换掉上一条。
#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    use notify_rust::{Hint, Notification};

    /// 能换掉上一条。
    pub const REPLACES: bool = true;

    /// 弹过的：上一条的编号（下一条换掉它），弹不出来的说过没有。
    #[derive(Default)]
    pub struct Shown {
        last: Option<u32>,
        failed: bool,
    }

    impl Shown {
        pub fn show(&mut self, title: &str, body: &str) {
            let mut notification = Notification::new();
            notification
                .appname("Miyu")
                .summary(title)
                .body(body)
                .icon("audio-input-microphone")
                .hint(Hint::Category("im".to_string()));
            if let Some(id) = self.last {
                notification.id(id);
            }
            match notification.show() {
                Ok(handle) => {
                    self.last = Some(handle.id());
                    self.failed = false;
                }
                Err(error) => {
                    if !self.failed {
                        eprintln!("miyu-voice-assistant: notifications: {error}");
                    }
                    self.failed = true;
                }
            }
        }
    }
}

/// Windows、macOS：换不掉上一条，每条新弹。
#[cfg(any(windows, target_os = "macos"))]
mod platform {
    use notify_rust::Notification;

    /// 能换掉上一条。
    pub const REPLACES: bool = false;

    /// 弹不出来的说过没有。
    #[derive(Default)]
    pub struct Shown {
        failed: bool,
    }

    impl Shown {
        pub fn show(&mut self, title: &str, body: &str) {
            match Notification::new()
                .appname("Miyu")
                .summary(title)
                .body(body)
                .show()
            {
                Ok(_) => self.failed = false,
                Err(error) => {
                    if !self.failed {
                        eprintln!("miyu-voice-assistant: notifications: {error}");
                    }
                    self.failed = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
