//! 听（设计 20 第九节「规程」第 1～3 步）：能量门 → 人声检测切句 → 唤醒词。输入是 16 kHz 单声道的帧，来源不管（麦克风、
//! 测试里的 wav），和录音完全分开。照旧版 0.7.4 的取舍重写：
//!
//! - 唤醒词不逐帧流式认，等人声检测切出一整段再整段过一遍：「密友密友帮我查一下」反正要等说完才认得成字，早半句知道
//!   命中没有用；
//! - 静的时候人声检测也不跑：帧的能量低于自己跟着的底噪就跳过（能量门），只在人声检测还在句中、刚结束时接着喂；
//! - 段前补回 0.5 秒（第 2 步：旧版加了这一条，「密友密友」从 0/5 到 5/5）；
//! - 唤醒词后面紧跟着说的也算这一句：交回唤醒词说完以后的那一截；只说了唤醒词的，等下一段。

use std::collections::VecDeque;

use sherpa_onnx::{
    KeywordSpotter, KeywordSpotterConfig, OnlineTransducerModelConfig, SileroVadModelConfig,
    VadModelConfig, VoiceActivityDetector,
};

use crate::models::Models;

/// 采样率。
pub const RATE: usize = 16_000;
/// 段前补回多少（采样）：0.5 秒。
const PRE_ROLL: usize = RATE / 2;
/// 原始声音留多久（采样）：人声检测一段最长 20 秒，再加补回的。
const RING: usize = 23 * RATE;
/// 唤醒词说完以后再留多少（采样）：0.3 秒，免得把后面那句的头吃掉。
const KEYWORD_TAIL: usize = RATE * 3 / 10;
/// 唤醒词后面那一截短于它的不算一句话（采样）：0.5 秒。
const SHORTEST: usize = RATE / 2;
/// 能量门关了以后再放多久（采样）：1.5 秒，比人声检测认句尾的 0.6 秒长，句尾一定喂得到。
const GATE_TRAIL: usize = RATE * 3 / 2;
/// 人声连着多久算开口（采样）：0.3 秒（设计 20 第九节第 6 步）。
const ONSET: usize = RATE * 3 / 10;

/// 听到了什么。
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// 听到唤醒词。`rest` 是唤醒词后面紧跟着说的（短于 0.5 秒的不算，是空的）。
    Wake {
        /// 中了哪个（`@` 后面那段）。
        keyword: String,
        /// 后面紧跟着说的。
        rest: Vec<f32>,
    },
    /// 窗口开着的时候说的一句（追问、唤醒以后的那句）。
    Speech(Vec<f32>),
    /// 有人开口了：人声连着 0.3 秒（第 6 步「开口就打断」），一句话只报一次，这句说完才再报。这一句照样还会切出来交回。
    Onset,
}

/// 灵敏度（配置页「灵敏度」）。
#[derive(Debug, Clone, Copy)]
pub struct Sensitivity {
    /// 唤醒词判中的门槛：越小越容易中，0.01～1，旧版默认 0.25。
    pub threshold: f32,
    /// 唤醒词路径的加分：越大越容易中，0～10，默认 1。
    pub boost: f32,
}

impl Default for Sensitivity {
    fn default() -> Sensitivity {
        Sensitivity {
            threshold: 0.25,
            boost: 1.0,
        }
    }
}

/// 能量门：自己跟着底噪，安静的帧不进人声检测。
struct Gate {
    floor: f32,
    trail: usize,
}

impl Gate {
    /// 低于它一律算安静（均方根，约 -54 dBFS）。
    const QUIET: f32 = 0.002;
    /// 比底噪响几倍算可能有声。
    const RATIO: f32 = 2.5;

