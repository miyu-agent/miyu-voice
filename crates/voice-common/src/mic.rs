//! 打开本机的麦克风（设计 20 第三节 `stt.record`、第九节助手）：照设备名选、选不到用系统默认的（第十一节），几个声道平均
//! 成一个，一块块交出来。听写和助手共用；锁不在这里（[`crate::mic_lock`]）。

use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

/// 这台电脑上能选的麦克风的设备名，照系统列的先后，重名的只留一个（配置页的下拉用，施工 VO-21）。只列名字，不开麦克风。
#[must_use]
pub fn names() -> Vec<String> {
    let Ok(devices) = cpal::default_host().input_devices() else {
        return Vec::new();
    };
    let found: Vec<(String, Option<String>)> = devices
        .filter_map(|device| device.description().ok())
        .filter(cpal::DeviceDescription::supports_input)
        .map(|description| {
            (
                description.name().to_string(),
                description.driver().map(str::to_string),
            )
        })
        .collect();
    pick(&found, cfg!(target_os = "linux"))
}

/// 配置页麦克风下拉的选项（核心 SG-2 的 `choices_from`：回 `{"choices": [{"value", "name"}]}`）：头一项是空值「系统默认」，
/// 后面是 [`names`]。`language` 是界面语言。
#[must_use]
pub fn choices(language: &str) -> serde_json::Value {
    let default = match language {
        "zh" => "系统默认",
        "ja" => "システムの既定",
        _ => "System default",
    };
    let mut choices = vec![serde_json::json!({"value": "", "name": default})];
    choices.extend(
        names()
            .into_iter()
            .map(|name| serde_json::json!({"value": name, "name": name})),
    );
    serde_json::json!({"choices": choices})
}

/// 从系统列的设备（名字、驱动）里挑出给人选的。`alsa`（Linux）：ALSA 把重采样、空设备这类插件、同一块声卡的几种开法都列成
/// 设备，只留每块声卡的 `sysdefault:CARD=…` 那一项（带转换、名字是声卡的名字）；别的系统列的本来就是一个个设备，全留。
fn pick(found: &[(String, Option<String>)], alsa: bool) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (name, driver) in found {
        let card = driver
            .as_deref()
            .is_some_and(|driver| driver.starts_with("sysdefault:CARD="));
        if (card || !alsa) && !name.is_empty() && !names.contains(name) {
            names.push(name.clone());
        }
    }
    names
}

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

#[cfg(test)]
mod tests;
