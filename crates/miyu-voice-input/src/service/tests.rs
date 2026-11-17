use serde_json::{Map, json};

use super::{Settings, number, only};

#[test]
fn settings_default_to_chinese_and_the_built_in_engine() {
    let settings = Settings::from_config(&Map::new());
    assert_eq!(settings.language, "zh");
    assert!(settings.engine.is_empty());
    assert!(settings.microphone.is_empty());
    assert!(settings.chime, "chimes are on out of the box");
}

#[test]
fn chimes_can_be_turned_off() {
    let mut config = Map::new();
    config.insert("voice-input.chime".to_string(), json!(false));
    assert!(!Settings::from_config(&config).chime);
}

#[test]
fn numbers_take_defaults_and_ranges() {
    assert_eq!(
        number(&json!({}), "max_ms", 60_000, 1_000, 300_000).unwrap(),
        60_000
    );
    assert_eq!(
        number(&json!({"max_ms": 5000}), "max_ms", 60_000, 1_000, 300_000).unwrap(),
        5000
    );
    let error = number(
        &json!({"max_ms": 900_000}),
        "max_ms",
        60_000,
        1_000,
        300_000,
    )
    .unwrap_err();
    assert_eq!(error.code.as_deref(), Some("bad_params"));
    assert!(only(&json!({"audio": "x", "lang": "zh"}), &["audio", "language"]).is_err());
}

/// 没听到说话（施工 VO-33）：照「没听清」回、多 `speech: false`；录本机麦克风的带设备名和为什么停，空的设备名不带。
#[test]
fn nothing_heard_is_an_unclear_answer_with_the_device() {
    assert_eq!(
        super::nothing_heard("zh", 5100, Some("fifine Microphone Mono"), Some("silence")),
        serde_json::json!({"text": "", "language": "zh", "duration_ms": 5100, "clear": false,
            "speech": false, "device": "fifine Microphone Mono", "ended": "silence"})
    );
    assert_eq!(
        super::nothing_heard("auto", 800, Some(""), None),
        serde_json::json!({"text": "", "language": "auto", "duration_ms": 800, "clear": false, "speech": false})
    );
}
