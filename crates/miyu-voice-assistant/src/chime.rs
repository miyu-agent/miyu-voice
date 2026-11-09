//! 提示音（设计 20 第九节）：听到唤醒词响一声（第 2 步），`miyu listen` 开、关各一种声音。自己合成几段短的正弦波，不带
//! 音频文件；从本机音箱放（[`voice_common::speaker`]）。

use voice_common::audio;
use voice_common::speaker::Speaker;

/// 采样率。
const RATE: u32 = 24_000;

/// 哪一种。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chime {
    /// 听到唤醒词：一声短的高音。
    Wake,
    /// 开了：低到高两声。
    On,
    /// 关了：高到低两声。
    Off,
}

/// 这种提示音的 wav 字节。
#[must_use]
pub fn wav(chime: Chime) -> Vec<u8> {
    let notes: &[(f32, u32)] = match chime {
        Chime::Wake => &[(1320.0, 90)],
        Chime::On => &[(660.0, 80), (990.0, 110)],
        Chime::Off => &[(990.0, 80), (660.0, 110)],
    };
    let mut samples = Vec::new();
    for (pitch, ms) in notes {
        samples.extend(tone(*pitch, *ms));
        samples.extend(std::iter::repeat_n(0.0, (RATE / 50) as usize));
    }
    audio::write_wav(&samples, RATE)
}

/// 放一声（放不出来的，例如没有音箱，不管）。
pub fn play(speaker: &Speaker, chime: Chime) {
    let _ = speaker.play(wav(chime));
}

/// 一个音：正弦波，头尾各淡入淡出 10 毫秒，不爆音。
fn tone(pitch: f32, ms: u32) -> Vec<f32> {
    let n = RATE * ms / 1000;
    let fade = RATE / 100;
    (0..n)
        .map(|i| {
            #[expect(clippy::cast_precision_loss, reason = "一声最多几千个采样")]
            let (t, edge) = (
                i as f32 / RATE as f32,
                i.min(n - 1 - i).min(fade) as f32 / fade as f32,
            );
            0.25 * edge * (std::f32::consts::TAU * pitch * t).sin()
        })
        .collect()
}

#[cfg(test)]
mod tests;
