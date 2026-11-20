use serde_json::json;

use super::{ends, extension, seconds, short_hash, store, too_long};

#[test]
fn audio_is_stored_under_its_hash_and_no_temp_is_left() {
    let dir = std::env::temp_dir().join(format!("miyu-say-tool-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let name = format!("say-{}.mp3", short_hash(b"ID3"));
    std::fs::create_dir_all(&dir).unwrap();
    let saved = store(&dir, &name, b"ID3").unwrap();
    assert_eq!(std::fs::read(&saved).unwrap(), b"ID3");
    let left: Vec<_> = std::fs::read_dir(dir.join("audio")).unwrap().collect();
    assert_eq!(left.len(), 1);
    assert_eq!(short_hash(b"ID3").len(), 12);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// 起名照真实的格式（施工 VO-35：DeepSeek 引擎出的是 wav，以前一律叫 .mp3）。
#[test]
fn the_file_name_follows_the_real_format() {
    assert_eq!(extension("audio/mpeg"), "mp3");
    assert_eq!(extension("audio/wav"), "wav");
    assert_eq!(extension("audio/x-wav"), "wav");
    assert_eq!(extension("audio/ogg"), "ogg");
}

/// 超了场所的语音上限：说清这段多长、上限多少，让她重说（主人 11-17 定：不退回发文件、不切）。
#[test]
fn too_long_says_both_lengths() {
    assert_eq!(seconds(2_345), "2.3 s");
    assert_eq!(
        too_long(75_200, 60_000),
        "Too long for a voice message here: 75.2 s, the limit is 60.0 s. Say it again in parts or more briefly."
    );
}

/// 工作区里的 `audio` 是指到外面的链接：不往外写（审计 #119）。
#[cfg(unix)]
#[test]
fn audio_folder_linked_outside_the_workspace_is_refused() {
    let root = std::env::temp_dir().join(format!("miyu-say-escape-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let workspace = root.join("workspace");
    let outside = root.join("outside");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, workspace.join("audio")).unwrap();
    let stored = store(&workspace, "say-x.mp3", b"ID3");
    assert!(stored.is_err(), "{stored:?}");
    assert_eq!(
        std::fs::read_dir(&outside).unwrap().count(),
        0,
        "nothing written outside"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

/// 另一个线程正在写同名的临时文件：照样存成，也不删掉它的（审计 #119：以前临时文件名只带进程号，撞了就把对方的删了）。
#[test]
fn a_temp_file_already_there_is_left_alone() {
    let dir = std::env::temp_dir().join(format!("miyu-say-temp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let audio = dir.join("audio");
    std::fs::create_dir_all(&audio).unwrap();
    let name = "say-same.mp3";
    let theirs = audio.join(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&theirs, b"half written").unwrap();
    let stored = store(&dir, name, b"ID3");
    assert!(stored.is_ok(), "{stored:?}");
    assert!(theirs.exists(), "the other writer's temp file stays");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// 说出来的默认结束这一轮（主人 11-20 验收第 5 条，照表情包）；写了 `end: false` 的接着干活；`file_only` 要拿路径接着用，没写的
/// 不结束，写了 `end: true` 的照写的。
#[test]
fn speaking_ends_the_turn_unless_end_is_false_or_it_only_saves_a_file() {
    assert!(ends(&json!({"text": "好的"})));
    assert!(!ends(&json!({"text": "好的", "end": false})));
    assert!(!ends(&json!({"text": "好的", "file_only": true})));
    assert!(ends(
        &json!({"text": "好的", "file_only": true, "end": true})
    ));
}

/// 工具说明三句以内，写着默认结束、`end` 的默认值（提示词登记簿记着这一份的 token）。
#[test]
fn the_tool_says_it_ends_the_turn() {
    let description = super::DESCRIPTION;
    assert!(description.contains("ends your turn unless end is false"));
    assert!(
        description.matches(". ").count() <= 2,
        "at most three sentences"
    );
    assert_eq!(
        super::schema()["properties"]["end"]["description"],
        "Default true, false with file_only."
    );
}
