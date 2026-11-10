//! 打开本机的麦克风（设计 20 第三节 `stt.record`、第九节助手）：照配置里选的那个开、选不到用系统默认的（第十一节）；Linux 上
//! 有声音服务（PipeWire、PulseAudio）的经它开，不直接占声卡，没有的经 ALSA（施工 VO-22）。几个声道平均
//! 成一个，一块块交出来。听写和助手共用；锁不在这里（[`crate::mic_lock`]）。

use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

/// 系统是照哪一套列设备的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Listing {
    /// 声音服务（PipeWire 的 PulseAudio 接口、PulseAudio）：列的是它的一个个来源，同系统声音设置里的。
    Server,
    /// ALSA（Linux 上没有声音服务时）。
    Alsa,
    /// macOS、Windows。
    Plain,
}

/// 系统列的一个能录音的设备。
struct Found {
    /// 这一套里不变的编号（声音服务的来源名）。
    id: String,
    /// 给人看的名字。
    name: String,
    driver: Option<String>,
}

fn found(host: &cpal::Host) -> (Listing, Vec<Found>) {
    let listing = match format!("{:?}", host.id()).as_str() {
        "PulseAudio" | "PipeWire" => Listing::Server,
        "Alsa" => Listing::Alsa,
        _ => Listing::Plain,
    };
    let devices = host.input_devices().map_or_else(
        |_| Vec::new(),
        |devices| {
            devices
                .filter_map(|device| {
                    let description = device.description().ok()?;
                    description.supports_input().then(|| Found {
                        id: device
                            .id()
                            .map(|id| id.id().to_string())
                            .unwrap_or_default(),
                        name: description.name().to_string(),
                        driver: description.driver().map(str::to_string),
                    })
                })
                .collect()
        },
    );
    (listing, devices)
}

/// 这台电脑上能选的麦克风，照系统列的先后：（存进配置的值，给人看的名字）（配置页的下拉用，施工 VO-21、VO-22）。只列，不开
/// 麦克风。
#[must_use]
pub fn listed() -> Vec<(String, String)> {
    let (listing, devices) = found(&cpal::default_host());
    pick(&devices, listing)
}

/// 从系统列的设备里挑出给人选的，重的只留一个。
///
/// - 声音服务（施工 VO-22，项目主人 2026-11-11：「可以走 pipewire，如果有的话」）：一个来源一项，不要「监听某个输出」的那种
///   （`….monitor`，录的是电脑自己放的声音）；值是来源的编号（不随界面语言变），名字是系统声音设置里的那个。
/// - ALSA：重采样、空设备这类插件、同一块声卡的几种开法都列成设备，只留每块声卡的 `sysdefault:CARD=…` 那一项；值和名字都是
///   声卡的名字。
/// - 别的系统列的本来就是一个个设备，全留；值和名字都是设备名。
fn pick(found: &[Found], listing: Listing) -> Vec<(String, String)> {
    let mut picked: Vec<(String, String)> = Vec::new();
    for device in found {
        let (wanted, value) = match listing {
            Listing::Server => (!device.id.ends_with(".monitor"), &device.id),
            Listing::Alsa => (
                device
                    .driver
                    .as_deref()
                    .is_some_and(|driver| driver.starts_with("sysdefault:CARD=")),
                &device.name,
            ),
            Listing::Plain => (true, &device.name),
        };
        if wanted && !value.is_empty() && !picked.iter().any(|(seen, _)| seen == value) {
            picked.push((value.clone(), device.name.clone()));
        }
    }
    picked
}

/// 配置页麦克风下拉的选项（核心 SG-2 的 `choices_from`：回 `{"choices": [{"value", "name"}]}`）：头一项是空值「系统默认」，
/// 后面是 [`listed`]。`language` 是界面语言。
#[must_use]
pub fn choices(language: &str) -> serde_json::Value {
    let default = match language {
        "zh" => "系统默认",
        "ja" => "システムの既定",
        _ => "System default",
    };
    let mut choices = vec![serde_json::json!({"value": "", "name": default})];
    choices.extend(
        listed()
            .into_iter()
            .map(|(value, name)| serde_json::json!({"value": value, "name": name})),
    );
    serde_json::json!({"choices": choices})
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
        // 配置里存的是下拉给的值：声音服务的来源编号，或者设备名（ALSA、别的系统，和以前手填的）。
        let chosen = device.and_then(|wanted| {
            host.input_devices().ok()?.find(|found| {
                found.id().is_ok_and(|id| id.id() == wanted)
                    || found
                        .description()
                        .is_ok_and(|description| description.name() == wanted)
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
