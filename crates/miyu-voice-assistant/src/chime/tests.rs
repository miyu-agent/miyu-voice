use super::{Chime, wav};

#[test]
fn the_chime_is_a_short_readable_quiet_wav() {
    let pcm = voice_common::audio::read_wav(&wav(Chime::Wake)).unwrap();
    assert!(pcm.duration_ms() > 50 && pcm.duration_ms() < 400);
    assert!(
        pcm.samples.iter().all(|s| s.abs() <= 0.26),
        "quiet enough, no clipping"
    );
    assert!(pcm.samples[0].abs() < 0.01, "fades in");
}
