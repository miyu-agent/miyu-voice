use serde_json::{Map, Value, json};

use super::Settings;

fn config(value: &Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn nothing_set_is_minimax_and_not_configured() {
    let settings = Settings::from_config(&Map::new());
    assert_eq!(settings.engine, "minimax");
    assert!(!settings.configured(), "no key yet");
}

#[test]
fn a_key_makes_the_chosen_engine_ready() {
    let settings = Settings::from_config(&config(&json!({
        "say.engine": "mimo",
        "say.mimo.key": "sk-x",
    })));
    assert!(settings.configured());
    let openai = Settings::from_config(&config(&json!({"say.engine": "openai"})));
    assert!(
        openai.configured(),
        "local servers need no key; address and model have defaults"
    );
}

#[test]
fn personas_get_their_own_voice_and_others_the_default() {
    let settings = Settings::from_config(&config(&json!({
        "say.voice": "female-shaonv",
        "say.speed": 1.1,
        "say.personas": [{"persona": "miyu", "voice": "female-tianmei", "speed": 0.9}],
    })));
    assert_eq!(
        settings.voice_for(Some("miyu")),
        ("female-tianmei".to_string(), 0.9)
    );
    let (voice, speed) = settings.voice_for(Some("other"));
    assert_eq!(voice, "female-shaonv");
    assert!((speed - 1.1).abs() < 1e-6);
    assert_eq!(settings.voice_for(None).0, "female-shaonv");
}

#[test]
fn a_community_engine_is_ready_once_a_package_is_picked() {
    let unpicked = Settings::from_config(&config(&json!({"say.engine": "community"})));
    assert!(!unpicked.configured(), "no package picked yet");
    let picked = Settings::from_config(&config(&json!({
        "say.engine": "community",
        "say.community.package": " deepseek ",
    })));
    assert_eq!(picked.community, "deepseek");
    assert!(picked.configured());
}
