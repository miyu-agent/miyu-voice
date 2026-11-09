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
