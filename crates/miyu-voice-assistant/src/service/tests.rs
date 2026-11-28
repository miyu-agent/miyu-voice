use serde_json::{Map, json};

use super::{DEFAULT_WAKE_WORDS, Settings, UTTERANCES_KEPT, utterance_queue};

#[test]
fn wake_words_default_to_ni_hao_ni_hao_and_empty_lists_fall_back() {
    // 项目主人 2026-11-20 验收：默认只留「你好你好」。
    assert_eq!(DEFAULT_WAKE_WORDS.as_slice(), ["你好你好"].as_slice());
    let settings = Settings::from_config(&Map::new());
    assert_eq!(settings.wake_words, ["你好你好"]);
    assert!((settings.threshold - 0.25).abs() < 1e-6);
    let empty = json!({"voice-assistant.wake_words": ["", " "]});
    assert_eq!(
        Settings::from_config(empty.as_object().unwrap()).wake_words,
        ["你好你好"]
    );
    let own =
        json!({"voice-assistant.wake_words": ["小美小美"], "voice-assistant.sensitivity": 0.4});
    let own = Settings::from_config(own.as_object().unwrap());
    assert_eq!(own.wake_words, ["小美小美"]);
    assert!((own.threshold - 0.4).abs() < 1e-6);
}

/// 施工 VO-49：认字那头卡住（`stt` 半天不回），收麦线程照样一段段交。原来的通道没有上限，交多少留多少；现在最多留 30 段，
/// 留下的是最新的。
#[test]
fn a_stuck_recognizer_keeps_only_the_newest_utterances() {
    let (mut utterances, heard) = utterance_queue();
    for n in 0..100u8 {
        utterances.send(vec![f32::from(n); 16_000]);
    }
    assert_eq!(heard.queued(), UTTERANCES_KEPT);
    assert_eq!(UTTERANCES_KEPT, 30);
    let first = heard.try_recv().unwrap();
    assert!((first[0] - 70.0).abs() < f32::EPSILON);
}
