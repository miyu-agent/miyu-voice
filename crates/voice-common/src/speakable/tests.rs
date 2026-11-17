use super::{sentences, to_speech};

#[test]
fn code_blocks_and_tables_are_named_not_read() {
    let markdown = "改好了：\n\n```rust\nfn main() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n跑一下 `cargo test` 看看。";
    assert_eq!(
        to_speech(markdown),
        "改好了： 一段代码。 一张表。 跑一下 cargo test 看看。"
    );
}

#[test]
fn links_read_their_text_and_bare_urls_their_domain() {
    assert_eq!(
        to_speech("看[这篇文章](https://example.com/a/b)，或者 https://www.rust-lang.org/learn 。"),
        "看这篇文章，或者 rust-lang.org 。"
    );
    assert_eq!(to_speech("<https://docs.rs/x>"), "docs.rs.");
}

#[test]
fn headings_and_list_items_become_sentences() {
    assert_eq!(
        to_speech("# 今天\n\n- 买菜\n- 写代码\n"),
        "今天。 买菜。 写代码。"
    );
    assert_eq!(to_speech("# Plan\n\n- shop\n- code"), "Plan. shop. code.");
}

#[test]
fn emoji_and_zero_width_characters_are_dropped() {
    assert_eq!(to_speech("好的👍\u{200b}，马上🚀！"), "好的，马上！");
    assert_eq!(to_speech("\u{200b}\u{200b}"), "", "nothing to say");
    assert_eq!(to_speech("😀🎉"), "");
}

#[test]
fn japanese_uses_its_own_words() {
    assert_eq!(
        to_speech("できました：\n\n```\nx\n```"),
        "できました： コードブロック。"
    );
}

#[test]
fn sentences_split_at_ends_and_short_ones_join_the_next() {
    // 「好的。」只有两个字，并进下一句；最后的「明白！」也太短，并到前一句后面。
    assert_eq!(
        sentences("好的。今天下午三点开会，记得带电脑。明白！"),
        ["好的。今天下午三点开会，记得带电脑。 明白！"]
    );
    assert_eq!(
        sentences("The build passed. Tests are green too. Ship it?"),
        ["The build passed.", "Tests are green too.", "Ship it?"].map(String::from)
    );
    assert!(sentences("。。").is_empty());
}

/// 紧贴中文、括号的裸网址也念域名（todolist 第 14 条：原来只按空格切词，「见https://…」「（https://…）」照原文念）；
/// 网址里的 `&`、`_` 让 Markdown 把字切成几段的，也不半截照念。
#[test]
fn bare_urls_next_to_chinese_or_brackets_read_their_domain() {
    assert_eq!(
        to_speech("见https://a.com/x?y=1的说明（https://www.b.com/p）。"),
        "见a.com的说明（b.com）。"
    );
    assert_eq!(
        to_speech("详见 https://weather.example.com/beijing?day=1&unit=c 的预报。"),
        "详见 weather.example.com 的预报。"
    );
    assert_eq!(
        to_speech("see (https://x.org/a_b_c), or https://y.org."),
        "see (x.org), or y.org."
    );
}

/// 末尾的参考链接那一段不念，正文里的链接留字（规则同网页 `web-demo/packages/voice/readable.js`，W-1205）。
#[test]
fn the_trailing_sources_section_is_not_read() {
    // 小标题 + 带说明的链接行
    assert_eq!(
        to_speech(
            "正文一句。\n\n## 参考资料\n1. [甲](https://a.com/x) — 甲的报道\n2. [乙](https://b.com) 乙的说明"
        ),
        "正文一句。"
    );
    // 没有小标题、只有链接的几行
    assert_eq!(
        to_speech("正文一句。\n\n- https://a.com/x?y=1\n- <https://b.com>\n[1]: https://c.com"),
        "正文一句。"
    );
    // 一行里的小标题加几个链接；英文的 Sources；加粗的小标题
    assert_eq!(
        to_speech("正文。\n来源：[甲](https://a.com)、[乙](https://b.com)"),
        "正文。"
    );
    assert_eq!(
        to_speech("Answer here.\n\n---\n\nSources:\n- [A](https://a.com)\n- [B](https://b.com)"),
        "Answer here."
    );
    let weather = [
        "今天北京晴，最高气温二十度。详见 https://weather.example.com/beijing?day=1&unit=c 的预报。",
        "",
        "**参考链接：**",
        "- [北京天气预报 - 中国天气网](https://www.weather.com.cn/weather/101010100.shtml?from=a)",
        "- [Beijing weather](https://example.org/w?q=beijing;x=1)",
    ]
    .join("\n");
    assert_eq!(
        to_speech(&weather),
        "今天北京晴，最高气温二十度。详见 weather.example.com 的预报。"
    );
}

/// 不该去掉的照念：结尾是带说明的链接列表、整段都是链接、参考小标题不在末尾。
#[test]
fn links_that_are_the_answer_are_still_read() {
    assert_eq!(
        to_speech("推荐两个：\n- [甲](https://a.com)：好用，适合新手\n- [乙](https://b.com)：快"),
        "推荐两个： 甲：好用，适合新手。 乙：快。"
    );
    assert_eq!(
        to_speech("- [甲](https://a.com)\n- [乙](https://b.com)"),
        "甲。 乙。"
    );
    assert_eq!(
        to_speech("## 参考资料\n- [甲](https://a.com)\n\n还有正文。"),
        "参考资料。 甲。 还有正文。"
    );
}

/// 引用角标（只有编号的链接）、脚注角标不念，图片念替代文字。
#[test]
fn citation_marks_are_not_read() {
    assert_eq!(
        to_speech(
            "看[官网](https://x.com/a?b)的说明[1](https://y.com)[^2]，![一张图](https://i.com/p.png)，还有 https://www.z.com/q?x=1。"
        ),
        "看官网的说明，一张图，还有 z.com。"
    );
}