    /// 这一帧要不要进人声检测。
    fn admit(&mut self, frame: &[f32], busy: bool) -> bool {
        let loudness = rms(frame);
        if loudness > (self.floor * Gate::RATIO).max(Gate::QUIET) {
            // 有声时底噪只许慢慢往上走，免得一直说话把底抬高。
            self.floor += (loudness - self.floor) * 0.002;
            self.trail = 0;
            return true;
        }
        self.floor =
            (self.floor + (loudness - self.floor) * 0.05).clamp(Gate::QUIET / Gate::RATIO, 0.2);
        self.trail = self.trail.saturating_add(frame.len());
        busy || self.trail < GATE_TRAIL
    }
}

/// 听着的那一套。
pub struct Listener {
    vad: VoiceActivityDetector,
    kws: KeywordSpotter,
    gate: Gate,
    /// 原始的声音（含能量门跳过的），补回段前用；`ring_start` 是第一个采样在原始流里的位置。
    ring: VecDeque<f32>,
    ring_start: usize,
    raw_total: usize,
    admitted_total: usize,
    /// 进人声检测的流和原始流的对应：每次能量门重新开时记一条（进检测的位置，原始的位置）。
    runs: VecDeque<(usize, usize)>,
    was_admitted: bool,
    /// 窗口开着：说什么都算一句，不用唤醒词。
    window: bool,
    /// 这一句人声连着多少采样了；报过开口的记 `onset_told`。
    voiced: usize,
    onset_told: bool,
}

