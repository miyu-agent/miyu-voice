//! 录本机的麦克风（设计 20 第三节「形状」`stt.record`）：先拿整机的麦克风锁（助手让出），照设备名选麦克风、选不到用
//! 系统默认的（第十一节），录成单声道，一帧帧交给 [`Endpoint`] 判什么时候停；`stt.stop` 经 [`Stopper`] 叫停。
//! 录完交回原采样率的样本，转 16 kHz、认，交给识别的子进程。
//!
//! 提示音（施工 VO-25，项目主人 2026-11-13）：给了音箱的，麦克风开好以后先响上扬的一声、响完才开始听（这一声里麦克风收到的
//! 丢掉，不当成人在说话），停了响下沉的一声。麦克风被占、打不开的不响。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use voice_common::audio::Pcm;
use voice_common::chime::{self, Chime};
use voice_common::mic::Mic;
use voice_common::mic_lock::{LockError, MicLock};
use voice_common::speaker::Speaker;

use crate::endpoint::{Ended, Endpoint, FRAME_MS};

/// 录不了的原因（`stt.record` 回的错）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordError {
    /// 另一个听写在录（`mic_busy`）。
    MicBusy,
    /// 找不到、打不开麦克风（`no_microphone`）。
    NoMicrophone(String),
}

/// 叫停正在录的那一个（`stt.stop`）。
#[derive(Debug, Clone, Default)]
pub struct Stopper {
    flag: Arc<AtomicBool>,
    recording: Arc<AtomicBool>,
}

impl Stopper {
    /// 叫停；交回有没有在录的。
    pub fn stop(&self) -> bool {
        let recording = self.recording.load(Ordering::SeqCst);
        if recording {
            self.flag.store(true, Ordering::SeqCst);
        }
        recording
    }
}

/// 录：`device` 是设备名（不写、找不到的用默认的），人声停了 `silence_ms` 停，最长 `max_ms`；`chimes` 给了音箱的，开始、
/// 停止各响一声。
///
/// # Errors
///
/// 麦克风被别的听写拿着；没有麦克风、打不开。
pub fn record(
    device: Option<&str>,
    silence_ms: u64,
    max_ms: u64,
    stopper: &Stopper,
    chimes: Option<&Speaker>,
) -> Result<(Pcm, Ended), RecordError> {
    let _lock = MicLock::for_dictation().map_err(|error| match error {
        LockError::Busy => RecordError::MicBusy,
        LockError::Unsafe(why) => RecordError::NoMicrophone(format!(
            "The microphone lock folder is not safe to use: {why}."
        )),
    })?;
    let mic = Mic::open(device).map_err(RecordError::NoMicrophone)?;
    let rate = mic.rate;
    if let Some(speaker) = chimes {
        chime::play_and_wait(speaker, Chime::Wake);
        while mic.chunks.try_recv().is_ok() {}
    }
    let result = listen(&mic, rate, silence_ms, max_ms, stopper);
    drop(mic);
    if let Some(speaker) = chimes {
        chime::play(speaker, Chime::Off);
    }
    result
}

/// 麦克风开好以后的这一段：一帧帧交给 [`Endpoint`]，到它说停、被叫停、设备出错。
fn listen(
    mic: &Mic,
    rate: u32,
    silence_ms: u64,
    max_ms: u64,
    stopper: &Stopper,
) -> Result<(Pcm, Ended), RecordError> {
    stopper.flag.store(false, Ordering::SeqCst);
    stopper.recording.store(true, Ordering::SeqCst);
    let frame = usize::try_from(u64::from(rate) * FRAME_MS / 1000)
        .unwrap_or(480)
        .max(1);
    let mut endpoint = Endpoint::new(silence_ms, max_ms);
    let mut samples = Vec::new();
    let mut fed = 0;
    let ended = 'listening: loop {
        if stopper.flag.load(Ordering::SeqCst) {
            break Ended::Stop;
        }
        if let Some(why) = mic.failure() {
            stopper.recording.store(false, Ordering::SeqCst);
            return Err(RecordError::NoMicrophone(why));
        }
        if let Ok(chunk) = mic.chunks.recv_timeout(Duration::from_millis(50)) {
            samples.extend(chunk);
        }
        while samples.len() - fed >= frame {
            if let Some(ended) = endpoint.feed(&samples[fed..fed + frame]) {
                break 'listening ended;
            }
            fed += frame;
        }
    };
    stopper.recording.store(false, Ordering::SeqCst);
    Ok((Pcm { samples, rate }, ended))
}
