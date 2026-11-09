use super::split;

const RATE: u32 = 100;

#[test]
fn short_audio_is_one_piece() {
    let samples = vec![0.5; 25 * RATE as usize];
    assert_eq!(split(&samples, RATE), vec![0..samples.len()]);
}

#[test]
fn long_audio_is_cut_at_the_quiet_part_and_covers_everything() {
    // 70 秒的响声，24 秒处、50 秒处各安静 0.5 秒。
    let mut samples = vec![0.5f32; 70 * RATE as usize];
    for quiet in [24, 50] {
        let at = quiet * RATE as usize;
        samples[at..at + RATE as usize / 2].fill(0.0);
    }
    let pieces = split(&samples, RATE);
    assert_eq!(pieces.len(), 3, "{pieces:?}");
    assert_eq!(pieces[0].start, 0);
    assert_eq!(pieces.last().unwrap().end, samples.len());
    for pair in pieces.windows(2) {
        assert_eq!(pair[0].end, pair[1].start, "no gap, no overlap");
    }
    for piece in &pieces {
        assert!(piece.len() <= 30 * RATE as usize, "{piece:?}");
    }
    let first_cut_tenths = pieces[0].end * 10 / RATE as usize;
    assert!(
        (240..245).contains(&first_cut_tenths),
        "cut at {first_cut_tenths} tenths"
    );
}
