//! 核心拒了的原因写成一句（施工 VO-45，项目主人 2026-11-20 验收第 6 条）：别的包的程序回了错时，核心回 `method_failed`，它自己
//! 的 `message` 只有「方法执行失败。」（照连接的语言），程序说的那一句在 `data.message`（核心 `docs/zh/reference/packages.md`）。
//! 原来照开发包的 `Display` 写成 `method_failed: 方法执行失败。`，余额不足、key 不对这些真正的原因全丢了。

use miyu_ext::CoreError;

/// 有程序自己说的那一句（`data.message`）的照它；别的照 `<原因码>: <核心的话>`。
#[must_use]
pub fn why(error: &CoreError) -> String {
    error.data["message"]
        .as_str()
        .map(str::trim)
        .filter(|said| !said.is_empty())
        .map_or_else(|| error.to_string(), str::to_string)
}

#[cfg(test)]
mod tests;
