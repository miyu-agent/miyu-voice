use super::Reply;

#[test]
fn whole_sentences_go_out_as_soon_as_they_end() {
    let mut reply = Reply::default();
    reply.start(0, "text");
    assert_eq!(reply.text(0, "好的，"), None);
    assert_eq!(
        reply.text(0, "三点开会。明天"),
        Some("好的，三点开会。".to_string())
    );
    assert_eq!(reply.text(0, "见！"), Some("明天见！".to_string()));
    assert_eq!(reply.finish(), None);
}

#[test]
fn english_needs_a_space_after_the_stop() {
    let mut reply = Reply::default();
    reply.start(0, "text");
    assert_eq!(
        reply.text(0, "Version 2.5 is out"),
        None,
        "2.5 is not a sentence end"
    );
    assert_eq!(
        reply.text(0, ". Try it"),
        Some("Version 2.5 is out.".to_string()),
        "a stop followed by a space ends the sentence"
    );
    assert_eq!(reply.text(0, " now"), None);
    assert_eq!(reply.finish(), Some("Try it now".to_string()));
}

#[test]
fn thinking_and_tool_calls_are_not_read() {
    let mut reply = Reply::default();
    reply.start(0, "thinking");
    assert_eq!(reply.text(0, "嗯。让我想想。"), None);
    reply.start(1, "tool_call");
    assert_eq!(reply.text(1, "{\"path\":\"a.\"}\n"), None);
    reply.start(2, "text");
    assert_eq!(reply.text(2, "查好了。"), Some("查好了。".to_string()));
}

#[test]
fn code_blocks_are_not_cut_in_half() {
    let mut reply = Reply::default();
    reply.start(0, "text");
    assert_eq!(
        reply.text(0, "改成这样：\n```\nlet a = 1;\n"),
        Some("改成这样：".to_string())
    );
    assert_eq!(reply.text(0, "a.max(2);\n"), None, "inside the fence");
    assert_eq!(
        reply.text(0, "```\n就好了。"),
        Some("```\nlet a = 1;\na.max(2);\n```\n就好了。".to_string())
    );
}
