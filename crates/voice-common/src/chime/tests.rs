use super::{Chime, LEAD_MS, wav};

#[test]
fn chimes_are_audible_quiet_enough_and_differ() {
    for chime in [Chime::Wake, Chime::Off] {
        let pcm = crate::audio::read_wav(&wav(chime)).unwrap();
        assert!(
            pcm.duration_ms() > 400 && pcm.duration_ms() < 800,
            "{chime:?}: {} ms",
            pcm.duration_ms()
        );
        assert!(
            pcm.samples.iter().all(|s| s.abs() <= 0.31),
            "quiet enough, no clipping"
        );
        let lead = (pcm.rate * LEAD_MS / 1000) as usize;
        assert!(
            pcm.samples[..lead].iter().all(|s| *s == 0.0),
            "silence first, so a sleeping sound card wakes before the tone"
        );
        assert!(
            pcm.samples[lead..].iter().any(|s| s.abs() > 0.2),
            "then a tone loud enough to hear"
        );
    }
    assert_ne!(wav(Chime::Wake), wav(Chime::Off));
}
