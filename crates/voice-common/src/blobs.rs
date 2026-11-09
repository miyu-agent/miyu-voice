//! 经核心存、读 blob（核心仓库 `docs/reference/blob.md`）：总线上的音频只传 blob 的编号（设计 20 第三节）。一行最长 1 MiB，
//! base64 以后一次 `blob.put` 只放得下 700 KiB 上下；大的拆成 512 KiB 一块走 `blob.open/write/close`。读也一块块读。

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use miyu_ext::{Core, CoreError};
use serde_json::{Value, json};

/// 一块多大：`blob.write`、`blob.get` 一次最多 512 KiB。
const CHUNK: usize = 512 * 1024;
/// 比它小的一次 `blob.put`。
const SINGLE: usize = 600 * 1024;
/// 最大读多少：20 MiB（核心收的附件上限），超过的当坏的。
const LARGEST: u64 = 20 * 1024 * 1024;

/// 存一段字节，交回 blob 编号。
///
/// # Errors
///
/// 核心拒了（太大、存不下）。
pub fn put(core: &Core, bytes: &[u8], name: &str, media_type: &str) -> Result<String, CoreError> {
    let answer = if bytes.len() <= SINGLE {
        core.call(
            "blob.put",
            json!({"data": STANDARD.encode(bytes), "name": name, "media_type": media_type}),
        )?
    } else {
        let opened = core.call(
            "blob.open",
            json!({"name": name, "media_type": media_type, "size": bytes.len()}),
        )?;
        let upload = opened["upload"].clone();
        for (n, chunk) in bytes.chunks(CHUNK).enumerate() {
            core.call(
                "blob.write",
                json!({"upload": upload, "offset": n * CHUNK, "data": STANDARD.encode(chunk)}),
            )?;
        }
        core.call("blob.close", json!({"upload": upload}))?
    };
    Ok(answer["blob"].as_str().unwrap_or_default().to_string())
}

/// 读一个 blob 的全部字节。
///
/// # Errors
///
/// 核心拒了（读不到、不认识）；超过 20 MiB；回的不是 base64。
pub fn get(core: &Core, blob: &str) -> Result<Vec<u8>, String> {
    let size = core
        .call("blob.get", json!({"blob": blob, "length": 0}))
        .map_err(|error| error.to_string())?["size"]
        .as_u64()
        .unwrap_or(0);
    if size > LARGEST {
        return Err(format!(
            "The audio is larger than {} MiB.",
            LARGEST / 1024 / 1024
        ));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    while (bytes.len() as u64) < size {
        let part: Value = core
            .call(
                "blob.get",
                json!({"blob": blob, "offset": bytes.len(), "length": CHUNK}),
            )
            .map_err(|error| error.to_string())?;
        let data = STANDARD
            .decode(part["data"].as_str().unwrap_or_default())
            .map_err(|error| format!("The core sent bad base64: {error}."))?;
        if data.is_empty() {
            break;
        }
        bytes.extend(data);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests;
