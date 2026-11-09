use super::{short_hash, store};

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
