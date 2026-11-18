use super::{QUIET_RMS, heard_speech, read_wav, wav_duration_ms, wav_heard_speech, write_wav};

#[test]
fn a_written_wav_reads_back() {
    let samples: Vec<f32> = (0..1600u16)
        .map(|n| f32::from(n % 40) / 40.0 - 0.5)
        .collect();
    let pcm = read_wav(&write_wav(&samples, 16_000)).unwrap();
    assert_eq!(pcm.rate, 16_000);
    assert_eq!(pcm.samples.len(), 1600);
    assert_eq!(pcm.duration_ms(), 100);
    for (got, want) in pcm.samples.iter().zip(&samples) {
        assert!((got - want).abs() < 1e-3, "{got} vs {want}");
    }
}

#[test]
fn stereo_is_averaged_and_the_rate_kept() {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut bytes = std::io::Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut bytes, spec).unwrap();
    for _ in 0..480 {
        writer.write_sample(i16::MAX).unwrap();
        writer.write_sample(0i16).unwrap();
    }
    writer.finalize().unwrap();
    let pcm = read_wav(&bytes.into_inner()).unwrap();
    assert_eq!(pcm.rate, 48_000);
    assert_eq!(pcm.samples.len(), 480);
    assert!((pcm.samples[0] - 0.5).abs() < 1e-3, "{}", pcm.samples[0]);
}

#[test]
fn not_a_wav_is_bad_audio() {
    let error = read_wav(b"OggS this is not a wav").unwrap_err();
    assert!(error.0.starts_with("Not a readable WAV file"), "{error}");
}

/// 数字静音（全零、只差一两个最低位）算静音；正常房间的底噪（比它大几十倍）和人声不算（施工 VO-26）。
#[test]
fn digital_silence_is_told_apart_from_a_quiet_room() {
    use super::{SILENT_PEAK, silent, wav_silent};
    let zeros = vec![0.0f32; 16_000];
    let lsb: Vec<f32> = (0..16_000)
        .map(|i| {
            if i % 2 == 0 {
                1.0 / 32768.0
            } else {
                -2.0 / 32768.0
            }
        })
        .collect();
    let room: Vec<f32> = (0..16_000u32)
        .map(|i| (f32::from(u16::try_from(i * 7919 % 200).unwrap()) - 100.0) / 100.0 * 0.002)
        .collect();
    let mut one_word = zeros.clone();
    one_word[8_000] = 0.3;
    assert!(silent(&zeros) && silent(&lsb) && silent(&[]));
    assert!(!silent(&room) && !silent(&one_word));
    for (samples, expected) in [
        (&zeros, true),
        (&lsb, true),
        (&room, false),
        (&one_word, false),
    ] {
        assert_eq!(wav_silent(&write_wav(samples, 16_000)).unwrap(), expected);
    }
    const { assert!(SILENT_PEAK < 0.0001) };
    assert!(wav_silent(b"not a wav").is_err());
}

/// 没听到说话（施工 VO-33）：只有底噪的整段算没听到；中间有一小段正常说话响度的就算听到了，开头一开口就说的也算；
/// 短得不到一帧的照样量；wav 的那一份和样本的那一份判得一样。
#[test]
fn speech_is_heard_by_the_loudest_frame_not_the_opening_noise() {
    let rate = 16_000;
    // 底噪：约 -46 dBFS 的方波，比门槛小。
    let hum = |n: usize| -> Vec<f32> {
        (0..n)
            .map(|i| if i % 2 == 0 { 0.005 } else { -0.005 })
            .collect()
    };
    // 正常说话的响度：约 -20 dBFS。
    let voice = |n: usize| -> Vec<f32> {
        (0..n)
            .map(|i| if i % 2 == 0 { 0.1 } else { -0.1 })
            .collect()
    };
    let quiet = hum(rate as usize * 3);
    assert!(!heard_speech(&quiet, rate));
    assert!(!wav_heard_speech(&write_wav(&quiet, rate)).unwrap());
    assert_eq!(wav_duration_ms(&write_wav(&quiet, rate)).unwrap(), 3000);
    // 中间说了 0.2 秒。
    let mut spoke = hum(rate as usize);
    spoke.extend(voice(3200));
    spoke.extend(hum(rate as usize));
    assert!(heard_speech(&spoke, rate));
    assert!(wav_heard_speech(&write_wav(&spoke, rate)).unwrap());
    // 一开口就说：开头就是人声。
    let mut at_once = voice(rate as usize / 2);
    at_once.extend(hum(rate as usize));
    assert!(heard_speech(&at_once, rate));
    // 不到一帧（30 毫秒是 480 个样本）的一小段。
    assert!(heard_speech(&voice(100), rate));
    assert!(!heard_speech(&hum(100), rate));
    // 门槛本身：正好到门槛的算听到。
    let edge: Vec<f32> = (0..480)
        .map(|i| if i % 2 == 0 { QUIET_RMS } else { -QUIET_RMS })
        .collect();
    assert!(heard_speech(&edge, rate));
}

/// 坏样本（NaN、无穷）当 0 算响度（审计 #127）：一个 NaN 不让够响的那一帧算成没声音，一个无穷不让静音算成有人说话。
#[test]
fn nan_and_infinite_samples_count_as_zero() {
    let mut loud = vec![0.5f32; 480];
    loud[10] = f32::NAN;
    assert!(
        heard_speech(&loud, 16_000),
        "one NaN does not hide the speech"
    );
    let mut quiet = vec![0.0f32; 480];
    quiet[10] = f32::INFINITY;
    assert!(!heard_speech(&quiet, 16_000), "one infinity is not speech");
}
