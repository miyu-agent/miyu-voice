//! 从本机的音箱放（设计 20 第三节「形状」`tts.play`、`tts.stop`）：几个一起来照先后排队，一个放完放下一个，不互相打断；
//! `stop` 把正在放的和排着的全停掉。音箱只在有东西要放的时候开着，队列空了就关，不占着声卡。mp3、wav 都认（rodio）。

use std::io::Cursor;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};

/// 放不了的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayError {
    /// 读不出来的音频（`bad_audio`）。
    BadAudio(String),
    /// 没有音箱、打不开（`no_speaker`）。
    NoSpeaker(String),
}

/// 本机的音箱。
#[derive(Default)]
pub struct Speaker {
    /// 轮到谁放：拿着它的那一个在放。
    turn: Mutex<()>,
    /// 每叫停一次加一：排着的、在放的看到变了就停。
    generation: AtomicU64,
    /// 在放的和排着的有几个。
    pending: AtomicUsize,
    /// 开着的音箱、在放的那一个。
    open: Mutex<Option<Open>>,
}

struct Open {
    sink: MixerDeviceSink,
    playing: Option<Arc<Player>>,
}

impl Speaker {
    /// 放一段，放完交回 `true`，被 [`Speaker::stop`] 停了交回 `false`。
    ///
    /// # Errors
    ///
    /// 音频读不出来；没有音箱。
    pub fn play(&self, audio: Vec<u8>) -> Result<bool, PlayError> {
        let mine = self.generation.load(Ordering::SeqCst);
        self.pending.fetch_add(1, Ordering::SeqCst);
        let finished = self.play_in_turn(audio, mine);
        if self.pending.fetch_sub(1, Ordering::SeqCst) == 1 {
            // 最后一个放完了：关音箱。
            *self.open.lock().unwrap_or_else(PoisonError::into_inner) = None;
        }
        finished
    }

    /// 正在放的、排着的全停；交回停了几个。
    #[expect(dead_code, reason = "VO-10 登记 tts.stop 时用")]
    pub fn stop(&self) -> usize {
        let stopped = self.pending.load(Ordering::SeqCst);
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Some(open) = self
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
            player.append(decoder);
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
