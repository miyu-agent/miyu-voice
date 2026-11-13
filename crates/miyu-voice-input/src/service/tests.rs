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
