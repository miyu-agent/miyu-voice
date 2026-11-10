//! 回声消除（施工 VO-20；项目主人 2026-11-10：她听到自己的声音「应该通过软件解决」）：她说话时麦克风照样开着，把音箱正在放的
//! 声音从麦克风收到的里面减掉，剩下的才交给听的那一套——外放也能开口就打断她，不用戴耳机。
//!
//! 用的是 WebRTC 那一套回声消除（AEC3）的纯 Rust 移植 `sonora`。它要两路声音：音箱在放什么（[`Played`]：助手自己的音箱旁听来的，
//! 提示音、她念的每一句都经过它）、麦克风收到什么。两路都转成 16 kHz、切成 10 毫秒一块，放的一块、收的一块交替着喂；
//! 两边的先后差多少（声卡的缓冲、声音在屋里走的时间）它自己估。
//!
//! 音箱安静了 [`HOLD`] 以后不再过它（省 CPU、不动唤醒词听到的声音），学到的屋子的回声留着，下一句接着用。
//!
//! 消不干净的时候（音箱开得很响、她刚开口那一下）剩下的回声会被当成你开口。所以另算一件事：最近 0.3 秒消掉以后比消掉以前
//! 小了多少，小了 [`MOSTLY_ECHO_DB`] 分贝以上的算「麦克风里主要是她自己」（[`Echo::new`] 给的那个开关），这时听到的开口不算打断。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use sherpa_onnx::LinearResampler;
use sonora::config::EchoCanceller;
use sonora::{AudioProcessing, Config, StreamConfig};
use voice_common::speaker::Tap;

use crate::listen::RATE;

/// 一块多少个采样：16 kHz 下 10 毫秒（回声消除只收这么大的块）。
const BLOCK: usize = RATE / 100;
/// 音箱安静多久以后不再过回声消除：屋里的余响、声卡缓冲里没放完的都过去了。
const HOLD: Duration = Duration::from_secs(1);
/// 放的那一路攒了这么多还没喂（声卡一口气取走一大段）就多喂几块追上，不让它越攒越多。
const BACKLOG: usize = RATE * 3 / 10;
/// 消掉以后比消掉以前小了这么多分贝以上：麦克风里主要是她自己的声音。
const MOSTLY_ECHO_DB: f32 = 15.0;
/// 算上面那件事时每一块占多少（约看最近 0.3 秒）。
const SMOOTH: f32 = 0.1;
/// 比这还轻的当安静，不算谁大谁小。
const QUIET: f32 = 1e-7;

/// 音箱放过、还没交给回声消除的声音。复制出去的是同一个。
#[derive(Clone, Default)]
pub struct Played {
    queue: Arc<Mutex<Queue>>,
}

#[derive(Default)]
struct Queue {
    rate: u32,
    samples: Vec<f32>,
}

impl Played {
    /// 交给音箱的旁听（[`voice_common::speaker::Speaker::tapped`]）：记下放了什么。采样率半路变了的，前面没取走的丢掉。
    #[must_use]
    pub fn tap(&self) -> Tap {
        let queue = Arc::clone(&self.queue);
        Arc::new(move |block, rate| {
            let mut queue = queue.lock().unwrap_or_else(PoisonError::into_inner);
            if queue.rate != rate {
                queue.samples.clear();
                queue.rate = rate;
            }
            queue.samples.extend_from_slice(block);
        })
    }

    fn take(&self) -> (u32, Vec<f32>) {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        (queue.rate, std::mem::take(&mut queue.samples))
    }
}

/// 收麦的线程拿着的那一套。
pub struct Echo {
    played: Played,
    drowned: Arc<AtomicBool>,
    canceller: Option<AudioProcessing>,
    /// 放的那一路转成 16 kHz 用的，和它是照哪个采样率建的。
    resampler: Option<(u32, LinearResampler)>,
    /// 放的那一路，16 kHz，还没喂的。
    render: VecDeque<f32>,
    /// 收的那一路不满一块的零头。
    capture: Vec<f32>,
    /// 音箱最后一次放出声音是什么时候。
    last_played: Option<Instant>,
    /// 最近消掉以前、以后的音量（平方的平均）。
    before: f32,
    after: f32,
}

