use serde_json::{Map, json};

use super::{DEFAULT_WAKE_WORDS, Settings};

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
