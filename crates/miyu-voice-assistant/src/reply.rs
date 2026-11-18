//! 她的回答边写边念（设计 20 第九节「规程」第 4 步）：会话推来的 `model.delta` 里正文那几块一段段接起来，凑出完整的一句就交出去
//! 念，第一句好了就开始放，不等她写完。在代码块里（照 `CommonMark` 认围栏）不切，免得把一段代码切成两半念；表格（`|` 开头的行）
//! 整张一起交，一行一行交的每行单看不是表格，竖线会被念出来。一轮结束（`turn.ended`）把剩下的也交出去。只看正文：思考、
//! 工具调用不念。

/// 一轮里正在写的回答。
#[derive(Debug, Default)]
pub struct Reply {
    /// 正在写的那几块里，哪几块是正文（`model.delta` 的 `index`）。
    text_blocks: Vec<u64>,
    /// 还没交出去的正文。
    pending: String,
}

impl Reply {
    /// 一块开始（`start` 是块的种类）。
    pub fn start(&mut self, index: u64, kind: &str) {
        if kind == "text" {
            self.text_blocks.push(index);
        }
    }

    /// 一块接上的字：是正文的接进来，交回凑够了的整句（可能没有）。
    pub fn text(&mut self, index: u64, text: &str) -> Option<String> {
        if !self.text_blocks.contains(&index) {
            return None;
        }
        self.pending.push_str(text);
        let cut = cut_point(&self.pending)?;
        let ready: String = self.pending.drain(..cut).collect();
        let ready = ready.trim().to_string();
        (!ready.is_empty()).then_some(ready)
    }

    /// 一轮完了（或者一次模型响应写完、块的编号要从头数了）：剩下的全交出去。
    pub fn finish(&mut self) -> Option<String> {
        self.text_blocks.clear();
        let rest = std::mem::take(&mut self.pending);
        let rest = rest.trim();
        (!rest.is_empty()).then(|| rest.to_string())
    }
}

/// 能切在哪：最后一个句末标点（中文的「。！？」、英文后面跟着空白的「. ! ?」、换行）之后；代码块里、表格的行里不切。
fn cut_point(text: &str) -> Option<usize> {
    // 开着的围栏：哪种字符、几个（审计 #124：原来逐字找三个反引号翻转，四个反引号翻两次）。
    let mut fence: Option<(char, usize)> = None;
    let mut last = None;
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let at = start;
        start += line.len();
        match (fence, fence_marker(line)) {
            (None, Some((mark, count, info))) => {
                // 反引号的围栏，后面的说明里不能再有反引号（那是行内代码）。
                if mark != '`' || !info.contains('`') {
                    fence = Some((mark, count));
                }
                continue;
            }
            (Some((open, opened)), Some((mark, count, info)))
                if mark == open && count >= opened && info.trim().is_empty() =>
            {
                fence = None;
            }
            _ => {}
        }
        if fence.is_some() || line.trim_start().starts_with('|') {
            continue;
        }
        let mut chars = line.char_indices().peekable();
        while let Some((offset, c)) = chars.next() {
            let end = match c {
                '。' | '！' | '？' | '…' | '\n' => true,
                '.' | '!' | '?' => chars.peek().is_some_and(|(_, next)| next.is_whitespace()),
                _ => false,
            };
            if end {
                last = Some(at + offset + c.len_utf8());
            }
        }
    }
    last
}

/// 一行是不是围栏（`CommonMark`）：前面最多 3 个空格，接着 3 个以上连着的反引号或 `~`。交回（哪种字符、几个、后面剩下的）。
fn fence_marker(line: &str) -> Option<(char, usize, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let mark = rest.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let count = rest.len() - rest.trim_start_matches(mark).len();
    (count >= 3).then(|| (mark, count, &rest[count..]))
}

#[cfg(test)]
mod tests;
