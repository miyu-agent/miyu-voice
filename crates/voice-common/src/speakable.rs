//! Markdown 转成念得出来的话、按句切（设计 20 第四节）：`tts.synthesize` 收 Markdown，`miyu-say` 在这里转，网页、终端、
//! 助手、IM 念出来的一样，社区引擎收到的已经是纯文字。
//!
//! - 代码块念「一段代码」，行内代码照念；表格念「一张表」；链接念文字，裸网址念域名；图片念替代文字；
//! - 网址不念原文（todolist 第 14 条，规则同网页 `web-demo/packages/voice/readable.js`，W-1205）：裸网址紧贴中文、括号的
//!   也念域名；只有编号的链接（`[1](网址)` 引用角标）、`[^1]` 脚注角标不念；末尾的参考链接那一段整段不念（`speakable/sources.rs`）；
//! - 表情符号、零宽字符不念；列表、标题各当一句；
//! - 转完是空的不合成（旧版模型吐零宽空格，发出过空白的语音）；
//! - 按句切：句号、问号、叹号（中英日都认），太短的并进下一句。
//!
//! 念成什么的用词是数据：`data/speakable.json`，照这段话的语言挑。

mod sources;

use std::sync::OnceLock;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use serde_json::Value;

/// 比它短的句子（字数）并进下一句：一两个字的句子单独合成，停顿不自然，还多一次请求。
const SHORTEST_SENTENCE: usize = 6;

/// 转写用词（`data/speakable.json`）。
fn words() -> &'static Value {
    static WORDS: OnceLock<Value> = OnceLock::new();
    WORDS.get_or_init(|| {
        serde_json::from_str(include_str!("../data/speakable.json")).unwrap_or(Value::Null)
    })
}

/// 这段话照哪种语言念：有假名的日语，有汉字的中文，别的英语。
fn language(text: &str) -> &'static str {
    if text.chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c)) {
        "ja"
    } else if text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)) {
        "zh"
    } else {
        "en"
    }
}

