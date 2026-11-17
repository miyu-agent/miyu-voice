use std::sync::Mutex;

use super::{SayError, synthesize};
use crate::engine::{Audio, Engine, EngineError, Format, Request, Voice};

/// 记下收到的请求，交回固定的音频。
#[derive(Default)]
struct Fake {
    seen: Mutex<Vec<Request>>,
}

impl Engine for Fake {
    fn synthesize(&self, request: &Request) -> Result<Audio, EngineError> {
        self.seen.lock().unwrap().push(request.clone());
        Ok(Audio {
            bytes: b"ID3".to_vec(),
            media_type: "audio/mpeg".to_string(),
            duration_ms: None,
        })
    }

    fn voices(&self) -> Vec<Voice> {
        Vec::new()
    }
}

#[test]
fn the_engine_gets_speakable_text() {
    let fake = Fake::default();
    let audio = synthesize(
        &fake,
        "**好的**，看 `README`：\n\n```\nx\n```",
        "冰糖",
        1.0,
        Format::Mp3,
    )
    .unwrap();
    assert_eq!(audio.bytes, b"ID3");
    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen[0].text, "好的，看 README： 一段代码。");
    assert_eq!(seen[0].voice, "冰糖");
}

#[test]
fn nothing_to_say_and_too_long_are_refused_before_the_engine() {
    let fake = Fake::default();
    assert_eq!(
        synthesize(&fake, "👍\u{200b}", "", 1.0, Format::Mp3).unwrap_err(),
        SayError::EmptyText
    );
    let long = "好".repeat(5001);
    let error = synthesize(&fake, &long, "", 1.0, Format::Mp3).unwrap_err();
    assert_eq!(error.code(), "too_long");
    assert!(fake.seen.lock().unwrap().is_empty(), "no request was sent");
}

/// 照开头的字节认格式：wav、ogg、带 ID3 头的和裸帧的 mp3；认不出的不改（施工 VO-35）。
#[test]
fn the_real_format_is_read_from_the_bytes() {
    let wav = voice_common::audio::write_wav(&[0.0; 16], 16_000);
    assert_eq!(super::sniffed(&wav), Some("audio/wav"));
    assert_eq!(super::sniffed(b"OggS\0\x02"), Some("audio/ogg"));
    assert_eq!(super::sniffed(b"ID3\x04\0"), Some("audio/mpeg"));
    assert_eq!(
        super::sniffed(&[0xFF, 0xFB, 0x90, 0x00]),
        Some("audio/mpeg")
    );
    assert_eq!(
        super::sniffed(b"RIFF\0\0\0\0AVI "),
        None,
        "a RIFF that is not WAVE"
    );
    assert_eq!(super::sniffed(b"{\"error\""), None);
    assert_eq!(super::sniffed(&[]), None);
}
