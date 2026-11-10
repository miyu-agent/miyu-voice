use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::{BLOCK, Echo, Played, power};

/// 像说话的声音：一阵一阵的噪声（不带文件，每次一样）。
fn voice(samples: usize, seed: u32) -> Vec<f32> {
    let mut state = seed;
    let mut low = 0.0f32;
    (0..samples)
        .map(|i| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            #[expect(clippy::cast_precision_loss, reason = "测试里的小数")]
            let (noise, t) = ((state >> 8) as f32 / 8_388_608.0 - 1.0, i as f32 / 16_000.0);
            low += 0.35 * (noise - low);
            let syllable = (std::f32::consts::TAU * 3.0 * t).sin().max(0.0);
            0.5 * low * syllable
        })
        .collect()
}

/// 音箱放 `played`、麦克风收 `heard`，一块块照实际的先后过一遍，交回消过的。
fn run(
    echo: &mut Echo,
    tap: &voice_common::speaker::Tap,
    played: &[f32],
    heard: &[f32],
) -> Vec<f32> {
    let start = Instant::now();
    let mut out = Vec::new();
    for (index, (to_speaker, from_mic)) in played.chunks(BLOCK).zip(heard.chunks(BLOCK)).enumerate()
    {
        tap(to_speaker, 16_000);
        let now = start + Duration::from_millis(10) * u32::try_from(index).unwrap();
        out.extend(echo.clean_at(from_mic, now));
    }
    out
}

/// 麦克风收到的：音箱的声音晚 `delay` 个采样、轻一些地漏回来，加上 `near`（你说的）。
fn room(played: &[f32], near: &[f32], delay: usize) -> Vec<f32> {
    (0..played.len())
        .map(|i| {
            let echo = if i >= delay {
                0.6 * played[i - delay]
            } else {
                0.0
            };
            let more = if i >= delay + 400 {
                0.2 * played[i - delay - 400]
            } else {
                0.0
            };
            echo + more + near.get(i).copied().unwrap_or(0.0)
        })
        .collect()
}

#[test]
fn her_own_voice_is_removed_and_flagged() {
    let played = voice(16_000 * 4, 1);
    let heard = room(&played, &[], 1_440);
    let flag = Arc::new(AtomicBool::new(false));
    let source = Played::default();
    let mut echo = Echo::new(source.clone(), Arc::clone(&flag));
    let out = run(&mut echo, &source.tap(), &played, &heard);
    // 学了两秒以后：剩下的比原来小 20 分贝（100 倍）以上，算「主要是她自己」。
    let (raw, cleaned) = (power(&heard[32_000..]), power(&out[32_000..]));
    assert!(cleaned * 100.0 < raw, "raw {raw} cleaned {cleaned}");
    assert!(flag.load(Ordering::SeqCst));
}

#[test]
fn your_voice_over_hers_survives_and_is_not_flagged() {
    let played = voice(16_000 * 5, 1);
    // 你从第 3 秒说到最后，和漏回来的她差不多响。
    let mut near = vec![0.0; 16_000 * 3];
    near.extend(voice(16_000 * 2, 7).iter().map(|sample| sample * 0.8));
    let heard = room(&played, &near, 1_440);
    let flag = Arc::new(AtomicBool::new(false));
    let source = Played::default();
    let mut echo = Echo::new(source.clone(), Arc::clone(&flag));
    let out = run(&mut echo, &source.tap(), &played, &heard);
    let (yours, cleaned) = (
        power(&near[56_000..]),
        power(&out[56_000..out.len().min(80_000)]),
    );
    // 你的声音留下一半以上的能量（少不到 3 分贝），不算「主要是她自己」。
    assert!(cleaned * 2.0 > yours, "yours {yours} cleaned {cleaned}");
    assert!(!flag.load(Ordering::SeqCst));
}

#[test]
fn quiet_speaker_passes_the_microphone_through() {
    let flag = Arc::new(AtomicBool::new(true));
    let mut echo = Echo::new(Played::default(), Arc::clone(&flag));
    let heard = voice(1_000, 3);
    let now = Instant::now();
    let mut out = echo.clean_at(&heard[..500], now);
    out.extend(echo.clean_at(&heard[500..], now));
    assert_eq!(out, heard);
    assert!(!flag.load(Ordering::SeqCst));
}

/// 她说完一秒以后不再过回声消除；中间攒的零头照先后带出来，一个采样不丢、不乱。
#[test]
fn after_she_stops_the_leftover_comes_out_in_order() {
    let source = Played::default();
    let mut echo = Echo::new(source.clone(), Arc::new(AtomicBool::new(false)));
    let tap = source.tap();
    let start = Instant::now();
    tap(&vec![0.0; 160], 16_000);
    // 音箱在放安静：消除器开着，200 个采样只出得来一整块，零头 40 个留着。
    let first = echo.clean_at(&[0.25; 200], start);
    assert_eq!(first.len(), 160);
    let later = echo.clean_at(&[0.5; 100], start + Duration::from_secs(2));
    assert_eq!(later.len(), 140);
    assert!(
        later[..40]
            .iter()
            .all(|sample| (sample - 0.25).abs() < 1e-6)
    );
    assert!(later[40..].iter().all(|sample| (sample - 0.5).abs() < 1e-6));
}
