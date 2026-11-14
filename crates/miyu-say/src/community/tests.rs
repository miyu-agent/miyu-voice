use serde_json::json;
use voice_common::audio;

use super::{LONGEST_PIECE, joined, pieces, voices_in};
use crate::engine::Audio;

#[test]
fn long_text_is_cut_at_sentences_into_pieces_the_engine_takes() {
    let sentence = "今天下午三点在三楼的会议室开会，大家记得带上电脑。";
    let text = sentence.repeat(40);
    let cut = pieces(&text);
    assert!(
        cut.len() > 1,
        "{} chars is more than one piece",
        text.chars().count()
    );
    assert!(
        cut.iter()
            .all(|piece| piece.chars().count() <= LONGEST_PIECE)
    );
    assert!(
        cut.iter().all(|piece| piece.ends_with('。')),
        "cut between sentences: {cut:?}"
    );
    assert_eq!(cut.concat(), text, "nothing lost or added");
    assert_eq!(pieces("好的。"), ["好的。"]);
}

#[test]
fn one_sentence_longer_than_a_piece_is_cut_hard() {
    let text = "啊".repeat(LONGEST_PIECE * 2 + 3);
    let cut = pieces(&text);
    assert_eq!(
        cut.iter()
            .map(|piece| piece.chars().count())
            .collect::<Vec<_>>(),
        [LONGEST_PIECE, LONGEST_PIECE, 3]
    );
}

fn wav(samples: &[f32], rate: u32, duration_ms: Option<u64>) -> Audio {
    Audio {
        bytes: audio::write_wav(samples, rate),
        media_type: "audio/wav".to_string(),
        duration_ms,
    }
}

#[test]
fn wav_pieces_join_into_one_wav() {
    let joined = joined(vec![
        wav(&[0.5; 800], 8_000, Some(100)),
        wav(&[-0.5; 1600], 8_000, Some(200)),
    ])
    .unwrap();
    let pcm = audio::read_wav(&joined.bytes).unwrap();
    assert_eq!((pcm.rate, pcm.samples.len()), (8_000, 2400));
    assert!(pcm.samples[0] > 0.4 && pcm.samples[2399] < -0.4);
    assert_eq!(joined.duration_ms, Some(300));
}

#[test]
fn mp3_pieces_are_appended_and_mixed_ones_refused() {
    let mp3 = |bytes: &[u8]| Audio {
        bytes: bytes.to_vec(),
        media_type: "audio/mpeg".to_string(),
        duration_ms: None,
    };
    let joined = joined(vec![mp3(b"ab"), mp3(b"cd")]).unwrap();
    assert_eq!(
        (joined.bytes.as_slice(), joined.duration_ms),
        (&b"abcd"[..], None)
    );
    assert!(super::joined(vec![mp3(b"ab"), wav(&[0.0; 10], 8_000, None)]).is_err());
    assert!(
        super::joined(vec![
            wav(&[0.0; 10], 8_000, None),
            wav(&[0.0; 10], 16_000, None)
        ])
        .is_err()
    );
}

#[test]
fn voices_read_ids_and_fall_back_to_the_id_for_a_name() {
    let voices = voices_in(&json!({"voices": [
        {"id": "stella", "name": "Stella"},
        {"id": "tide"},
        {"name": "no id"},
    ]}));
    assert_eq!(
        voices
            .iter()
            .map(|voice| (voice.id.as_str(), voice.name.as_str()))
            .collect::<Vec<_>>(),
        [("stella", "Stella"), ("tide", "tide")]
    );
}
