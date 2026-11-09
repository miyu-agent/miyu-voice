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
