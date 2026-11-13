use std::sync::atomic::Ordering;

use super::Switches;

/// 一直收麦（施工 VO-14：开关管的是对话，不管听不听唤醒词）；关了「说话时可以打断她」的她说话时不收，开着的照收；开着对话不改收不收。
#[test]
fn the_microphone_is_wanted_unless_she_speaks_without_headphones() {
    let switches = Switches::default();
    assert!(switches.wanted(), "running means listening");
    switches.speaking.store(true, Ordering::SeqCst);
    assert!(!switches.wanted(), "half duplex: not while she speaks");
    switches.full_duplex.store(true, Ordering::SeqCst);
    assert!(
        switches.wanted(),
        "full duplex: keep listening to hear a cut-in"
    );
    switches.wake.store(true, Ordering::SeqCst);
    switches.full_duplex.store(false, Ordering::SeqCst);
    assert!(
        !switches.wanted(),
        "a conversation started by miyu listen still waits until she stops"
    );
}

/// 连着 30 秒数字静音说一次；中间有一点声音就重新数，静下来再满 30 秒再说一次（施工 VO-26）。
#[test]
fn thirty_seconds_of_digital_silence_is_said_once() {
    use super::{FRAME, SILENT_FOR, Silence};
    let block = vec![0.0f32; FRAME];
    let mut silence = Silence::default();
    let blocks = SILENT_FOR / FRAME;
    let said: usize = (0..blocks * 3).filter(|_| silence.heard(&block)).count();
    assert_eq!(said, 1, "once for a long silent stretch");
    assert!(!silence.heard(&[0.0, 0.2, 0.0]), "a sound resets it");
    let again = (0..=blocks).filter(|_| silence.heard(&block)).count();
    assert_eq!(again, 1, "said again after another 30 s");
    let mut room = Silence::default();
    let noise: Vec<f32> = (0..FRAME)
        .map(|i| if i % 2 == 0 { 0.001 } else { -0.001 })
        .collect();
    assert_eq!(
        (0..blocks * 2).filter(|_| room.heard(&noise)).count(),
        0,
        "a quiet room is not silent"
    );
}
