//! Markdown 转成念得出来的话、按句切（设计 20 第四节）：`tts.synthesize` 收 Markdown，`miyu-say` 在这里转，网页、终端、
//! 助手、IM 念出来的一样，社区引擎收到的已经是纯文字。
//!
//! - 代码块念「一段代码」，行内代码照念；表格念「一张表」；链接念文字，裸网址念域名；图片念替代文字；
//! - 表情符号、零宽字符不念；列表、标题各当一句；
//! - 转完是空的不合成（旧版模型吐零宽空格，发出过空白的语音）；
//! - 按句切：句号、问号、叹号（中英日都认），太短的并进下一句。
//!
//! 念成什么的用词是数据：`data/speakable.json`，照这段话的语言挑。

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
    for event in Parser::new_ext(
        markdown,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH,
    ) {
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
                    // 自动链接、文字就是网址的，念域名。
                    if text.trim().is_empty() || text.trim() == url {
                        out.push_str(&domain(&url));
                    } else {
                        out.push_str(&text);
                    }
                }
            }
            Event::Text(text) | Event::Code(text) => {
                let text: String = text.chars().filter(|c| !unspoken(*c)).collect();
                match link.as_mut() {
                    Some((_, collected)) => collected.push_str(&text),
                    None => out.push_str(&bare_urls(&text)),
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

/// 一段字里的裸网址换成域名。
fn bare_urls(text: &str) -> String {
    text.split(' ')
        .map(|word| {
            if word.starts_with("http://") || word.starts_with("https://") {
                domain(word)
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
