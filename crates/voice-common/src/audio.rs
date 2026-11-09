//! wav 读成单声道、`f32` 的样本（设计 20 第三节「形状」）：`stt.transcribe` 必须认 16 kHz、单声道、16 位 PCM 的 wav，
//! 别的采样率、声道、位深的 wav 也收。几个声道取平均；采样率照原样交回，转成 16 kHz 由识别的一方做（它带着重采样）。

use std::io::Cursor;

/// 读出来的一段声音。
#[derive(Debug, Clone, PartialEq)]
pub struct Pcm {
    /// 单声道的样本，`-1.0` 到 `1.0`。
    pub samples: Vec<f32>,
    /// 采样率（赫兹）。
    pub rate: u32,
}

impl Pcm {
    /// 多长，毫秒。
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        if self.rate == 0 {
            return 0;
        }
        self.samples.len() as u64 * 1000 / u64::from(self.rate)
    }
}

/// 读不出来的原因，给人看的一句（`stt.transcribe` 回 `bad_audio` 时带上）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadAudio(pub String);

impl std::fmt::Display for BadAudio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// 把 wav 文件的全部字节读成单声道的样本。
///
/// # Errors
///
/// 不是 wav、wav 坏了、是没见过的样本格式、没有声道。
pub fn read_wav(bytes: &[u8]) -> Result<Pcm, BadAudio> {
    let reader = hound::WavReader::new(Cursor::new(bytes))
        .map_err(|error| BadAudio(format!("Not a readable WAV file: {error}.")))?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels);
    if channels == 0 || spec.sample_rate == 0 {
        return Err(BadAudio("The WAV file has no channels.".to_string()));
    }
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|error| broken(&error))?,
        hound::SampleFormat::Int => {
            let scale = 2f32.powi(i32::from(spec.bits_per_sample) - 1);
            reader
                .into_samples::<i32>()
                .map(|sample| sample.map(|value| int_to_f32(value) / scale))
                .collect::<Result<_, _>>()
                .map_err(|error| broken(&error))?
        }
    };
    let samples = interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / count_to_f32(frame.len()))
        .collect();
    Ok(Pcm {
        samples,
        rate: spec.sample_rate,
    })
}

/// 只看 wav 的头读不读得出来、不解样本：交给别的进程解之前先挡掉明显不是 wav 的（`bad_audio`）。
///
/// # Errors
///
/// 不是 wav、头坏了、没有声道。
pub fn check_wav(bytes: &[u8]) -> Result<(), BadAudio> {
    let reader = hound::WavReader::new(Cursor::new(bytes))
        .map_err(|error| BadAudio(format!("Not a readable WAV file: {error}.")))?;
    let spec = reader.spec();
    if spec.channels == 0 || spec.sample_rate == 0 {
        return Err(BadAudio("The WAV file has no channels.".to_string()));
    }
    Ok(())
}

/// 单声道、16 位 PCM 的 wav 文件的字节：交给社区引擎（设计 20 第三节 `stt.engine.transcribe`）、测试造数据用。
#[must_use]
pub fn write_wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut out = Cursor::new(Vec::new());
    // 写进内存不会失败：只有底下的写会出错，Vec 不会。
    if let Ok(mut writer) = hound::WavWriter::new(&mut out, spec) {
        for sample in samples {
            let _ = writer.write_sample(f32_to_i16(*sample));
        }
        let _ = writer.finalize();
    }
    out.into_inner()
}

fn broken(error: &hound::Error) -> BadAudio {
    BadAudio(format!("The WAV file is damaged: {error}."))
}

#[expect(
    clippy::cast_precision_loss,
    reason = "样本最多 32 位，转 f32 的精度够听"
)]
fn int_to_f32(value: i32) -> f32 {
    value as f32
}

#[expect(clippy::cast_precision_loss, reason = "声道数最多 65535")]
fn count_to_f32(count: usize) -> f32 {
    count as f32
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "先夹到 -1 到 1，乘出来在 i16 以内"
)]
fn f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
}

#[cfg(test)]
mod tests;
