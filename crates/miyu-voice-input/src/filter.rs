//! 识别前后的过滤（设计 20 第五节「过滤」，旧版的教训）：短于 0.5 秒的不识别；识别出的有效字少于 2 个的不算听清。
//! 没听清不算出错：`text` 空、`clear` 是 `false`，调的一方照它说「没听清」或者不交给她（第三节「形状」）。

/// 短于它的不识别（毫秒）。
pub const SHORTEST_MS: u64 = 500;
/// 有效字少于它的不算听清。
pub const FEWEST_CHARS: usize = 2;

/// 这么长的声音值不值得识别。
#[must_use]
pub fn long_enough(duration_ms: u64) -> bool {
    duration_ms >= SHORTEST_MS
}

/// 识别出的字算不算听清：数字母、数字、汉字这类，标点、空白、符号不算。
#[must_use]
pub fn clear(text: &str) -> bool {
    text.chars().filter(|c| c.is_alphanumeric()).count() >= FEWEST_CHARS
}

/// 识别结果去掉头尾的空白；不算听清的交回空的。
#[must_use]
pub fn kept(text: &str) -> Option<String> {
    let text = text.trim();
    clear(text).then(|| text.to_string())
}

#[cfg(test)]
mod tests;
