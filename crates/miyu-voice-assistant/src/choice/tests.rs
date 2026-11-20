use serde_json::json;

use super::Choice;

/// 没设的照默认，`venue.session` 不带人格、预设（核心照自己的默认造）；设了的带上；换了设置的另开（`fresh`）。
#[test]
fn only_chosen_persona_and_preset_go_into_venue_session() {
    let none = Choice::from_config(json!({}).as_object().unwrap());
    assert_eq!(none, Choice::default());
    assert_eq!(
        none.session_params("voice:local", "voice:owner", false),
        json!({"venue": "voice:local", "kind": "private", "peer": "voice:owner", "images": false})
    );
    let chosen = Choice::from_config(
        json!({"voice-assistant.persona": " coder ", "voice-assistant.preset": "full"})
            .as_object()
            .unwrap(),
    );
    assert_eq!(
        chosen.session_params("voice:local", "voice:owner", true),
        json!({"venue": "voice:local", "kind": "private", "peer": "voice:owner", "images": false,
               "persona": "coder", "preset": "full", "fresh": true})
    );
}

/// 记下的那一份读得回来；没记过的当默认（以前的版本只会照默认开，升上来、没设的不另开）。
#[test]
fn the_choice_last_opened_with_is_remembered() {
    let dir = std::env::temp_dir().join(format!("miyu-va-choice-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(Choice::load(&dir), Choice::default());
    let chosen = Choice {
        persona: "coder".to_string(),
        preset: String::new(),
    };
    chosen.save(&dir).unwrap();
    assert_eq!(Choice::load(&dir), chosen);
    std::fs::write(dir.join(super::FILE), "not json").unwrap();
    assert_eq!(Choice::load(&dir), Choice::default());
    std::fs::remove_dir_all(&dir).unwrap();
}
