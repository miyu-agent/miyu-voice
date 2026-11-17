use serde_json::json;

use super::{Plugged, choice, listed_in, name_in};

#[test]
fn only_packages_plugged_into_that_name_are_listed() {
    let answer = json!({"names": [
        {"name": "stt", "kind": "provides", "package": "voice-input", "ready": true, "methods": []},
        {"name": "stt.engine", "kind": "plugs", "package": "deepseek", "ready": true, "methods": ["transcribe"]},
        {"name": "stt.engine", "kind": "plugs", "package": "whisper", "ready": false, "methods": []},
        {"name": "tts.engine", "kind": "plugs", "package": "gptsovits", "ready": true, "methods": []},
    ]});
    assert_eq!(
        listed_in(&answer, "stt.engine"),
        [
            Plugged {
                package: "deepseek".into(),
                ready: true,
                name: None,
            },
            Plugged {
                package: "whisper".into(),
                ready: false,
                name: None,
            },
        ]
    );
    assert!(listed_in(&json!({}), "stt.engine").is_empty());
}

#[test]
fn a_package_not_running_says_so_in_the_dropdown() {
    let running = Plugged {
        package: "deepseek".into(),
        ready: true,
        name: None,
    };
    assert_eq!(
        choice(&running, "zh"),
        json!({"value": "deepseek", "name": "deepseek"})
    );
    let stopped = Plugged {
        ready: false,
        ..running
    };
    assert_eq!(choice(&stopped, "zh")["description"], "没在运行");
    assert_eq!(choice(&stopped, "fr")["description"], "Not running");
}

/// 下拉里写包的名字（照界面语言），值照旧是编号；问不到名字的写编号（施工 VO-32）。
#[test]
fn the_dropdown_shows_the_package_name_and_stores_its_id() {
    let named = Plugged {
        package: "deepseek-tools".into(),
        ready: true,
        name: name_in(&json!({"package": "deepseek-tools", "name": "DeepSeek 功能全家桶"})),
    };
    assert_eq!(
        choice(&named, "zh"),
        json!({"value": "deepseek-tools", "name": "DeepSeek 功能全家桶"})
    );
    assert_eq!(name_in(&json!({"package": "x"})), None);
    assert_eq!(name_in(&json!({"name": "  "})), None);
}
