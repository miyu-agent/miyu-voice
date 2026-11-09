//! 她的工具 `say`（设计 20 第六节「她的工具」）：合成一段语音、存进这一轮工作区的 `audio/`，交回路径，不放。发不发、去哪放
//! 她接着定：IM 里调桥的 `send_voice`，网页上用 `show`。给模型看的字写英文短句，记在 `docs/prompt-ledger.md`。
//! 存文件先写临时文件再改名（预先放好的链接只会被换掉，不会被顺着写进去），同 miyu-render 的存图。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use miyu_ext::{Call, Reply, Tool};
use serde_json::{Value, json};

use crate::engine::Format;
use crate::service::Service;

/// 工具名。
pub const NAME: &str = "say";
/// 存在工作区的哪个文件夹。
pub const FOLDER: &str = "audio";

/// 工具的规格。
#[must_use]
pub fn spec() -> Tool {
    Tool::new(
        NAME,
        "Turn text into speech in your voice and save it as an MP3 file. Nobody hears it until you send or show the file.",
    )
    .schema(json!({"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}))
    .access("write")
    .venues(&["local", "private", "group"])
    .display_name(json!({"zh": "说话", "en": "Say", "ja": "話す"}))
}

/// 办一次：合成、存好，交回路径和时长。
pub fn call(service: &Arc<Service>, args: &Value, call: &Call) -> Reply {
    match run(service, args, call) {
        Ok(text) => Reply::text(text),
        Err(why) => Reply::failed(why),
    }
}

fn run(service: &Arc<Service>, args: &Value, call: &Call) -> Result<String, String> {
    let text = args["text"].as_str().ok_or("Give text.")?;
    if call.cwd().is_empty() {
        return Err("No workspace for this turn.".to_string());
    }
    // 先用默认音色：核心给 tool.call 加上 persona（core1 11-10 定，排在 PL-1 后面）、开发包的 Call 能读它以后照人格挑。
    let audio = service
        .speak(text, None, Format::Mp3)
        .map_err(|error| error.message)?;
    let folder = Path::new(call.cwd()).join(FOLDER);
    let name = format!("say-{}.mp3", short_hash(&audio.bytes));
    store(&folder, &name, &audio.bytes)?;
    let shown = Path::new(FOLDER).join(&name);
    Ok(match audio.duration_ms {
        Some(ms) => format!(
            "Saved {} ({}.{} s).",
            shown.display(),
            ms / 1000,
            ms % 1000 / 100
        ),
        None => format!("Saved {}.", shown.display()),
    })
}

/// 写进 `folder/name`：先写同目录下的临时文件、再改名。
fn store(folder: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    fs::create_dir_all(folder).map_err(|error| format!("Cannot create {FOLDER}/: {error}."))?;
    let real =
        fs::canonicalize(folder).map_err(|error| format!("Cannot open {FOLDER}/: {error}."))?;
    let target = real.join(name);
    let temp = real.join(format!(".{name}.{}.tmp", std::process::id()));
    let written = fs::File::create_new(&temp)
        .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()))
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
