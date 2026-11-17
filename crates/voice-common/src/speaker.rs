//! 从本机的音箱放（设计 20 第三节「形状」`tts.play`、`tts.stop`）：几个一起来照先后排队，一个放完放下一个，不互相打断；
//! `stop` 把正在放的和排着的全停掉。音箱只在有东西要放的时候开着，队列空了等 [`LINGER`] 还没有新的才关，不一直占着声卡；
//! 一句接一句念的（语音助手）不用每句重开一次（施工 VO-15：重开那一下句子之间听得出停顿）。mp3、wav 都认（rodio）。
//!
//! 坏了的音箱不留（施工 VO-30，项目主人 11-17：「`/voice-input` 一直在听、没有提示音」）：声音服务断开（`PulseAudio
//! disconnected`）以后，开着的那个音箱再也不取声音，原来「等放完」就永远等下去——下沉那声卡在那里、攥着排队的锁，
//! 下一次开始听的上扬那声排在它后面，录音就一直停在「还没开始听」，`stt.stop` 也叫不停。现在声音服务报了错的音箱标成坏的，
//! 下次用时重开；放的时候 [`STALL`] 之内一点都没取走的也当坏了，停下、扔掉、交回 [`PlayError::NoSpeaker`]，不再无限等。

use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

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

/// 放的时候音箱多久一点声音都没取走就当它坏了（施工 VO-30）。声卡从挂起醒过来不到一秒，两秒不会误判正常的音箱。
pub const STALL: Duration = Duration::from_secs(2);

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
    /// 声音服务对这个音箱报过错（断开、设备没了）：不能再用，下次用时重开。
    broken: Arc<AtomicBool>,
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
        let used = inner.take();
        let finished = inner.play_in_turn(audio, mine);
        Inner::give_back(inner, used);
        finished
    }

    /// 现在就开着音箱，直到交回的 [`Awake`] 丢掉（之后照常再开 [`LINGER`]）：开着的音箱一直放静音，声卡不会被声音服务挂起。
    /// 先开好、过一会儿才放的短音（语音输入开始听、停止听的提示音，施工 VO-28）用它：声卡闲了几秒被 PipeWire 挂起以后，
    /// 叫醒它的那一下会把开头吞掉，短音整个吞没了。
    ///
    /// # Errors
    ///
    /// 没有音箱。
    pub fn awake(&self) -> Result<Awake, PlayError> {
        let used = self.inner.take();
        let opened = self.inner.ensure_open();
        let awake = Awake {
            inner: Arc::clone(&self.inner),
            used,
        };
        opened.map(|()| awake)
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

/// [`Speaker::awake`] 交回的：丢掉以前音箱一直开着。
pub struct Awake {
    inner: Arc<Inner>,
    used: u64,
}

impl Drop for Awake {
    fn drop(&mut self) {
        Inner::give_back(&self.inner, self.used);
    }
}

impl Inner {
    /// 要用音箱了：算一个在用的，交回这是第几次用。
    fn take(&self) -> u64 {
        self.pending.fetch_add(1, Ordering::SeqCst);
        self.uses.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// 用完了：最后一个用完的等 [`LINGER`]，还没有新的再关音箱。
    fn give_back(inner: &Arc<Inner>, used: u64) {
        if inner.pending.fetch_sub(1, Ordering::SeqCst) != 1 {
            return;
        }
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

    /// 音箱没开的、开着但坏了的开一个新的。
    fn ensure_open(&self) -> Result<(), PlayError> {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        if open
            .as_ref()
            .is_some_and(|open| open.broken.load(Ordering::SeqCst))
        {
            *open = None;
        }
        if open.is_none() {
            let broken: Arc<AtomicBool> = Arc::default();
            let mut sink = open_sink(&broken).map_err(|error| {
                PlayError::NoSpeaker(format!("The speaker cannot be opened: {error}."))
            })?;
            sink.log_on_drop(false);
            *open = Some(Open {
                sink,
                playing: None,
                broken,
            });
        }
        Ok(())
    }

    /// 这个音箱坏了：还是开着的那一个的话扔掉，下次用时重开。
    fn discard(&self, broken: &Arc<AtomicBool>) {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        if open
            .as_ref()
            .is_some_and(|open| Arc::ptr_eq(&open.broken, broken))
        {
            *open = None;
        }
    }

    fn play_in_turn(&self, audio: Vec<u8>, mine: u64) -> Result<bool, PlayError> {
        let decoder = Decoder::try_from(Cursor::new(audio))
            .map_err(|error| PlayError::BadAudio(format!("The audio cannot be read: {error}.")))?;
        let _turn = self.turn.lock().unwrap_or_else(PoisonError::into_inner);
        if self.generation.load(Ordering::SeqCst) != mine {
            return Ok(false);
        }
        self.ensure_open()?;
        let pulled: Arc<AtomicU64> = Arc::default();
        let (player, broken) = {
            let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(open) = open.as_mut() else {
                return Ok(false);
            };
            let player = Arc::new(Player::connect_new(open.sink.mixer()));
            let counted = Counted::new(decoder, Arc::clone(&pulled));
            match &self.tap {
                Some(tap) => player.append(Tapped::new(counted, Arc::clone(tap))),
                None => player.append(counted),
            }
            open.playing = Some(Arc::clone(&player));
            (player, Arc::clone(&open.broken))
        };
        let mut watch = Watch::new(Instant::now());
        while !player.empty() {
            if self.generation.load(Ordering::SeqCst) != mine {
                player.stop();
                return Ok(false);
            }
            if broken.load(Ordering::SeqCst)
                || watch.stalled(pulled.load(Ordering::SeqCst), Instant::now(), STALL)
            {
                player.stop();
                self.discard(&broken);
                return Err(PlayError::NoSpeaker(
                    "The speaker stopped taking sound (the sound server disconnected or the device went away)."
                        .to_string(),
                ));
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        Ok(true)
    }
}

/// 盯着音箱取走了多少（施工 VO-30）：取走的数一直没变、过了时限的算卡住了。
#[derive(Debug, Clone, Copy)]
struct Watch {
    /// 上一次看到的取走数。
    seen: u64,
    /// 这个数最后一次变的时刻。
    since: Instant,
}

impl Watch {
    fn new(now: Instant) -> Watch {
        Watch {
            seen: 0,
            since: now,
        }
    }

    /// 现在取走了 `pulled` 个：从上次变了起过了 `limit` 还没变的交回 `true`。
    fn stalled(&mut self, pulled: u64, now: Instant, limit: Duration) -> bool {
        if pulled != self.seen {
            self.seen = pulled;
            self.since = now;
            return false;
        }
        now.duration_since(self.since) >= limit
    }
}

/// 开音箱：系统默认的。测试用：设了 `MIYU_VOICE_TEST_SPEAKER`（输出设备的编号，例如 PipeWire 临时建的虚拟输出）的开它，
/// 端到端测试照真的那一路放、录它的监听，不从主人的音箱出声（施工 VO-28）。
///
/// 声音服务报错时在 `broken` 上记一笔（施工 VO-30），标准错误照 rodio 原来的写法印一行。默认设备带着这个开不了的，退回
/// rodio 自己挑别的设备的那一路（那一路没有报错的回调，卡住了靠 [`STALL`] 发现）。
fn open_sink(broken: &Arc<AtomicBool>) -> Result<MixerDeviceSink, String> {
    let broken = Arc::clone(broken);
    let on_error = move |error: rodio::cpal::Error| {
        eprintln!("audio stream error: {error}");
        broken.store(true, Ordering::SeqCst);
    };
    if let Some(wanted) = std::env::var_os("MIYU_VOICE_TEST_SPEAKER") {
        use rodio::cpal::traits::{DeviceTrait, HostTrait};
        let wanted = wanted.to_string_lossy().into_owned();
        let device = rodio::cpal::default_host()
            .output_devices()
            .map_err(|error| error.to_string())?
            .find(|device| device.id().is_ok_and(|id| id.id() == wanted))
            .ok_or_else(|| format!("no output device {wanted}"))?;
        return DeviceSinkBuilder::from_device(device)
            .map(|builder| builder.with_error_callback(on_error))
            .and_then(DeviceSinkBuilder::open_stream)
            .map_err(|error| error.to_string());
    }
    DeviceSinkBuilder::from_default_device()
        .map(|builder| builder.with_error_callback(on_error))
        .and_then(|builder| builder.open_sink_or_fallback())
        .or_else(|_| DeviceSinkBuilder::open_default_sink())
        .map_err(|error| error.to_string())
}

/// 一段声音外面套一层：声卡每取走一个采样，`pulled` 加一（施工 VO-30，[`Watch`] 照它看音箱还在不在取）。
struct Counted<S> {
    inner: S,
    pulled: Arc<AtomicU64>,
}

impl<S> Counted<S> {
    fn new(inner: S, pulled: Arc<AtomicU64>) -> Counted<S> {
        Counted { inner, pulled }
    }
}

impl<S: Source> Iterator for Counted<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        self.pulled.fetch_add(1, Ordering::Relaxed);
        Some(sample)
    }
}

impl<S: Source> Source for Counted<S> {
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
