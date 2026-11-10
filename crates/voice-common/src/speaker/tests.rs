use std::num::NonZero;
use std::sync::{Arc, Mutex};

use rodio::buffer::SamplesBuffer;

use super::{Tap, Tapped};

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
