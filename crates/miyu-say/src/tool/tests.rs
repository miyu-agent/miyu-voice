use super::{extension, seconds, short_hash, store, too_long};

#[test]
fn audio_is_stored_under_its_hash_and_no_temp_is_left() {
    let dir = std::env::temp_dir().join(format!("miyu-say-tool-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let name = format!("say-{}.mp3", short_hash(b"ID3"));
    let saved = store(&dir.join("audio"), &name, b"ID3").unwrap();
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
