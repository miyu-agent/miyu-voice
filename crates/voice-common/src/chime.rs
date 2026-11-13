//! 提示音（设计 20 第九节）：语音助手叫醒了、开了对话响上扬的一声（第 2 步），对话结束响下沉的一声；语音输入开始听、停止听
//! 同样这两声（施工 VO-25，项目主人 2026-11-13）。自己合成短的正弦波，不带音频文件；从本机音箱放（[`crate::speaker`]）。
//!
//! 测试用：设了 `MIYU_VOICE_TEST_CHIMES`（一个文件）的不放，一声一行（`wake`、`off`）记进那个文件，端到端测试不出声。
//!
//! 前面垫 [`LEAD_MS`] 的静音（施工 VO-18，主人：「没有音效」）：声卡闲着时声音服务（PipeWire、PulseAudio）让它睡着，
//! 叫醒它的那一下开头的声音被吞掉；原来一声只有 90 毫秒，整个吞没了。朗读一句几秒，丢开头一点听不出来。

use std::io::Write;

use crate::audio;
use crate::speaker::Speaker;

/// 采样率。
const RATE: u32 = 24_000;
/// 前面垫多久静音，等声卡醒过来。
pub const LEAD_MS: u32 = 250;

/// 哪一种。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chime {
    /// 叫醒了、开始听：低到高两声。
    Wake,
    /// 对话结束、停止听：高到低两声。
    Off,
}

/// 这种提示音的 wav 字节。
#[must_use]
pub fn wav(chime: Chime) -> Vec<u8> {
    let notes: &[(f32, u32)] = match chime {
        Chime::Wake => &[(880.0, 110), (1320.0, 150)],
        Chime::Off => &[(1320.0, 110), (880.0, 150)],
    };
    let mut samples = vec![0.0; (RATE * LEAD_MS / 1000) as usize];
    for (pitch, ms) in notes {
        samples.extend(tone(*pitch, *ms));
        samples.extend(std::iter::repeat_n(0.0, (RATE / 50) as usize));
    }
    audio::write_wav(&samples, RATE)
}

/// 放一声，另开线程，马上回（放不出来的，例如没有音箱，不管）。
pub fn play(speaker: &Speaker, chime: Chime) {
    if logged(chime) {
        return;
    }
    let speaker = speaker.clone();
    std::thread::spawn(move || {
        let _ = speaker.play(wav(chime));
    });
}

/// 放一声，放完才回（放不出来的不管）：开始听之前用，免得把这一声也录进去。
pub fn play_and_wait(speaker: &Speaker, chime: Chime) {
    if !logged(chime) {
        let _ = speaker.play(wav(chime));
    }
}

/// 设了 `MIYU_VOICE_TEST_CHIMES` 的记一行、不放，交回 `true`。
fn logged(chime: Chime) -> bool {
    let Some(path) = std::env::var_os("MIYU_VOICE_TEST_CHIMES") else {
        return false;
    };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let name = match chime {
            Chime::Wake => "wake",
            Chime::Off => "off",
        };
        let _ = writeln!(file, "{name}");
    }
    true
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
            0.3 * edge * (std::f32::consts::TAU * pitch * t).sin()
        })
        .collect()
}

#[cfg(test)]
mod tests;
