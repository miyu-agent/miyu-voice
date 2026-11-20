//! 语音会话照哪个人格、预设造（施工 VO-44，项目主人 2026-11-20 验收第 1 条）：设置项 `voice-assistant.persona`、
//! `voice-assistant.preset`，空的照核心的默认（`venue.session` 不带这一格）。
//!
//! 核心找回已有的场所会话时不看 `persona`、`preset`（核心 `docs/zh/reference/venues.md`：「用于创建会话的几项，找回已有的
//! 会话时不会查看」），所以换了设置要另开一个新会话（`fresh: true`）才生效。上一次照哪一份开的记在包的状态目录
//! （[`FILE`]）：设置和它不一样才另开，一样的照常找回原来的会话（主人在终端、网页里给这个会话换的人格不被覆盖）。正在对话的不
//! 打断，这一场完了再换（[`crate::service`]）。

use std::path::Path;

use serde_json::{Map, Value, json};

/// 记上一次照哪一份开的文件（在 `MIYU_STATE_DIR` 里）。
pub const FILE: &str = "session.json";

/// 人格、预设：空的是照核心的默认。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Choice {
    /// 人格的编号。
    pub persona: String,
    /// 预设的编号。
    pub preset: String,
}

impl Choice {
    /// 照核心交来的配置读（`voice-assistant.persona`、`voice-assistant.preset`）。
    #[must_use]
    pub fn from_config(config: &Map<String, Value>) -> Choice {
        let text = |key: &str| {
            config
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        Choice {
            persona: text("voice-assistant.persona"),
            preset: text("voice-assistant.preset"),
        }
    }

    /// `venue.session` 的参数：`venue`、`peer` 是这个场所的；设了人格、预设的带上，`fresh` 是另开一个新的。
    #[must_use]
    pub fn session_params(&self, venue: &str, peer: &str, fresh: bool) -> Value {
        let mut params = json!({"venue": venue, "kind": "private", "peer": peer, "images": false});
        if !self.persona.is_empty() {
            params["persona"] = self.persona.clone().into();
        }
        if !self.preset.is_empty() {
            params["preset"] = self.preset.clone().into();
        }
        if fresh {
            params["fresh"] = true.into();
        }
        params
    }

    /// 上一次照哪一份开的：没有记过、读不出来的当是默认的（以前的版本只会照默认开）。
    #[must_use]
    pub fn load(dir: &Path) -> Choice {
        let saved: Value = std::fs::read(dir.join(FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or(Value::Null);
        let text = |key: &str| saved[key].as_str().unwrap_or_default().to_string();
        Choice {
            persona: text("persona"),
            preset: text("preset"),
        }
    }

    /// 记下这一份。
    ///
    /// # Errors
    ///
    /// 写不进去：为什么。
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let text = json!({"persona": self.persona, "preset": self.preset}).to_string();
        std::fs::create_dir_all(dir)
            .and_then(|()| std::fs::write(dir.join(FILE), text))
            .map_err(|error| format!("cannot save {FILE}: {error}"))
    }
}

#[cfg(test)]
mod tests;
