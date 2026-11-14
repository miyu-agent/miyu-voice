//! 包的配置（清单里的 `say.*`，设计 20 第六节「配置页」）：握手时交来、改了推来，key 是真值。照它配引擎，挑音色。

use miyu_ext::Core;
use serde_json::{Map, Value};

use crate::community::Community;
use crate::engine::Engine;
use crate::{mimo, minimax, openai};

/// 一个人格配的音色。
#[derive(Debug, Clone, PartialEq)]
pub struct PersonaVoice {
    /// 人格编号。
    pub persona: String,
    /// 引擎的音色编号。
    pub voice: String,
    /// 语速。
    pub speed: f32,
}

/// 读出来的配置。
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// `minimax`、`mimo`、`openai`、`community`（社区引擎，施工 VO-29）。
    pub engine: String,
    /// 选了社区引擎的：接进 `tts.engine` 的那个包的编号（`say.community.package`）。
    pub community: String,
    /// 默认音色；空的用引擎的默认。
    pub voice: String,
    /// 默认语速。
    pub speed: f32,
    /// 每个人格的音色。
    pub personas: Vec<PersonaVoice>,
    minimax_region: String,
    minimax_key: String,
    minimax_model: String,
    mimo_key: String,
    mimo_model: String,
    openai_base_url: String,
    openai_key: String,
    openai_model: String,
}

impl Settings {
    /// 照核心交来的配置读；没写的照清单的默认。
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
        #[expect(clippy::cast_possible_truncation, reason = "语速在 0 到 2 之间")]
        let speed = config
            .get("say.speed")
            .and_then(Value::as_f64)
            .unwrap_or(1.0) as f32;
        let personas = config
            .get("say.personas")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|item| {
                        #[expect(clippy::cast_possible_truncation, reason = "语速在 0 到 2 之间")]
                        let speed = item["speed"].as_f64().unwrap_or(1.0) as f32;
                        Some(PersonaVoice {
                            persona: item["persona"].as_str()?.to_string(),
                            voice: item["voice"].as_str()?.trim().to_string(),
                            speed,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Settings {
            engine: text("say.engine", "minimax"),
            community: text("say.community.package", ""),
            voice: text("say.voice", ""),
            speed,
            personas,
            minimax_region: text("say.minimax.region", "mainland"),
            minimax_key: text("say.minimax.key", ""),
            minimax_model: text("say.minimax.model", minimax::MODEL),
            mimo_key: text("say.mimo.key", ""),
            mimo_model: text("say.mimo.model", mimo::MODEL),
            openai_base_url: text("say.openai.base_url", "https://api.openai.com/v1"),
            openai_key: text("say.openai.key", ""),
            openai_model: text("say.openai.model", "gpt-4o-mini-tts"),
        }
    }

    /// 选的引擎配好了没有（没配好的不登记 `tts.*`，头就不画朗读按钮）。
    #[must_use]
    pub fn configured(&self) -> bool {
        match self.engine.as_str() {
            "minimax" => !self.minimax_key.is_empty(),
            "mimo" => !self.mimo_key.is_empty(),
            "openai" => !self.openai_base_url.is_empty() && !self.openai_model.is_empty(),
            // 挑了包就算配好：那个包没在运行的，合成时回 engine_failed。
            "community" => !self.community.is_empty(),
            _ => false,
        }
    }

    /// 照配置建选的引擎；社区引擎经 `core` 调总线。
    #[must_use]
    pub fn engine(&self, core: &Core) -> Box<dyn Engine> {
        match self.engine.as_str() {
            "community" => Box::new(Community {
                package: self.community.clone(),
                core: core.clone(),
            }),
            "mimo" => Box::new(mimo::MiMo {
                base_url: mimo::BASE_URL.to_string(),
                key: self.mimo_key.clone(),
                model: self.mimo_model.clone(),
            }),
            "openai" => Box::new(openai::OpenAiCompatible {
                base_url: self.openai_base_url.clone(),
                key: self.openai_key.clone(),
                model: self.openai_model.clone(),
            }),
            _ => Box::new(minimax::MiniMax {
                base_url: if self.minimax_region == "international" {
                    minimax::INTERNATIONAL
                } else {
                    minimax::MAINLAND
                }
                .to_string(),
                key: self.minimax_key.clone(),
                model: self.minimax_model.clone(),
            }),
        }
    }

    /// 这个人格用哪个音色、多快；没配的用默认。
    #[must_use]
    pub fn voice_for(&self, persona: Option<&str>) -> (String, f32) {
        persona
            .and_then(|persona| self.personas.iter().find(|one| one.persona == persona))
            .map_or_else(
                || (self.voice.clone(), self.speed),
                |one| (one.voice.clone(), one.speed),
            )
    }
}

#[cfg(test)]
mod tests;
