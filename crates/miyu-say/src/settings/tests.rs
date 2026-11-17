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

/// 引擎和内置的排在一张下拉里（施工 VO-32）：值不是内置的那几个的，就是接进 `tts.engine` 的包的编号，挑了就算配好；
/// 清空了的不算。
#[test]
fn a_community_engine_is_the_package_id_in_the_same_list() {
    let picked = Settings::from_config(&config(&json!({"say.engine": " deepseek-tools "})));
    assert_eq!(picked.engine, "deepseek-tools");
    assert!(picked.configured());
    let cleared = Settings::from_config(&config(&json!({"say.engine": ""})));
    assert!(!cleared.configured(), "an empty engine is not ready");
}

/// 下拉里内置的几家和 [`BUILT_IN`] 对得上，照界面语言挑名字，没有的语言用英文。
#[test]
fn built_in_choices_follow_the_language_and_match_the_built_in_list() {
    let zh = super::built_in_choices("zh");
    let values: Vec<&str> = zh.iter().filter_map(|c| c["value"].as_str()).collect();
    assert_eq!(values, super::BUILT_IN);
    assert_eq!(zh[1]["name"], "MiMo（小米）");
    assert!(zh[2]["description"].as_str().unwrap().contains("硅基流动"));
    assert!(zh[0].get("description").is_none());
    assert_eq!(super::built_in_choices("fr")[1]["name"], "MiMo (Xiaomi)");
}
