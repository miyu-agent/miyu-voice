//! 录本机的麦克风（设计 20 第三节「形状」`stt.record`）：先拿整机的麦克风锁（助手让出），照设备名选麦克风、选不到用
//! 系统默认的（第十一节），录成单声道，一帧帧交给 [`Endpoint`] 判什么时候停；`stt.stop` 经 [`Stopper`] 叫停。
//! 录完交回原采样率的样本，转 16 kHz、认，交给识别的子进程。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use voice_common::audio::Pcm;
use voice_common::mic_lock::{Busy, MicLock};

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
    #[expect(dead_code, reason = "VO-9 登记 stt.stop 时用")]
    pub fn stop(&self) -> bool {
        let recording = self.recording.load(Ordering::SeqCst);
        if recording {
            self.flag.store(true, Ordering::SeqCst);
        }
        recording
    }
}

/// 录：`device` 是设备名（不写、找不到的用默认的），人声停了 `silence_ms` 停，最长 `max_ms`。
///
/// # Errors
///
/// 麦克风被别的听写拿着；没有麦克风、打不开。
pub fn record(
    device: Option<&str>,
    silence_ms: u64,
    max_ms: u64,
    stopper: &Stopper,
) -> Result<(Pcm, Ended), RecordError> {
    let _lock = MicLock::for_dictation().map_err(|Busy| RecordError::MicBusy)?;
    let host = cpal::default_host();
    let chosen = device.and_then(|name| {
        host.input_devices().ok()?.find(|found| {
            found
                .description()
                .is_ok_and(|description| description.name() == name)
        })
    });
    let input = chosen
        .or_else(|| host.default_input_device())
        .ok_or_else(|| RecordError::NoMicrophone("No microphone was found.".to_string()))?;
    let supported = input.default_input_config().map_err(|error| {
        RecordError::NoMicrophone(format!("The microphone cannot be opened: {error}."))
    })?;
    let rate = supported.sample_rate();
    let channels = usize::from(supported.channels());
    let config = supported.config();
    let (chunks, received) = mpsc::channel::<Vec<f32>>();
    let failed: Arc<Mutex<Option<String>>> = Arc::default();
    let stream = match supported.sample_format() {
        SampleFormat::F32 => open::<f32>(&input, &config, channels, chunks, &failed),
        SampleFormat::I16 => open::<i16>(&input, &config, channels, chunks, &failed),
        SampleFormat::U16 => open::<u16>(&input, &config, channels, chunks, &failed),
        SampleFormat::I32 => open::<i32>(&input, &config, channels, chunks, &failed),
        other => Err(format!("Unsupported microphone sample format {other}.")),
    }
    .map_err(RecordError::NoMicrophone)?;
    stream.play().map_err(|error| {
        RecordError::NoMicrophone(format!("The microphone cannot start: {error}."))
    })?;

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
        if let Some(why) = failed.lock().unwrap_or_else(PoisonError::into_inner).take() {
            stopper.recording.store(false, Ordering::SeqCst);
            return Err(RecordError::NoMicrophone(why));
        }
        if let Ok(chunk) = received.recv_timeout(Duration::from_millis(50)) {
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
    drop(stream);
    Ok((Pcm { samples, rate }, ended))
}

/// 开一路录音：回调里把几个声道平均成一个，交给录的那一头。
fn open<T>(
    input: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    chunks: mpsc::Sender<Vec<f32>>,
    failed: &Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let failed = Arc::clone(failed);
    input
        .build_input_stream::<T, _, _>(
            *config,
            move |data: &[T], _| {
                let mono: Vec<f32> = data
                    .chunks(channels.max(1))
                    .map(|frame| {
                        let sum: f32 = frame.iter().map(|s| s.to_sample::<f32>()).sum();
                        sum / f32::from(u16::try_from(frame.len()).unwrap_or(1))
                    })
                    .collect();
                let _ = chunks.send(mono);
            },
            move |error| {
                *failed.lock().unwrap_or_else(PoisonError::into_inner) =
                    Some(format!("The microphone stopped: {error}."));
            },
            None,
        )
        .map_err(|error| format!("The microphone cannot be opened: {error}."))
}
