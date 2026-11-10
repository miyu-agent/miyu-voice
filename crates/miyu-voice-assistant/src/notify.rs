//! 系统通知（设计 20 第九节第 2 步「响一声提示音、弹一条通知」，施工 VO-16）：一路告诉人现在在干什么：在听、认成了什么、
//! 在想、她答了什么，没听清、哪一步出了错。项目主人 2026-11-10 真机验收：「缺少通知。用户完全不知道中途是什么情况」。
//!
//! 一个线程照先后弹（D-Bus、WinRT 要一会儿，不耽误收麦）；弹不出来的（没有通知服务）说一次，别的照旧。三个平台做得到的不一样（关在这里）：
//! - Linux（freedesktop 的通知服务）：每一条换掉上一条（记着上一条的编号，`replaces_id`），一路只有一条，跟着变。
//! - Windows、macOS：换不掉上一条，每变一次多弹一条；所以「在听」、回答说到一半的不弹，只弹认成了什么、回答完了、没听清、
//!   出错、开关对话。
//!
//! 署名是会话的人格（名字、有头像的带头像；项目主人 2026-11-10：「通知应该显示人格名称而不是程序名称」）：Linux 写进应用名、
//! 头像当图标，Windows、macOS 头像放进通知里的图。没有人格的署 Miyu。
//!
//! 测试用：设了 `MIYU_VOICE_TEST_NOTICES`（一个文件）的不弹，一条一行 JSON 记进那个文件（端到端测试不往主人桌面上弹）。

use std::io::Write as _;
use std::path::PathBuf;
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
    /// 对话结束了：`miyu listen` 关的，或者叫醒了一直没开口、她答完 30 秒没再说。
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

/// 谁在说：会话人格的名字、头像（存成文件的路径，没有的不带）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub avatar: Option<PathBuf>,
}

impl Identity {
    /// 读这个人格的名字、头像（`persona.get`、`persona.avatar`）；头像写进包的运行目录（`MIYU_RUNTIME_DIR`，文件名带版本，
    /// 换了头像换文件），通知服务照路径读。读不到名字的署 Miyu，没有头像、写不成的不带。
    #[must_use]
    pub fn of(core: &miyu_ext::Core, persona: &str) -> Identity {
        let name = core
            .call("persona.get", json!({"persona": persona}))
            .ok()
            .and_then(|answer| answer["name"].as_str().map(str::to_string))
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "Miyu".to_string());
        let avatar = core
            .call("persona.avatar", json!({"persona": persona}))
            .ok()
            .and_then(|answer| {
                save_avatar(
                    &answer,
                    &PathBuf::from(std::env::var_os("MIYU_RUNTIME_DIR")?),
                )
            });
        Identity { name, avatar }
    }
}

/// `persona.avatar` 的回应存成 `dir` 里的文件：`{avatar（版本）, media_type, data（base64）}`，没有头像的都是 `null`。
fn save_avatar(answer: &serde_json::Value, dir: &std::path::Path) -> Option<PathBuf> {
    use base64::Engine as _;
    let version = answer["avatar"].as_str()?;
    let extension = match answer["media_type"].as_str()? {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => return None,
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(answer["data"].as_str()?)
        .ok()?;
    let stamp: String = version
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(16)
        .collect();
    let path = dir.join(format!("avatar-{stamp}.{extension}"));
    if !path.exists() {
        std::fs::create_dir_all(dir).ok()?;
        std::fs::write(&path, bytes).ok()?;
    }
    Some(path)
}

impl Default for Identity {
    fn default() -> Identity {
        Identity {
            name: "Miyu".to_string(),
            avatar: None,
        }
    }
}

/// 标题和正文，照人的语言（`zh`、`ja`，别的用英语）；`name` 是她的名字（人格的）。
#[must_use]
pub fn text(notice: &Notice, language: &str, name: &str) -> (String, String) {
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
                "直接说；30 秒不说话就结束，miyu listen 也能关",
                "そのまま話してください。30 秒黙ると終了、miyu listen でも終了",
                "Just talk; it ends after 30 s of quiet, or miyu listen ends it",
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
                name.to_string()
            } else {
                match language {
                    "zh" => format!("{name} 在说…"),
                    "ja" => format!("{name} が話しています…"),
                    _ => format!("{name} is speaking…"),
                }
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
                Step::Sending => match language {
                    "zh" => format!("交不给{name}"),
                    "ja" => format!("{name} に届きません"),
                    _ => format!("Can't reach {name}"),
                },
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
    queue: Mutex<mpsc::Sender<(Notice, String, Identity)>>,
    /// 人的语言（核心握手、配置变了时给的）。
    language: Mutex<String>,
    /// 署谁的名（会话的人格）。
    identity: Mutex<Identity>,
}

impl Notifier {
    /// 起弹通知的线程。
    #[must_use]
    pub fn start() -> Notifier {
        let (queue, notices) = mpsc::channel::<(Notice, String, Identity)>();
        let record = std::env::var_os("MIYU_VOICE_TEST_NOTICES").map(std::path::PathBuf::from);
        std::thread::spawn(move || {
            let mut shown = platform::Shown::default();
            for (notice, language, identity) in notices {
                if record.is_none() && !platform::REPLACES && notice.passing() {
                    continue;
                }
                let (title, body) = text(&notice, &language, &identity.name);
                match &record {
                    Some(path) => {
                        let line = json!({"app": identity.name, "title": title, "body": body,
                                          "avatar": identity.avatar.is_some()})
                        .to_string();
                        if let Ok(mut file) = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(path)
                        {
                            let _ = writeln!(file, "{line}");
                        }
                    }
                    None => shown.show(&identity, &title, &body),
                }
            }
        });
        Notifier {
            queue: Mutex::new(queue),
            language: Mutex::new("en".to_string()),
            identity: Mutex::new(Identity::default()),
        }
    }

    /// 换署名（会话的人格）。
    pub fn set_identity(&self, identity: Identity) {
        *self.identity.lock().unwrap_or_else(PoisonError::into_inner) = identity;
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
        let identity = self
            .identity
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let _ = self
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .send((notice, language, identity));
    }
}

/// Linux 这一类：freedesktop 的通知服务，换掉上一条。
#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    use notify_rust::{Hint, Notification};

    use super::Identity;

    /// 能换掉上一条。
    pub const REPLACES: bool = true;

    /// 弹过的：上一条的编号（下一条换掉它），弹不出来的说过没有。
    #[derive(Default)]
    pub struct Shown {
        last: Option<u32>,
        failed: bool,
    }

    impl Shown {
        pub fn show(&mut self, identity: &Identity, title: &str, body: &str) {
            let mut notification = Notification::new();
            notification
                .appname(&identity.name)
                .summary(title)
                .body(body)
                .hint(Hint::Category("im".to_string()));
            // 有头像的头像当图标，没有的不放图标。
            if let Some(avatar) = identity.avatar.as_ref().and_then(|path| path.to_str()) {
                notification.icon(avatar);
            }
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

    use super::Identity;

    /// 能换掉上一条。
    pub const REPLACES: bool = false;

    /// 弹不出来的说过没有。
    #[derive(Default)]
    pub struct Shown {
        failed: bool,
    }

    impl Shown {
        pub fn show(&mut self, identity: &Identity, title: &str, body: &str) {
            let mut notification = Notification::new();
            notification
                .appname(&identity.name)
                .summary(title)
                .body(body);
            // 有头像的放进通知里的图，没有的不放。
            if let Some(avatar) = identity.avatar.as_ref().and_then(|path| path.to_str()) {
                notification.image_path(avatar);
            }
            match notification.show() {
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
