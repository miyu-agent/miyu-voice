use serde_json::{Map, json};

use super::{DEFAULT_WAKE_WORDS, Settings};

#[test]
fn wake_words_default_to_miyus_names_and_empty_lists_fall_back() {
    let settings = Settings::from_config(&Map::new());
    assert_eq!(settings.wake_words, DEFAULT_WAKE_WORDS.map(str::to_string));
    assert!((settings.threshold - 0.25).abs() < 1e-6);
    let empty = json!({"voice-assistant.wake_words": ["", " "]});
    assert_eq!(
        Settings::from_config(empty.as_object().unwrap())
            .wake_words
            .len(),
        4
    );
    let own =
        json!({"voice-assistant.wake_words": ["小美小美"], "voice-assistant.sensitivity": 0.4});
    let own = Settings::from_config(own.as_object().unwrap());
    assert_eq!(own.wake_words, ["小美小美"]);
    assert!((own.threshold - 0.4).abs() < 1e-6);
}
