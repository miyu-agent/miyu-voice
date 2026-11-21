//! 她的工具 `say`（设计 20 第六节「她的工具」）：合成一段语音，默认交给人（施工 VO-35，项目主人 11-17 验收第 16、26 条，方案
//! `~/Documents/miyu-say交付方案-2026-11-17.md`）：放进工具结果的 `sent`（`voice: true`），谁收到谁照自己的地方交——终端头从
//! 这台电脑的音箱放、网页在浏览器里放、im 在场所里发成语音消息（核心 SD-1）。`file_only` 照以前只存进这一轮工作区的
//! `audio/`、交回路径，不交给人。场所的平台给了语音上限（`voice_max_ms`）的，长过它的不交、回报错让她重说（主人定：不退回
//! 发文件、不切）。给模型看的字写英文短句，记在 `docs/prompt-ledger.md`。
//!
//! 念完就结束这一轮（施工 VO-45，项目主人 2026-11-20 验收第 5 条，照表情包 `meme_send`）：`end` 不写当 `true`，结果带
//! `end_turn`（核心不再请求模型，这一轮到此为止），像最后一句话是语音；写 `end: false` 的接着干活。`file_only` 不写 `end` 的不
//! 结束：要的是文件路径，拿到了还要接着用（发出去、展示）。出错的不结束，让她改了再说。
//! 存文件先写临时文件再改名（预先放好的链接只会被换掉，不会被顺着写进去），同 miyu-render 的存图。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use miyu_ext::{Call, Reply, Tool};
use serde_json::{Value, json};
use voice_common::{audio, blobs};

use crate::engine::{Audio, Format};
use crate::service::Service;

/// 工具名。
pub const NAME: &str = "say";
/// 存在工作区的哪个文件夹。
pub const FOLDER: &str = "audio";

/// 工具说明：三句，记在 `docs/prompt-ledger.md`。
pub const DESCRIPTION: &str = "Speak text aloud in your voice: the person hears it where they talk with you, as a voice message in chats, and it ends your turn unless end is false. With nobody in the conversation (a schedule or a loop), decide whether it should be heard. Set file_only to just save an audio file and get its path.";

/// 参数。
#[must_use]
pub fn schema() -> Value {
    json!({"type": "object", "properties": {"text": {"type": "string"}, "file_only": {"type": "boolean"}, "end": {"type": "boolean", "description": "Default true, false with file_only."}}, "required": ["text"]})
}

/// 工具的规格。
#[must_use]
pub fn spec() -> Tool {
    Tool::new(NAME, DESCRIPTION)
        .schema(schema())
        .access("write")
        .venues(&["local", "private", "group"])
        .display_name(json!({"zh": "说话", "en": "Say", "ja": "話す"}))
}

/// 办一次：合成，交给人或者存好。
pub fn call(service: &Arc<Service>, args: &Value, call: &Call) -> Reply {
    match run(service, args, call) {
        Ok(reply) => reply,
        Err(why) => Reply::failed(why),
    }
}

/// 这一次合成好了以后结束不结束这一轮：写了 `end` 的照它；没写的说出来的结束，`file_only` 的不结束。
fn ends(args: &Value) -> bool {
    let file_only = args["file_only"].as_bool().unwrap_or(false);
    args["end"].as_bool().unwrap_or(!file_only)
}

fn run(service: &Arc<Service>, args: &Value, call: &Call) -> Result<Reply, String> {
    let reply = done(service, args, call)?;
    Ok(if ends(args) { reply.end_turn() } else { reply })
}

fn done(service: &Arc<Service>, args: &Value, call: &Call) -> Result<Reply, String> {
    let text = args["text"].as_str().ok_or("Give text.")?;
    let file_only = args["file_only"].as_bool().unwrap_or(false);
    if file_only && call.cwd().is_empty() {
        return Err("No workspace for this turn.".to_string());
    }
    // 照这一轮的人格挑音色（核心 TC-1 起 tool.call 带 persona）；没有人格、没给它配的用默认音色。
    let said = service
        .speak(call.core(), text, call.persona(), Format::Mp3)
        .map_err(|error| error.message)?;
    let note = said.unknown_voice.as_deref().map(default_voice_note);
    let audio = said.audio;
    let duration_ms = audio.duration_ms.or_else(|| wav_duration(&audio));
    if file_only {
        return save(call.cwd(), &audio, duration_ms)
            .map(|saved| Reply::text(saved + note.as_deref().unwrap_or_default()));
    }
    if let (Some(limit), Some(ms)) = (call.voice_max_ms(), duration_ms)
        && ms > limit
    {
        return Err(too_long(ms, limit));
    }
    let name = format!(
        "say-{}.{}",
        short_hash(&audio.bytes),
        extension(&audio.media_type)
    );
    let blob = blobs::put(call.core(), &audio.bytes, &name, &audio.media_type)
        .map_err(|error| format!("Cannot hand the audio over: {error}."))?;
    let mut voice =
        json!({"blob": blob, "media_type": audio.media_type, "voice": true, "name": name});
    if let Some(ms) = duration_ms {
        voice["duration_ms"] = ms.into();
    }
    let spoken = match duration_ms {
        Some(ms) => format!("Spoken ({}).", seconds(ms)),
        None => "Spoken.".to_string(),
    };
    Ok(Reply::text(spoken + note.as_deref().unwrap_or_default()).sent(json!([voice])))
}

