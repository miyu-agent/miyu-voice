use super::{Chime, wav};

#[test]
fn chimes_are_short_readable_quiet_wavs_that_differ() {
    for chime in [Chime::Wake, Chime::Off] {
        let pcm = voice_common::audio::read_wav(&wav(chime)).unwrap();
        assert!(
            pcm.duration_ms() > 50 && pcm.duration_ms() < 400,
            "{chime:?}"
        );
        assert!(
            pcm.samples.iter().all(|s| s.abs() <= 0.26),
            "quiet enough, no clipping"
        );
        assert!(pcm.samples[0].abs() < 0.01, "fades in");
    }
    assert_ne!(wav(Chime::Wake), wav(Chime::Off));
}
