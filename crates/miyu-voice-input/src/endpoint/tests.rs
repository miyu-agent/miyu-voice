use super::{Ended, Endpoint, FRAME_MS};

fn frame(level: f32) -> Vec<f32> {
    // 480 个样本 = 16 kHz 下 30 毫秒；正负交替，均方根就是 level。
    (0..480)
        .map(|n| if n % 2 == 0 { level } else { -level })
        .collect()
}

/// 喂 `ms` 毫秒的同一个音量，交回第一次说停的时刻和原因。
fn run(endpoint: &mut Endpoint, level: f32, ms: u64) -> Option<Ended> {
    let mut fed = 0;
    while fed < ms {
        fed += FRAME_MS;
        if let Some(ended) = endpoint.feed(&frame(level)) {
            return Some(ended);
        }
    }
    None
}

#[test]
fn stops_after_speech_then_silence() {
    let mut endpoint = Endpoint::new(1200, 60_000);
    assert_eq!(run(&mut endpoint, 0.001, 300), None, "noise floor");
    assert_eq!(run(&mut endpoint, 0.2, 2000), None, "speaking");
    assert!(endpoint.spoke());
    assert_eq!(run(&mut endpoint, 0.001, 1100), None, "short pause");
    assert_eq!(run(&mut endpoint, 0.001, 200), Some(Ended::Silence));
}

#[test]
fn a_pause_shorter_than_silence_ms_keeps_recording() {
    let mut endpoint = Endpoint::new(1200, 60_000);
    run(&mut endpoint, 0.001, 300);
    run(&mut endpoint, 0.2, 600);
    assert_eq!(run(&mut endpoint, 0.001, 900), None);
    assert_eq!(run(&mut endpoint, 0.2, 600), None, "spoke again");
    assert_eq!(
        run(&mut endpoint, 0.001, 1100),
        None,
        "the pause counter restarted"
    );
}

#[test]
fn never_speaking_stops_after_five_seconds() {
    let mut endpoint = Endpoint::new(1200, 60_000);
    assert_eq!(run(&mut endpoint, 0.002, 5200), None);
    assert_eq!(run(&mut endpoint, 0.002, 300), Some(Ended::Silence));
    assert!(!endpoint.spoke());
}

#[test]
fn talking_on_hits_the_maximum() {
    let mut endpoint = Endpoint::new(1200, 3000);
    run(&mut endpoint, 0.001, 300);
    assert_eq!(run(&mut endpoint, 0.3, 3000), Some(Ended::Max));
}

#[test]
fn a_loud_room_needs_three_times_its_noise() {
    let mut endpoint = Endpoint::new(1200, 60_000);
    run(&mut endpoint, 0.05, 300);
    assert_eq!(run(&mut endpoint, 0.1, 600), None);
    assert!(!endpoint.spoke(), "twice the noise is not speech");
    run(&mut endpoint, 0.2, 60);
    assert!(endpoint.spoke());
}
