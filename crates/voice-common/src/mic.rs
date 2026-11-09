//! 打开本机的麦克风（设计 20 第三节 `stt.record`、第九节助手）：照设备名选、选不到用系统默认的（第十一节），几个声道平均
//! 成一个，一块块交出来。听写和助手共用；锁不在这里（[`crate::mic_lock`]）。

use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

/// 开着的麦克风：丢掉就关。
pub struct Mic {
    /// 收到的声音，单声道、原采样率。
    pub chunks: mpsc::Receiver<Vec<f32>>,
    /// 采样率。
    pub rate: u32,
    failed: Arc<Mutex<Option<String>>>,
    _stream: cpal::Stream,
}

impl Mic {
    /// 打开：`device` 是设备名，不写、找不到的用默认的。
    ///
    /// # Errors
    ///
    /// 找不到麦克风、打不开、样本格式不认识。
    pub fn open(device: Option<&str>) -> Result<Mic, String> {
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
            .ok_or_else(|| "No microphone was found.".to_string())?;
        let supported = input
            .default_input_config()
            .map_err(|error| format!("The microphone cannot be opened: {error}."))?;
        let rate = supported.sample_rate();
        let channels = usize::from(supported.channels());
        let config = supported.config();
        let (sender, chunks) = mpsc::channel::<Vec<f32>>();
        let failed: Arc<Mutex<Option<String>>> = Arc::default();
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(&input, &config, channels, sender, &failed),
            SampleFormat::I16 => build::<i16>(&input, &config, channels, sender, &failed),
            SampleFormat::U16 => build::<u16>(&input, &config, channels, sender, &failed),
            SampleFormat::I32 => build::<i32>(&input, &config, channels, sender, &failed),
            other => Err(format!("Unsupported microphone sample format {other}.")),
        }?;
        stream
            .play()
            .map_err(|error| format!("The microphone cannot start: {error}."))?;
        Ok(Mic {
            chunks,
            rate,
            failed,
            _stream: stream,
        })
    }

    /// 设备半路出了错（拔了、被占了）交回为什么。
    pub fn failure(&self) -> Option<String> {
        self.failed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}

fn build<T>(
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
                // 收的那一头走了（关麦）：丢掉。
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
