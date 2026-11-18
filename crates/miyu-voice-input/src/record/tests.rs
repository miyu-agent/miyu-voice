use std::time::Duration;

use super::{Stopper, listen_from};
use crate::endpoint::Ended;

/// 开麦、响开始那一声的时候（还没开始听）人就按了停：要停下，不能丢（审计 #120：以前这一下回 stopped: false，
/// 开始听时又把停的记号清掉，要等静下来五秒或到最长才停）。
#[test]
fn a_stop_while_the_microphone_opens_is_not_lost() {
    let stopper = Stopper::default();
    let starting = stopper.begin();
    assert!(stopper.stop(), "a recording is starting: the stop counts");
    // 一直有声音、从不安静：只有叫停能让它停。
    let loud = || Some(vec![0.3_f32; 160]);
    let handle = std::thread::scope(|scope| {
        let listening = scope.spawn(|| listen_from(|| None, loud, 16_000, 1_200, 60_000, &stopper));
        let started = std::time::Instant::now();
        let ended = listening.join().unwrap();
        (ended, started.elapsed())
    });
    drop(starting);
    let (ended, took) = handle;
    let (_, ended) = ended.unwrap();
    assert_eq!(ended, Ended::Stop);
    assert!(
        took < Duration::from_secs(1),
        "stopped at once, took {took:?}"
    );
    assert!(!stopper.stop(), "nothing is recording any more");
}