/// 配的音色这家引擎没有、用了它的默认音色（施工 VO-45）：接在结果后面的一句，记在 `docs/prompt-ledger.md`。
fn default_voice_note(voice: &str) -> String {
    format!(" Default voice used: this engine has no voice {voice}.")
}

/// `file_only`：存进工作区的 `audio/`，交回相对路径和时长。
fn save(cwd: &str, audio: &Audio, duration_ms: Option<u64>) -> Result<String, String> {
    let name = format!(
        "say-{}.{}",
        short_hash(&audio.bytes),
        extension(&audio.media_type)
    );
    store(Path::new(cwd), &name, &audio.bytes)?;
    let shown = Path::new(FOLDER).join(&name);
    Ok(match duration_ms {
        Some(ms) => format!("Saved {} ({}).", shown.display(), seconds(ms)),
        None => format!("Saved {}.", shown.display()),
    })
}

/// 长过场所的语音上限：不交，告诉她多长、上限多少，让她重说（主人 11-17 定）。
fn too_long(ms: u64, limit: u64) -> String {
    format!(
        "Too long for a voice message here: {}, the limit is {}. Say it again in parts or more briefly.",
        seconds(ms),
        seconds(limit)
    )
}

/// `2.3 s` 这样写时长。
fn seconds(ms: u64) -> String {
    format!("{}.{} s", ms / 1000, ms % 1000 / 100)
}

/// 引擎没报时长的 wav 照样本算（mp3 没报的算不出，交给场所那头兜底）。
fn wav_duration(audio: &Audio) -> Option<u64> {
    (audio.media_type == "audio/wav")
        .then(|| audio::wav_duration_ms(&audio.bytes).ok())
        .flatten()
}

/// 照真实的格式起扩展名（DeepSeek 引擎出的是 wav，以前一律叫 `.mp3`）。
fn extension(media_type: &str) -> &'static str {
    match media_type {
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        "audio/ogg" => "ogg",
        _ => "mp3",
    }
}

/// 写进工作区的 `audio/name`：先写同目录下的临时文件、再改名。
///
/// 审计 #119：`audio/` 照真实位置核对还在工作区里（工作区里放一个指到外面的 `audio` 链接，以前就顺着写到外面去）；
/// 临时文件名带进程号和进程里的序号（以前只带进程号，同一进程里两次存同一段声音撞名，失败的那个把对方的临时文件删了），
/// 只删自己建出来的那一个。
fn store(workspace: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let home = fs::canonicalize(workspace)
        .map_err(|error| format!("Cannot open the workspace: {error}."))?;
    let folder = workspace.join(FOLDER);
    fs::create_dir_all(&folder).map_err(|error| format!("Cannot create {FOLDER}/: {error}."))?;
    let real =
        fs::canonicalize(&folder).map_err(|error| format!("Cannot open {FOLDER}/: {error}."))?;
    if !real.starts_with(&home) {
        return Err(format!("{FOLDER}/ leads outside the workspace."));
    }
    let target = real.join(name);
    let temp = real.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file =
        fs::File::create_new(&temp).map_err(|error| format!("Cannot save the audio: {error}."))?;
    let written = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::rename(&temp, &target));
    if let Err(error) = written {
        let _ = fs::remove_file(&temp);
        return Err(format!("Cannot save the audio: {error}."));
    }
    Ok(target)
}

/// 内容哈希的头 12 位：同样的字同样的声音存成同一个文件。
fn short_hash(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let digest = sha2_digest(bytes);
    digest.iter().take(6).fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn sha2_digest(bytes: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).to_vec()
}

#[cfg(test)]
mod tests;
