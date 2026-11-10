//! 从本机的音箱放（设计 20 第三节「形状」`tts.play`、`tts.stop`）：几个一起来照先后排队，一个放完放下一个，不互相打断；
//! `stop` 把正在放的和排着的全停掉。音箱只在有东西要放的时候开着，队列空了等 [`LINGER`] 还没有新的才关，不一直占着声卡；
//! 一句接一句念的（语音助手）不用每句重开一次（施工 VO-15：重开那一下句子之间听得出停顿）。mp3、wav 都认（rodio）。

use std::io::Cursor;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use rodio::{
    ChannelCount, Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, SampleRate, Source,
};

/// 放不了的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayError {
    /// 读不出来的音频（`bad_audio`）。
    BadAudio(String),
    /// 没有音箱、打不开（`no_speaker`）。
    NoSpeaker(String),
}

/// 队列空了以后音箱再开多久：下一句在这之内来的接着放，不重开。
pub const LINGER: Duration = Duration::from_millis(1500);

/// 旁听正在放的声音（施工 VO-20，回声消除要知道音箱在放什么）：声音交给声卡的时候一小块一小块叫它，给的是合成一个声道的
/// 采样和采样率。在放声音的线程上叫，里面只该记下来、马上回。
pub type Tap = Arc<dyn Fn(&[f32], u32) + Send + Sync>;

/// 本机的音箱。复制出去的是同一个（同一个队列）。
#[derive(Default, Clone)]
pub struct Speaker {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    /// 轮到谁放：拿着它的那一个在放。
    turn: Mutex<()>,
    /// 每叫停一次加一：排着的、在放的看到变了就停。
    generation: AtomicU64,
    /// 在放的和排着的有几个。
    pending: AtomicUsize,
    /// 开着的音箱、在放的那一个。
    open: Mutex<Option<Open>>,
    /// 放过几段：等着关音箱的看到变了（又放了新的）就不关。
    uses: AtomicU64,
    /// 旁听的。
    tap: Option<Tap>,
}

struct Open {
    sink: MixerDeviceSink,
    playing: Option<Arc<Player>>,
}

impl Speaker {
    /// 带旁听的音箱：放的每一段都照先后交给 `tap`。
    #[must_use]
    pub fn tapped(tap: Tap) -> Speaker {
        Speaker {
            inner: Arc::new(Inner {
                tap: Some(tap),
                ..Inner::default()
            }),
        }
    }

    /// 放一段，放完交回 `true`，被 [`Speaker::stop`] 停了交回 `false`。
    ///
    /// # Errors
    ///
    /// 音频读不出来；没有音箱。
    pub fn play(&self, audio: Vec<u8>) -> Result<bool, PlayError> {
        let inner = &self.inner;
        let mine = inner.generation.load(Ordering::SeqCst);
        inner.pending.fetch_add(1, Ordering::SeqCst);
        let used = inner.uses.fetch_add(1, Ordering::SeqCst) + 1;
        let finished = inner.play_in_turn(audio, mine);
        if inner.pending.fetch_sub(1, Ordering::SeqCst) == 1 {
            // 最后一个放完了：等一会儿，还没有新的再关音箱。
            let inner = Arc::clone(inner);
            std::thread::spawn(move || {
                std::thread::sleep(LINGER);
                let mut open = inner.open.lock().unwrap_or_else(PoisonError::into_inner);
                if inner.pending.load(Ordering::SeqCst) == 0
                    && inner.uses.load(Ordering::SeqCst) == used
                {
                    *open = None;
                }
            });
        }
        finished
    }

    /// 正在放的、排着的全停；交回停了几个。
    pub fn stop(&self) -> usize {
        let inner = &self.inner;
        let stopped = inner.pending.load(Ordering::SeqCst);
        inner.generation.fetch_add(1, Ordering::SeqCst);
        if let Some(open) = inner
            .open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            && let Some(playing) = &open.playing
        {
            playing.stop();
        }
        stopped
    }
}

impl Inner {
    fn play_in_turn(&self, audio: Vec<u8>, mine: u64) -> Result<bool, PlayError> {
        let decoder = Decoder::try_from(Cursor::new(audio))
            .map_err(|error| PlayError::BadAudio(format!("The audio cannot be read: {error}.")))?;
        let _turn = self.turn.lock().unwrap_or_else(PoisonError::into_inner);
        if self.generation.load(Ordering::SeqCst) != mine {
            return Ok(false);
        }
        let player = {
            let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
            if open.is_none() {
                let mut sink = DeviceSinkBuilder::open_default_sink().map_err(|error| {
                    PlayError::NoSpeaker(format!("The speaker cannot be opened: {error}."))
                })?;
                sink.log_on_drop(false);
                *open = Some(Open {
                    sink,
                    playing: None,
                });
            }
            let Some(open) = open.as_mut() else {
                return Ok(false);
            };
            let player = Arc::new(Player::connect_new(open.sink.mixer()));
            match &self.tap {
                Some(tap) => player.append(Tapped::new(decoder, Arc::clone(tap))),
                None => player.append(decoder),
            }
            open.playing = Some(Arc::clone(&player));
            player
        };
        while !player.empty() {
            if self.generation.load(Ordering::SeqCst) != mine {
                player.stop();
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        Ok(true)
    }
}

/// 一段声音外面套一层：声卡每取走约 10 毫秒，把这一小块（几个声道平均成一个）交给旁听的。
struct Tapped<S> {
    inner: S,
    tap: Tap,
    /// 攒着还没交的（单声道）。
    block: Vec<f32>,
    /// 这一帧（每个声道一个采样）加到哪了。
    sum: f32,
    seen: u16,
}

impl<S: Source> Tapped<S> {
    fn new(inner: S, tap: Tap) -> Tapped<S> {
        Tapped {
            inner,
            tap,
            block: Vec::new(),
            sum: 0.0,
            seen: 0,
        }
    }

    fn flush(&mut self) {
        if !self.block.is_empty() {
            (self.tap)(&self.block, self.inner.sample_rate().get());
            self.block.clear();
        }
    }
}

impl<S: Source> Iterator for Tapped<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let Some(sample) = self.inner.next() else {
            self.flush();
            return None;
        };
        let channels = self.inner.channels().get();
        self.sum += sample;
        self.seen += 1;
        if self.seen >= channels {
            self.block.push(self.sum / f32::from(channels));
            self.sum = 0.0;
            self.seen = 0;
            if self.block.len() >= (self.inner.sample_rate().get() / 100) as usize {
                self.flush();
            }
        }
        Some(sample)
    }
}

impl<S: Source> Source for Tapped<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

#[cfg(test)]
mod tests;
