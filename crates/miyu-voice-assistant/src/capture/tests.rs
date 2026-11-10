use std::sync::atomic::Ordering;

use super::Switches;

/// 一直收麦（施工 VO-14：没有开关了）；半双工她说话时不收，全双工照收；敲了 `miyu listen` 不改收不收。
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
    assert!(!switches.wanted(), "a wake request waits until she stops");
}