impl Echo {
    /// `played` 同交给音箱旁听的是一个；`mostly_echo` 由这里写：麦克风里现在主要是她自己的声音。
    #[must_use]
    pub fn new(played: Played, mostly_echo: Arc<AtomicBool>) -> Echo {
        Echo {
            played,
            drowned: mostly_echo,
            canceller: None,
            resampler: None,
            render: VecDeque::new(),
            capture: Vec::new(),
            last_played: None,
            before: 0.0,
            after: 0.0,
        }
    }

    /// 麦克风收到的一段（16 kHz）进，消掉回声的出；音箱安静时原样出。出来的可能比进去的少不到一块，零头下一次带出来。
    pub fn clean(&mut self, heard: &[f32]) -> Vec<f32> {
        self.clean_at(heard, Instant::now())
    }

    fn clean_at(&mut self, heard: &[f32], now: Instant) -> Vec<f32> {
        self.take_played(now);
        let active = self
            .last_played
            .is_some_and(|last| now.duration_since(last) < HOLD);
        if !active {
            self.render.clear();
            self.drowned.store(false, Ordering::SeqCst);
            (self.before, self.after) = (0.0, 0.0);
            let mut out = std::mem::take(&mut self.capture);
            out.extend_from_slice(heard);
            return out;
        }
        self.capture.extend_from_slice(heard);
        let whole = self.capture.len() / BLOCK * BLOCK;
        let mut out = vec![0.0; whole];
        let canceller = self.canceller.get_or_insert_with(build);
        let mut played = [0.0f32; BLOCK];
        let mut unused = [0.0f32; BLOCK];
        for (from, to) in self.capture[..whole]
            .chunks(BLOCK)
            .zip(out.chunks_mut(BLOCK))
        {
            // 放的一块、收的一块交替；放的那一路攒多了就多喂几块，没有的喂安静。
            loop {
                for sample in &mut played {
                    *sample = self.render.pop_front().unwrap_or(0.0);
                }
                let _ = canceller.process_render_f32(&[&played], &mut [&mut unused]);
                if self.render.len() <= BACKLOG {
                    break;
                }
            }
            if canceller
                .process_capture_f32(&[from], &mut [&mut *to])
                .is_err()
            {
                to.copy_from_slice(from);
            }
            self.before += SMOOTH * (power(from) - self.before);
            self.after += SMOOTH * (power(to) - self.after);
        }
        self.capture.drain(..whole);
        let ratio = 10f32.powf(MOSTLY_ECHO_DB / 10.0);
        self.drowned.store(
            self.before > QUIET && self.after * ratio < self.before,
            Ordering::SeqCst,
        );
        out
    }

    /// 音箱新放的取过来，转成 16 kHz 排进放的那一路。
    fn take_played(&mut self, now: Instant) {
        let (rate, samples) = self.played.take();
        if samples.is_empty() {
            return;
        }
        self.last_played = Some(now);
        if rate as usize == RATE {
            self.render.extend(samples);
            return;
        }
        if self
            .resampler
            .as_ref()
            .is_none_or(|(built, _)| *built != rate)
        {
            self.resampler = i32::try_from(rate)
                .ok()
                .and_then(|from| LinearResampler::create(from, 16_000))
                .map(|resampler| (rate, resampler));
        }
        if let Some((_, resampler)) = &self.resampler {
            self.render.extend(resampler.resample(&samples, false));
        }
    }
}

fn build() -> AudioProcessing {
    let stream = StreamConfig::new(16_000, 1);
    AudioProcessing::builder()
        .config(Config {
            echo_canceller: Some(EchoCanceller::default()),
            ..Default::default()
        })
        .capture_config(stream)
        .render_config(stream)
        .build()
}

/// 一块的音量：平方的平均。
fn power(block: &[f32]) -> f32 {
    #[expect(clippy::cast_precision_loss, reason = "一块 160 个采样")]
    let n = block.len().max(1) as f32;
    block.iter().map(|sample| sample * sample).sum::<f32>() / n
}

#[cfg(test)]
mod tests;
