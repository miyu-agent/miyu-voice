//! 录音什么时候停（设计 20 第三节「形状」`stt.record`）：人声停了 `silence_ms` 就停，最长 `max_ms`。只看音量：
//! 开头 0.3 秒量出底噪，比底噪响三倍（至少 0.01）的算人声；一直没开口的，5 秒后照「静下来」停，交回的认出来是空的。
//! 人声检测的模型归助手（设计 20 第五节），听写按一下就说，看音量够了。

/// 一帧多长（毫秒）。
pub const FRAME_MS: u64 = 30;
/// 开头量底噪的时长（毫秒）。
const NOISE_MS: u64 = 300;
/// 一直没开口，多久停（毫秒）。
const NO_SPEECH_MS: u64 = 5000;
/// 人声比底噪响几倍。
const SPEECH_RATIO: f32 = 3.0;
/// 人声至少多响（均方根）：底噪量成接近零的安静房间里也不会把呼吸算进去。
const SPEECH_FLOOR: f32 = 0.01;

/// 为什么停（`stt.record` 回的 `ended`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// 静下来了。
    Silence,
    /// 调了 `stt.stop`。
    Stop,
    /// 到了最长。
    Max,
}

impl Ended {
    /// 协议上的写法。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Ended::Silence => "silence",
            Ended::Stop => "stop",
            Ended::Max => "max",
        }
    }
}

/// 一帧帧喂进去，说该不该停。
#[derive(Debug)]
pub struct Endpoint {
    silence_ms: u64,
    max_ms: u64,
    elapsed_ms: u64,
    noise: Vec<f32>,
    floor: f32,
    spoke: bool,
    quiet_ms: u64,
}

impl Endpoint {
    /// 人声停了 `silence_ms` 就停，最长 `max_ms`。
    #[must_use]
    pub fn new(silence_ms: u64, max_ms: u64) -> Endpoint {
        Endpoint {
            silence_ms,
            max_ms,
            elapsed_ms: 0,
            noise: Vec::new(),
            floor: 0.0,
            spoke: false,
            quiet_ms: 0,
        }
    }

    /// 喂一帧（[`FRAME_MS`] 长的单声道样本），该停了交回为什么。
    pub fn feed(&mut self, frame: &[f32]) -> Option<Ended> {
        self.elapsed_ms += FRAME_MS;
        let loudness = rms(frame);
        if self.elapsed_ms <= NOISE_MS {
            self.noise.push(loudness);
            if self.elapsed_ms + FRAME_MS > NOISE_MS {
                self.floor = median(&mut self.noise);
            }
        } else if loudness >= (self.floor * SPEECH_RATIO).max(SPEECH_FLOOR) {
            self.spoke = true;
            self.quiet_ms = 0;
        } else {
            self.quiet_ms += FRAME_MS;
        }
        if self.elapsed_ms >= self.max_ms {
            return Some(Ended::Max);
        }
        let quiet_enough = if self.spoke {
            self.silence_ms
        } else {
            NO_SPEECH_MS
        };
        (self.elapsed_ms > NOISE_MS && self.quiet_ms >= quiet_enough).then_some(Ended::Silence)
    }

    /// 开过口没有。
    #[cfg(test)]
    #[must_use]
    pub fn spoke(&self) -> bool {
        self.spoke
    }
}

fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum: f32 = frame.iter().map(|s| s * s).sum();
    #[expect(clippy::cast_precision_loss, reason = "一帧几千个样本")]
    let n = frame.len() as f32;
    (sum / n).sqrt()
}

fn median(values: &mut [f32]) -> f32 {
    values.sort_by(f32::total_cmp);
    values.get(values.len() / 2).copied().unwrap_or(0.0)
}

#[cfg(test)]
mod tests;
