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
fn english_sentences_keep_a_space_between_them_chinese_ones_do_not() {
    assert_eq!(
        pieces("The build is green. Ship it to everyone now!  Done at 3.14 pm."),
        ["The build is green. Ship it to everyone now! Done at 3.14 pm."]
    );
    assert_eq!(
        pieces("构建已经通过了。马上发给大家吧！"),
        ["构建已经通过了。马上发给大家吧！"]
    );
    assert_eq!(
        pieces("版本号是 0.2.0. 接着发给大家吧。"),
        ["版本号是 0.2.0. 接着发给大家吧。"],
        "a sentence ending in latin punctuation is followed by a space"
    );
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

/// 几段 mp3 拼之前去掉每段的 ID3 标签和 Xing 头帧（审计 #126）：头帧里写着第一段的帧数，硬拼的播完第一段就停。
#[test]
fn mp3_pieces_lose_their_tags_and_header_frames_before_joining() {
    use crate::mp3::tests::{frame, id3};
    let header = [0xFF, 0xFB, 0x90, 0x00];
    let piece = |fill: u8| {
        let mut bytes = id3(20);
        bytes.extend(frame(header, 417, 0, Some((36, b"Xing"))));
        bytes.extend(frame(header, 417, fill, None));
        Audio {
            bytes,
            media_type: "audio/mpeg".to_string(),
            duration_ms: Some(500),
        }
    };
    let joined = joined(vec![piece(1), piece(2)]).unwrap();
    let mut expected = frame(header, 417, 1, None);
    expected.extend(frame(header, 417, 2, None));
    assert_eq!(joined.bytes.len(), expected.len());
    assert!(joined.bytes == expected, "only the audio frames are left");
    assert_eq!(joined.duration_ms, Some(1_000));
}

/// 引擎回了错（DeepSeek 余额不足）：她看到的报错带上引擎自己说的原因，不只是核心那句「方法执行失败。」（主人 11-20 验收第 6 条）。
#[test]
fn an_engine_failure_keeps_the_engine_s_reason() {
    let error = miyu_ext::CoreError {
        code: Some(-32000),
        reason: Some("method_failed".to_string()),
        message: "方法执行失败。".to_string(),
        data: json!({"reason": "method_failed", "code": "engine_failed",
                     "message": "DeepSeek text to speech failed (402: the DeepSeek balance is too low): Insufficient Balance"}),
    };
    assert_eq!(
        super::failed("deepseek-tools", &error).message(),
        "deepseek-tools: DeepSeek text to speech failed (402: the DeepSeek balance is too low): Insufficient Balance"
    );
}
