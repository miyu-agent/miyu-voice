use std::sync::mpsc;
use std::time::Duration;

use super::{Busy, MicLock, want_file};

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("miyu-voice-lock-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn a_second_dictation_is_busy_and_the_first_one_releases() {
    let dir = temp("two");
    let first = MicLock::for_dictation_in(&dir).unwrap();
    assert_eq!(MicLock::for_dictation_in(&dir).unwrap_err(), Busy);
    assert!(!want_file(&dir).exists(), "the want mark is taken back");
    drop(first);
    assert!(MicLock::for_dictation_in(&dir).is_ok());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_assistant_yields_to_dictation() {
    let dir = temp("yield");
    let assistant = MicLock::for_assistant_in(&dir).unwrap();
    let (done, finished) = mpsc::channel();
    let dictation_dir = dir.clone();
    let dictation = std::thread::spawn(move || {
        let got = MicLock::for_dictation_in(&dictation_dir);
        done.send(()).unwrap();
        got
    });
    // 助手照 `wanted` 看到有人要，丢掉锁。
    while !want_file(&dir).exists() {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        MicLock::for_assistant_in(&dir).is_none(),
        "no new lock while wanted"
    );
    drop(assistant);
    finished.recv_timeout(Duration::from_secs(5)).unwrap();
    let held = dictation.join().unwrap().unwrap();
    assert!(
        MicLock::for_assistant_in(&dir).is_none(),
        "dictation holds it"
    );
    drop(held);
    assert!(
        MicLock::for_assistant_in(&dir).is_some(),
        "the assistant gets it back"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
