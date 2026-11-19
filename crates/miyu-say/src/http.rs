//! 连云端那一下：POST 一段 JSON，交回回应的字节。走系统代理；连不上、对方回 4xx/5xx 的照实说，带上对方回的头一段字
//! （他们的报错原因多半在里面），从不带请求头：key 在请求头里，报错、日志里不能出现（2026-11-09 定）。

use std::io::Read;
use std::time::Duration;

use serde_json::Value;

use crate::engine::EngineError;

/// 一句话最多等多久（设计 20 第三节「形状」`tts.synthesize` 登记 60 秒，留一点给读写）。
const TIMEOUT: Duration = Duration::from_secs(50);
/// 回应最多读多少：一句话的音频，几 MB 足够；超了的当对方出了错。
const LONGEST: u64 = 32 * 1024 * 1024;
/// 报错时带对方回的多少字。
const EXCERPT: usize = 300;

/// POST `body` 到 `url`，`headers` 是额外的请求头（鉴权在这里）。
///
/// # Errors
///
/// 连不上、超时、对方回 4xx/5xx、读到一半断了。
pub fn post_json(
    url: &str,
    headers: &[(&str, &str)],
    body: &Value,
) -> Result<Vec<u8>, EngineError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let mut request = agent.post(url).header("Content-Type", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = request
        .send(body.to_string())
        .map_err(|error| EngineError::Failed(format!("Could not reach {}: {error}.", host(url))))?;
    let status = response.status();
    let bytes = read_capped(response.into_body().into_reader(), LONGEST, host(url))?;
    if !status.is_success() {
        let excerpt: String = String::from_utf8_lossy(&bytes)
            .chars()
            .take(EXCERPT)
            .collect();
        return Err(EngineError::Failed(format!(
            "{} answered {}: {excerpt}",
            host(url),
            status.as_u16()
        )));
    }
    Ok(bytes)
}

/// 读完回应，最多 `limit` 个字节；多读一个字节看超没超，超了的照实报错，不悄悄截成半段音频（审计 #126）。
fn read_capped(reader: impl Read, limit: u64, host: &str) -> Result<Vec<u8>, EngineError> {
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            EngineError::Failed(format!("The answer from {host} broke off: {error}."))
        })?;
    if bytes.len() as u64 > limit {
        return Err(EngineError::Failed(format!(
            "The answer from {host} is larger than {} MB.",
            limit / (1024 * 1024)
        )));
    }
    Ok(bytes)
}

/// 网址的主机名，报错里说是哪家用：去掉 `user:pass@` 那段，账号密码不进报错、日志（审计 #126）。
fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host)
}

#[cfg(test)]
mod tests;
