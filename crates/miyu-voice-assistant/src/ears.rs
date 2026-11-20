//! 听的那一套照哪份唤醒词、门槛建（施工 VO-44，项目主人 2026-11-20 验收第 3 条）：原来助手起来时抄一份配置、建一次，设置页
//! 改了唤醒词要等助手重开才生效（主人 23:18 改的，23:22 重开助手才认）。现在收麦的线程每一帧拿现在的配置比一比，唤醒词、
//! 门槛变了的当场照新的重建；建不成的说一次，等配置再变了才再试（不每一帧都试）。

use crate::keywords;
use crate::listen::{Listener, Sensitivity};
use crate::models::Models;
use crate::service::Settings;

/// 建听的那一套要的两样。
#[derive(Debug, Clone, PartialEq)]
pub struct Tuning {
    /// 唤醒词。
    pub wake_words: Vec<String>,
    /// 唤醒词判中的门槛。
    pub threshold: f32,
}

impl Tuning {
    /// 从配置里取。
    #[must_use]
    pub fn of(settings: &Settings) -> Tuning {
        Tuning {
            wake_words: settings.wake_words.clone(),
            threshold: settings.threshold,
        }
    }
}

/// 听的那一套（`L`，测试换成假的）和它照着建的那一份。
pub struct Ears<L> {
    built: Option<Tuning>,
    listener: Option<L>,
}

impl<L> Default for Ears<L> {
    fn default() -> Ears<L> {
        Ears {
            built: None,
            listener: None,
        }
    }
}

impl<L> Ears<L> {
    /// 照 `tuning` 要听的那一套：还没建、唤醒词或门槛和上次建的不一样的，交 `build` 重建；建不成的说一次为什么、交回 `None`，
    /// 同一份配置不再试。
    pub fn tuned(
        &mut self,
        tuning: &Tuning,
        build: impl FnOnce(&Tuning) -> Result<L, String>,
    ) -> Option<&mut L> {
        if self.built.as_ref() != Some(tuning) {
            self.listener = match build(tuning) {
                Ok(listener) => Some(listener),
                Err(why) => {
                    eprintln!("miyu-voice-assistant: {why}");
                    None
                }
            };
            self.built = Some(tuning.clone());
        }
        self.listener.as_mut()
    }
}

/// 照配置的唤醒词、门槛建听的那一套：编不了的跳过（说一句），一个都编不了的不建。
///
/// # Errors
///
/// 读不了词表、一个唤醒词都编不了、模型加载不了：为什么。
pub fn build(models: &Models, settings: &Tuning) -> Result<Listener, String> {
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
