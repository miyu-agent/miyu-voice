//! 总线上的 `tts`（设计 20 第三节「形状」，核心 `docs/reference/bus.md`）：`tts.synthesize`、`tts.play`、`tts.stop`。选的
//! 引擎配好了才登记，配置改坏了撤掉（67 的 B8 照登记算 ready，头查到 ready 才画朗读按钮）。

use std::sync::{Arc, PoisonError, RwLock};

use miyu_ext::{Core, Extension, Method, MethodError};
use serde_json::{Map, Value, json};
use voice_common::blobs;

use crate::engine::Format;
use crate::say;
use crate::settings::Settings;
use voice_common::speaker::{PlayError, Speaker};

/// 三个方法的全名。
pub const METHODS: [&str; 3] = ["tts.synthesize", "tts.play", "tts.stop"];

/// 跑着的朗读：配置、音箱。
pub struct Service {
    settings: RwLock<Settings>,
    speaker: Speaker,
}

impl Service {
    /// 照握手交来的配置建。
    #[must_use]
    pub fn new(config: &Map<String, Value>) -> Arc<Service> {
        Arc::new(Service {
            settings: RwLock::new(Settings::from_config(config)),
            speaker: Speaker::default(),
        })
    }

    /// 现在的配置。
    pub fn settings(&self) -> Settings {
        self.settings
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// 配置变了（握手、推送）：重读，配好了登记 `tts.*`，没配好撤掉。
    pub fn refresh(&self, core: &Core) {
        let settings = Settings::from_config(&core.config());
        let ready = settings.configured();
        *self
            .settings
            .write()
            .unwrap_or_else(PoisonError::into_inner) = settings;
        let switched = if ready {
            core.enable_methods(&METHODS)
        } else {
            core.disable_methods(&METHODS)
        };
        if let Err(error) = switched {
            eprintln!("miyu-say: package.methods failed: {error}");
        }
    }

    /// 登记三个方法（先不登记，[`Service::refresh`] 照配置开）。
    pub fn register(self: &Arc<Service>, ext: &mut Extension) {
        let service = Arc::clone(self);
        ext.method_with(
            Method::new("tts.synthesize").timeout_ms(60_000).later(),
            move |params, _, core| service.synthesize(params, core),
        );
        let service = Arc::clone(self);
        ext.method_with(
            Method::new("tts.play").timeout_ms(600_000).later(),
            move |params, _, core| service.play(params, core),
        );
        let service = Arc::clone(self);
        ext.method_with(
            Method::new("tts.stop").timeout_ms(5_000).later(),
            move |params, _, _| {
                only(params, &[])?;
                Ok(json!({"stopped": service.speaker.stop()}))
            },
        );
    }

    fn synthesize(&self, params: &Value, core: &Core) -> Result<Value, MethodError> {
        only(params, &["text", "persona", "format"])?;
        let format = match params["format"].as_str() {
            None | Some("mp3") => Format::Mp3,
            Some("wav") => Format::Wav,
            Some(other) => return Err(bad(&format!("Unknown format {other}."))),
        };
        let text = params["text"].as_str().ok_or_else(|| bad("Give text."))?;
        let audio = self.speak(text, params["persona"].as_str(), format)?;
        let name = format!("speech.{}", format.as_str());
        let blob = blobs::put(core, &audio.bytes, &name, &audio.media_type).map_err(|error| {
            MethodError::new(
                "engine_failed",
                format!("Could not store the audio: {error}."),
            )
        })?;
        let mut answer = json!({"audio": blob, "media_type": audio.media_type});
        if let Some(duration) = audio.duration_ms {
            answer["duration_ms"] = duration.into();
        }
        Ok(answer)
    }

    fn play(&self, params: &Value, core: &Core) -> Result<Value, MethodError> {
        only(params, &["audio", "text", "persona"])?;
        let bytes = match (params["audio"].as_str(), params["text"].as_str()) {
            (Some(blob), None) => {
                blobs::get(core, blob).map_err(|why| MethodError::new("bad_audio", why))?
            }
            (None, Some(text)) => {
                self.speak(text, params["persona"].as_str(), Format::Wav)?
                    .bytes
            }
            _ => return Err(bad("Give audio or text, not both.")),
        };
        let finished = self.speaker.play(bytes).map_err(|error| match error {
            PlayError::BadAudio(why) => MethodError::new("bad_audio", why),
            PlayError::NoSpeaker(why) => MethodError::new("no_speaker", why),
        })?;
        Ok(json!({"finished": finished}))
    }

    /// 照配置合成一段：这个人格的音色、语速。
    pub fn speak(
        &self,
        text: &str,
        persona: Option<&str>,
        format: Format,
    ) -> Result<crate::engine::Audio, MethodError> {
        let settings = self.settings();
        let (voice, speed) = settings.voice_for(persona);
        say::synthesize(settings.engine().as_ref(), text, &voice, speed, format)
            .map_err(|error| MethodError::new(error.code(), error.message()))
    }
}

/// 参数里只能有这几格（设计 20 第三节「形状」：不认识的字段报 `bad_params`，写错名字的一方早知道）。
fn only(params: &Value, allowed: &[&str]) -> Result<(), MethodError> {
    let Some(fields) = params.as_object() else {
        return Ok(());
    };
    match fields.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(unknown) => Err(bad(&format!("Unknown field {unknown}."))),
        None => Ok(()),
    }
}

fn bad(message: &str) -> MethodError {
    MethodError::new("bad_params", message)
}

#[cfg(test)]
mod tests;
