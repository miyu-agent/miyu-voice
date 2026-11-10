use serde_json::json;

use super::{Notice, Step, save_avatar, tail, text};

#[test]
fn every_step_reads_in_the_persons_language() {
    let heard = Notice::Heard("明天几点开会".to_string());
    assert_eq!(
        text(&heard, "zh", "Miyu"),
        ("在想…".to_string(), "你：明天几点开会".to_string())
    );
    assert_eq!(text(&heard, "en", "Miyu").1, "You: 明天几点开会");
    assert_eq!(text(&Notice::Listening, "ja", "Miyu").0, "聞いています");
    assert_eq!(
        text(&Notice::NotHeard, "fr", "Miyu").0,
        "Didn't catch that",
        "other languages fall back to English"
    );
    let speaking = Notice::Reply {
        text: "好的。".to_string(),
        done: false,
    };
    assert_eq!(
        text(&speaking, "zh", "小美").0,
        "小美 在说…",
        "signed with the persona's name"
    );
    let done = Notice::Reply {
        text: "好的。".to_string(),
        done: true,
    };
    assert_eq!(
        text(&done, "zh", "小美"),
        ("小美".to_string(), "好的。".to_string())
    );
}

#[test]
fn trouble_reaching_her_names_her() {
    let (title, _) = text(&Notice::Trouble(Step::Sending, String::new()), "en", "小美");
    assert_eq!(title, "Can't reach 小美");
}

#[test]
fn a_speech_failure_says_where_the_reply_went() {
    let (title, body) = text(
        &Notice::Trouble(Step::Speech, "no key".to_string()),
        "zh",
        "Miyu",
    );
    assert_eq!(title, "念不出来");
    assert!(
        body.starts_with("no key\n") && body.contains("通知"),
        "{body}"
    );
    let (_, body) = text(
        &Notice::Trouble(Step::Microphone, "busy".to_string()),
        "en",
        "Miyu",
    );
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

#[test]
fn an_avatar_is_saved_once_per_version_and_none_means_no_icon() {
    let dir = std::env::temp_dir().join(format!("miyu-avatar-test-{}", std::process::id()));
    let answer =
        json!({"avatar": "sha256:ab12cd", "media_type": "image/png", "data": "iVBORw0KGgo="});
    let path = save_avatar(&answer, &dir).unwrap();
    assert_eq!(path, dir.join("avatar-sha256ab12cd.png"));
    assert_eq!(std::fs::read(&path).unwrap(), b"\x89PNG\r\n\x1a\n");
    let none = json!({"avatar": null, "media_type": null, "data": null});
    assert_eq!(save_avatar(&none, &dir), None);
    let gif = json!({"avatar": "v", "media_type": "image/gif", "data": "R0lG"});
    assert_eq!(
        save_avatar(&gif, &dir),
        None,
        "only what the core accepts as an avatar"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
