use super::{read_wav, write_wav};

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
