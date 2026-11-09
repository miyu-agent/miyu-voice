//! 她的回答边写边念（设计 20 第九节「规程」第 4 步）：会话推来的 `model.delta` 里正文那几块一段段接起来，凑出完整的一句就交出去
//! 念，第一句好了就开始放，不等她写完。在代码块里（三个反引号开、三个反引号关）不切，免得把一段代码切成两半念。一轮结束（`turn.ended`）把剩下
//! 的也交出去。只看正文：思考、工具调用不念。

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

/// 能切在哪：最后一个句末标点（中文的「。！？」、英文后面跟着空白的「. ! ?」、换行）之后；在代码块里的不切。
fn cut_point(text: &str) -> Option<usize> {
    let mut fence = false;
    let mut last = None;
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if text[at..].starts_with("```") {
            fence = !fence;
        }
        if fence {
            continue;
        }
        let end = match c {
            '。' | '！' | '？' | '…' | '\n' => true,
            '.' | '!' | '?' => chars.peek().is_some_and(|(_, next)| next.is_whitespace()),
            _ => false,
        };
        if end {
            last = Some(at + c.len_utf8());
        }
    }
    last
}

#[cfg(test)]
mod tests;
