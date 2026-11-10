use std::sync::atomic::Ordering;

use super::Switches;

/// 一直收麦（施工 VO-14：开关管的是对话，不管听不听唤醒词）；半双工她说话时不收，全双工照收；开着对话不改收不收。
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
    switches.held.store(true, Ordering::SeqCst);
    switches.full_duplex.store(false, Ordering::SeqCst);
    assert!(
        !switches.wanted(),
        "an open conversation still waits until she stops"
    );
}
