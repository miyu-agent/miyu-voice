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
    .unwrap()
    .audio;
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

/// 列全了音色的引擎（社区引擎）：配的音色不在里头（换了引擎，还是上一家的编号）就交空的、用它的默认，记下配的是哪个；在里头的照交。
/// 列不全的（MiniMax 克隆的音色）照配的交（施工 VO-45）。
#[test]
fn a_voice_the_engine_does_not_have_falls_back_to_its_default() {
    struct Listed {
        every: bool,
        seen: Mutex<Vec<String>>,
    }
    impl Engine for Listed {
        fn synthesize(&self, request: &Request) -> Result<Audio, EngineError> {
            self.seen.lock().unwrap().push(request.voice.clone());
            Ok(Audio {
                bytes: b"ID3".to_vec(),
                media_type: "audio/mpeg".to_string(),
                duration_ms: None,
            })
        }
        fn voices(&self) -> Vec<Voice> {
            ["stella", "tide"]
                .map(|id| Voice {
                    id: id.to_string(),
                    name: id.to_string(),
                })
                .to_vec()
        }
        fn lists_every_voice(&self) -> bool {
            self.every
        }
    }
    let deepseek = Listed {
        every: true,
        seen: Mutex::default(),
    };
    let said = synthesize(&deepseek, "好的。", "female-shaonv", 1.0, Format::Mp3).unwrap();
    assert_eq!(said.unknown_voice.as_deref(), Some("female-shaonv"));
    let kept = synthesize(&deepseek, "好的。", "tide", 1.0, Format::Mp3).unwrap();
    assert_eq!(kept.unknown_voice, None);
    assert_eq!(*deepseek.seen.lock().unwrap(), ["", "tide"]);
    let minimax = Listed {
        every: false,
        seen: Mutex::default(),
    };
    let cloned = synthesize(&minimax, "好的。", "my-clone", 1.0, Format::Mp3).unwrap();
    assert_eq!(cloned.unknown_voice, None);
    assert_eq!(*minimax.seen.lock().unwrap(), ["my-clone"]);
}
