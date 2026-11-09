use std::sync::mpsc;
use std::time::Duration;

use super::{LockError, MicLock, want_file, wanted_in};

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("miyu-voice-lock-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn a_second_dictation_is_busy_and_the_first_one_releases() {
    let dir = temp("two");
    let first = MicLock::for_dictation_in(&dir).unwrap();
    assert_eq!(
        MicLock::for_dictation_in(&dir).unwrap_err(),
        LockError::Busy
    );
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

#[test]
fn a_stale_want_mark_is_cleared() {
    let dir = temp("stale");
    std::fs::create_dir_all(&dir).unwrap();
    let want = want_file(&dir);
    std::fs::File::create(&want).unwrap();
    assert!(wanted_in(&dir), "fresh");
    let old = std::time::SystemTime::now() - Duration::from_secs(10);
    std::fs::File::options()
        .write(true)
        .open(&want)
        .unwrap()
        .set_modified(old)
        .unwrap();
    assert!(
        !wanted_in(&dir),
        "older than 3 seconds counts as left behind"
    );
    assert!(!want.exists(), "and is removed");
    assert!(MicLock::for_assistant_in(&dir).is_some());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[test]
fn an_own_folder_that_is_too_open_is_tightened() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp("open");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(MicLock::for_dictation_in(&dir).is_ok());
    let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[test]
fn a_link_in_place_of_the_folder_is_not_used() {
    let real = temp("real");
    let link = temp("link");
    std::fs::create_dir_all(&real).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert!(matches!(
        MicLock::for_dictation_in(&link),
        Err(LockError::Unsafe(_))
    ));
    std::fs::remove_file(&link).unwrap();
    std::fs::remove_dir_all(&real).unwrap();
}