fn word(language: &str, key: &str) -> String {
    words()[language][key]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// Markdown 转成一段念得出来的话；念不出东西的交回空的。
#[must_use]
pub fn to_speech(markdown: &str) -> String {
    // 整段都是参考链接的（问的就是链接）照念。
    let kept = sources::without(markdown);
    let markdown = if kept.trim().is_empty() {
        markdown
    } else {
        kept.as_str()
    };
    let lang = language(markdown);
    let end = word(lang, "sentence_end");
    let mut out = String::new();
    // 一块（段落、标题、列表项）念完补一个句末标点，好让切句和停顿落在这里。
    let close = |out: &mut String| {
        let trimmed = out.trim_end();
        // 已经以标点（句号、冒号、引号……）收尾的不补。
        if trimmed.ends_with(char::is_alphanumeric) {
            out.truncate(trimmed.len());
            out.push_str(&end);
        }
        out.push(' ');
    };
    let mut link: Option<(String, String)> = None;
    let mut skip_depth = 0usize;
    // 连着的几段字攒在一起再换网址：Markdown 在 `&`、`_`、`[` 这些地方把字切开，一个网址会分在几段里。
    let mut run = String::new();
    for event in Parser::new_ext(
        markdown,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH,
    ) {
        let text_outside_link = matches!(event, Event::Text(_) | Event::Code(_)) && link.is_none();
        if !text_outside_link && !run.is_empty() {
            out.push_str(&spoken_text(&run));
            run.clear();
        }
        match event {
            Event::Start(Tag::CodeBlock(_)) => {
                close(&mut out);
                out.push_str(&word(lang, "code"));
                close(&mut out);
                skip_depth += 1;
            }
            Event::Start(Tag::Table(_)) => {
                close(&mut out);
                out.push_str(&word(lang, "table"));
                close(&mut out);
                skip_depth += 1;
            }
            Event::End(TagEnd::CodeBlock | TagEnd::Table) => {
                skip_depth = skip_depth.saturating_sub(1);
            }
            _ if skip_depth > 0 => {}
            Event::Start(Tag::Link { dest_url, .. }) => {
                link = Some((dest_url.to_string(), String::new()));
            }
            Event::End(TagEnd::Link) => {
                if let Some((url, text)) = link.take() {
                    // 自动链接、文字就是网址的，念域名；只有编号的（引用角标）不念。
                    if text.trim().is_empty() || text.trim() == url {
                        out.push_str(&domain(&url));
                    } else if !is_citation(&text) {
                        out.push_str(&text);
                    }
                }
            }
            Event::Text(text) | Event::Code(text) => {
                let text: String = text.chars().filter(|c| !unspoken(*c)).collect();
                match link.as_mut() {
                    Some((_, collected)) => collected.push_str(&text),
                    None => run.push_str(&text),
                }
            }
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            Event::End(
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item | TagEnd::BlockQuote(_),
            ) => close(&mut out),
            // 图片：替代文字照 Text 进来；HTML、脚注这些不念。
            _ => {}
        }
    }
    out.push_str(&spoken_text(&run));
    if !out.chars().any(char::is_alphanumeric) {
        return String::new();
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 一段念得出来的话按句切，太短的并进下一句。
#[must_use]
pub fn sentences(speech: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    for c in speech.chars() {
        current.push(c);
        if is_sentence_end(c) {
            pieces.push(std::mem::take(&mut current));
        }
    }
    pieces.push(current);
    let mut merged: Vec<String> = Vec::new();
    let mut pending = String::new();
    for piece in pieces {
        pending.push_str(&piece);
        let spoken = pending.chars().filter(|c| c.is_alphanumeric()).count();
        if spoken >= SHORTEST_SENTENCE {
            merged.push(pending.trim().to_string());
            pending.clear();
        }
    }
    let rest = pending.trim();
    if rest.chars().any(char::is_alphanumeric) {
        match merged.last_mut() {
            Some(last) => {
                last.push(' ');
                last.push_str(rest);
            }
            None => merged.push(rest.to_string()),
        }
    }
    merged
}

fn is_sentence_end(c: char) -> bool {
    matches!(c, '。' | '？' | '！' | '.' | '?' | '!' | '…')
}

/// 不念的字：表情符号、旗子、变体选择、零宽字符。
fn unspoken(c: char) -> bool {
    matches!(c,
        '\u{200b}'..='\u{200f}' | '\u{2060}' | '\u{feff}' | '\u{fe0e}' | '\u{fe0f}'
        | '\u{2600}'..='\u{27bf}' | '\u{1f000}'..='\u{1faff}' | '\u{e0020}'..='\u{e007f}')
}

/// 一段字里的裸网址换成域名（紧贴中文、括号的也算），`[^1]` 脚注角标去掉。
fn spoken_text(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some(len) = bare_url_len(rest) {
            out.push_str(&domain(&rest[..len]));
            rest = &rest[len..];
        } else if let Some(after) = sources::footnote_end(rest) {
            rest = after;
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// `text` 以裸网址打头的，交回网址有多长（字节）：到网址用不着的字（空白、尖括号、括号、中文）为止，末尾的
/// `.,;:!?` 当句子的标点留下（同 `readable.js` 的 `BARE`，W-1205）。
fn bare_url_len(text: &str) -> Option<usize> {
    let scheme = ["https://", "http://"]
        .into_iter()
        .find(|scheme| text.starts_with(scheme))?;
    let body = text[scheme.len()..]
        .find(|c: char| !(c.is_ascii_alphanumeric() || "-._~:/?#[]@!$&'*+,;=%".contains(c)))
        .unwrap_or(text.len() - scheme.len());
    let url = text[..scheme.len() + body].trim_end_matches(['.', ',', ';', ':', '!', '?']);
    (url.len() > scheme.len()).then_some(url.len())
}

/// 只有编号的链接字（`1`、`[1]`、`¹`）：引用角标，不念（同 `readable.js` 的 `CITATION`）。
fn is_citation(text: &str) -> bool {
    text.chars()
        .all(|c| c.is_whitespace() || c.is_ascii_digit() || "[]¹²³⁴⁵⁶⁷⁸⁹⁰,，、-".contains(c))
}

/// 网址的域名，去掉 `www.`。
fn domain(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = host.rsplit_once('@').map_or(host, |(_, host)| host);
    host.trim_start_matches("www.").to_string()
}

#[cfg(test)]
mod tests;
