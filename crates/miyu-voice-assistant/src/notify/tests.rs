use super::{Notice, Step, tail, text};

#[test]
fn every_step_reads_in_the_persons_language() {
    let heard = Notice::Heard("明天几点开会".to_string());
    assert_eq!(
        text(&heard, "zh"),
        ("在想…".to_string(), "你：明天几点开会".to_string())
    );
    assert_eq!(text(&heard, "en").1, "You: 明天几点开会");
    assert_eq!(text(&Notice::Listening, "ja").0, "聞いています");
    assert_eq!(
        text(&Notice::NotHeard, "fr").0,
        "Didn't catch that",
        "other languages fall back to English"
    );
    let speaking = Notice::Reply {
        text: "好的。".to_string(),
        done: false,
    };
    assert_eq!(text(&speaking, "zh").0, "Miyu 在说…");
    let done = Notice::Reply {
        text: "好的。".to_string(),
        done: true,
    };
    assert_eq!(
        text(&done, "zh"),
        ("Miyu".to_string(), "好的。".to_string())
    );
}

#[test]
fn a_speech_failure_says_where_the_reply_went() {
    let (title, body) = text(&Notice::Trouble(Step::Speech, "no key".to_string()), "zh");
    assert_eq!(title, "念不出来");
    assert!(
        body.starts_with("no key\n") && body.contains("通知"),
        "{body}"
    );
    let (_, body) = text(&Notice::Trouble(Step::Microphone, "busy".to_string()), "en");
    assert_eq!(body, "busy");
}

#[test]
fn long_replies_keep_the_end() {
    assert_eq!(tail("abc", 5), "abc");
    assert_eq!(tail("一二三四五六", 3), "…四五六");
}

#[test]
fn only_settled_notices_pop_where_they_cannot_be_replaced() {
    assert!(Notice::Listening.passing());
    assert!(
        Notice::Reply {
            text: String::new(),
            done: false
        }
        .passing()
    );
    assert!(
        !Notice::Reply {
            text: String::new(),
            done: true
        }
        .passing()
    );
    assert!(!Notice::Heard(String::new()).passing());
    assert!(!Notice::ConversationOff.passing());
}
