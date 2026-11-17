use std::num::NonZero;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rodio::buffer::SamplesBuffer;

use super::{Counted, STALL, Tap, Tapped, Watch};

/// 音箱一直在取的不算卡住；取走的数停在那里过了时限才算（施工 VO-30：断开的音箱再也不取，原来就一直等下去）。
#[test]
fn a_speaker_that_stops_taking_sound_counts_as_stalled_after_the_limit() {
    let start = Instant::now();
    let at = |ms: u64| start + Duration::from_millis(ms);
    let mut watch = Watch::new(start);
    // 开头声卡还在醒：没取走，没到时限，不算。
    assert!(!watch.stalled(0, at(1_900), STALL));
    // 取了：时限从这时重新算。
    assert!(!watch.stalled(480, at(1_950), STALL));
    assert!(!watch.stalled(960, at(3_000), STALL));
    assert!(!watch.stalled(960, at(4_999), STALL));
    // 停在 960 整整两秒：卡住了。
    assert!(watch.stalled(960, at(5_000), STALL));
    // 一开始就一点没取（开了个已经断开的音箱）：两秒到了也算。
    let mut dead = Watch::new(start);
    assert!(dead.stalled(0, at(2_000), STALL));
}

/// 套上计数的：声卡拿到的采样不变，取走一个数一个。
#[test]
fn counted_passes_samples_through_and_counts_them() {
    let pulled: Arc<AtomicU64> = Arc::default();
    let samples: Vec<f32> = (0..30u8).map(|i| f32::from(i) / 30.0).collect();
    let source = SamplesBuffer::new(
        NonZero::new(2).unwrap(),
        NonZero::new(1000).unwrap(),
        samples.clone(),
    );
    let mut counted = Counted::new(source, Arc::clone(&pulled));
    let first: Vec<f32> = counted.by_ref().take(10).collect();
    assert_eq!(first, samples[..10]);
    assert_eq!(pulled.load(Ordering::SeqCst), 10);
    let rest: Vec<f32> = counted.collect();
    assert_eq!(rest, samples[10..]);
    assert_eq!(pulled.load(Ordering::SeqCst), 30);
}

/// 两个声道的一段套上旁听：声卡照样拿到原来的采样，旁听的拿到平均成一个声道的、一块约 10 毫秒，最后不满一块的也交。
#[test]
fn tap_hears_mono_blocks_and_passes_samples_through() {
    let heard: Arc<Mutex<Vec<(usize, u32)>>> = Arc::default();
    let all: Arc<Mutex<Vec<f32>>> = Arc::default();
    let (blocks, samples) = (Arc::clone(&heard), Arc::clone(&all));
    let tap: Tap = Arc::new(move |block, rate| {
        blocks.lock().unwrap().push((block.len(), rate));
        samples.lock().unwrap().extend_from_slice(block);
    });
    // 1000 Hz、两个声道、25 帧：左 0.5、右 -0.1，平均 0.2。
    let stereo: Vec<f32> = (0..25).flat_map(|_| [0.5, -0.1]).collect();
    let source = SamplesBuffer::new(
        NonZero::new(2).unwrap(),
        NonZero::new(1000).unwrap(),
        stereo.clone(),
    );
    let passed: Vec<f32> = Tapped::new(source, tap).collect();
    assert_eq!(passed, stereo);
    assert_eq!(*heard.lock().unwrap(), [(10, 1000), (10, 1000), (5, 1000)]);
    assert!(all.lock().unwrap().iter().all(|s| (s - 0.2).abs() < 1e-6));
}
