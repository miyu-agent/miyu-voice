use super::{decode, encode};
use crate::recognize::{Heard, Refused};

#[test]
fn answers_round_trip_between_the_two_processes() {
    let heard = Heard {
        text: "开会".to_string(),
        language: "zh".to_string(),
        duration_ms: 1200,
        clear: true,
    };
    assert_eq!(decode(&encode(Ok(heard.clone()))), Ok(heard));
    for refused in [
        Refused::TooLong,
        Refused::Language("xx".to_string()),
        Refused::Engine("broken".to_string()),
    ] {
        assert_eq!(decode(&encode(Err(refused.clone()))), Err(refused));
    }
}

/// 识别的子进程卡死了（审计 #120）：过了期限回错，收掉它也不卡（关了输入不退的杀掉）。以前没有期限、收的时候只等不杀，
/// 主进程跟着永远卡住。
#[cfg(unix)]
#[test]
fn a_stuck_recognizer_is_given_up_on_and_killed() {
    use std::time::{Duration, Instant};

    use super::Workers;

    let workers = Workers::with_command(
        "sleep".into(),
        vec!["30".to_string()],
        Duration::from_millis(300),
    );
    let started = Instant::now();
    let got = workers.transcribe_wav(&voice_common::audio::write_wav(&[0.0; 160], 16_000), "zh");
    assert!(
        matches!(&got, Err(Refused::Engine(why)) if why.contains("too long")),
        "{got:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "gave up in time: {:?}",
        started.elapsed()
    );
    let dropping = Instant::now();
    drop(workers);
    assert!(
        dropping.elapsed() < Duration::from_secs(5),
        "dropped in time"
    );
}

/// 子进程一拉起就退了：再拉一次，还是退了就回错，不卡。
#[cfg(unix)]
#[test]
fn a_recognizer_that_keeps_exiting_is_an_error() {
    use std::time::Duration;

    use super::Workers;

    let workers = Workers::with_command("true".into(), Vec::new(), Duration::from_secs(5));
    let got = workers.transcribe_wav(&voice_common::audio::write_wav(&[0.0; 160], 16_000), "zh");
    assert!(
        matches!(&got, Err(Refused::Engine(why)) if why.contains("stopped unexpectedly")),
        "{got:?}"
    );
}