impl Listener {
    /// 照 `models` 建，`keywords` 是 [`crate::keywords::encode`] 编好的几行。
    ///
    /// # Errors
    ///
    /// 模型加载不起来；没有一行唤醒词。
    pub fn new(
        models: &Models,
        keywords: &[String],
        sensitivity: Sensitivity,
    ) -> Result<Listener, String> {
        if keywords.is_empty() {
            return Err("There is no wake word.".to_string());
        }
        let vad_config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(models.vad().display().to_string()),
                threshold: 0.5,
                min_silence_duration: 0.6,
                min_speech_duration: 0.25,
                window_size: 512,
                max_speech_duration: 20.0,
            },
            sample_rate: 16_000,
            num_threads: 1,
            ..VadModelConfig::default()
        };
        let vad = VoiceActivityDetector::create(&vad_config, 60.0)
            .ok_or("The voice activity model could not be loaded.")?;
        let [encoder, decoder, joiner, tokens] = models.kws();
        let mut kws_config = KeywordSpotterConfig::default();
        kws_config.model_config.transducer = OnlineTransducerModelConfig {
            encoder: Some(encoder.display().to_string()),
            decoder: Some(decoder.display().to_string()),
            joiner: Some(joiner.display().to_string()),
        };
        kws_config.model_config.tokens = Some(tokens.display().to_string());
        kws_config.model_config.num_threads = 1;
        kws_config.keywords_threshold = sensitivity.threshold.clamp(0.01, 1.0);
        kws_config.keywords_score = sensitivity.boost.clamp(0.0, 10.0);
        kws_config.keywords_buf = Some(keywords.join("\n"));
        let kws = KeywordSpotter::create(&kws_config)
            .ok_or("The wake word model could not be loaded.")?;
        Ok(Listener {
            vad,
            kws,
            gate: Gate {
                floor: 0.01,
                trail: usize::MAX,
            },
            ring: VecDeque::with_capacity(RING),
            ring_start: 0,
            raw_total: 0,
            admitted_total: 0,
            runs: VecDeque::new(),
            was_admitted: false,
            window: false,
            voiced: 0,
            onset_told: false,
        })
    }

    /// 开、关窗口（追问窗口、等唤醒以后的那句；第 5 步）。
    pub fn set_window(&mut self, open: bool) {
        self.window = open;
    }

    /// 喂一帧，交回这一帧以后听到的。
    pub fn feed(&mut self, frame: &[f32]) -> Vec<Event> {
        self.remember(frame);
        let busy = self.vad.detected();
        let admitted = self.gate.admit(frame, busy);
        if admitted && !self.was_admitted {
            self.runs
                .push_back((self.admitted_total, self.raw_total - frame.len()));
            while self.runs.len() > 64 {
                self.runs.pop_front();
            }
        }
        self.was_admitted = admitted;
        if admitted {
            self.vad.accept_waveform(frame);
            self.admitted_total += frame.len();
        }
        let mut events = Vec::new();
        if admitted && self.vad.detected() {
            self.voiced += frame.len();
            if self.voiced >= ONSET && !self.onset_told {
                self.onset_told = true;
                events.push(Event::Onset);
            }
        } else {
            self.voiced = 0;
            self.onset_told = false;
        }
        while let Some(segment) = self.vad.front() {
            let start = usize::try_from(segment.start()).unwrap_or(0);
            let audio = self.with_pre_roll(start, segment.samples());
            self.vad.pop();
            if let Some(event) = self.judge(audio) {
                events.push(event);
            }
        }
        events
    }

    /// 一段人声：窗口开着的直接算一句；关着的看有没有唤醒词。
    fn judge(&self, audio: Vec<f32>) -> Option<Event> {
        if self.window {
            return Some(Event::Speech(audio));
        }
        let (keyword, end) = self.spot(&audio)?;
        let rest = audio.get(end..).map(<[f32]>::to_vec).unwrap_or_default();
        Some(Event::Wake {
            keyword,
            rest: if rest.len() >= SHORTEST {
                rest
            } else {
                Vec::new()
            },
        })
    }

    /// 整段过一遍唤醒词模型：中了交回哪个、在这段里说完的位置（拿不到时间的当整段都是唤醒词）。
    fn spot(&self, audio: &[f32]) -> Option<(String, usize)> {
        let stream = self.kws.create_stream();
        stream.accept_waveform(16_000, audio);
        // 流式模型要尾巴上的静音把最后几帧推出解码窗口。
        stream.accept_waveform(16_000, &vec![0.0; RATE / 2]);
        stream.input_finished();
        while self.kws.is_ready(&stream) {
            self.kws.decode(&stream);
            if let Some(result) = self.kws.get_result(&stream)
                && !result.keyword.is_empty()
            {
                let last = result
                    .timestamps
                    .iter()
                    .copied()
                    .fold(f32::NEG_INFINITY, f32::max);
                let end = if last.is_finite() && last >= 0.0 {
                    seconds_to_samples(last) + KEYWORD_TAIL
                } else {
                    audio.len()
                };
                return Some((result.keyword, end));
            }
        }
        None
    }

    /// 记进原始声音的环。
    fn remember(&mut self, frame: &[f32]) {
        self.ring.extend(frame);
        self.raw_total += frame.len();
        while self.ring.len() > RING {
            self.ring.pop_front();
            self.ring_start += 1;
        }
    }

    /// 段前补回 0.5 秒：照进检测的位置找回原始流的位置，从环里取。
    fn with_pre_roll(&self, start: usize, samples: &[f32]) -> Vec<f32> {
        let raw = self
            .runs
            .iter()
            .rev()
            .find(|(admitted, _)| *admitted <= start)
            .map(|(admitted, raw)| raw + (start - admitted));
        let Some(raw) = raw else {
            return samples.to_vec();
        };
        let from = raw.saturating_sub(PRE_ROLL).max(self.ring_start);
        let mut audio: Vec<f32> = self
            .ring
            .range(from - self.ring_start..raw.saturating_sub(self.ring_start).min(self.ring.len()))
            .copied()
            .collect();
        audio.extend_from_slice(samples);
        audio
    }
}

fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    #[expect(clippy::cast_precision_loss, reason = "一帧几百个样本")]
    let n = frame.len() as f32;
    (frame.iter().map(|s| s * s).sum::<f32>() / n).sqrt()
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "一段人声最长 20 秒，非负"
)]
fn seconds_to_samples(seconds: f32) -> usize {
    (seconds * 16_000.0) as usize
}

#[cfg(test)]
mod tests;
