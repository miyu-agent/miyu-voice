use serde_json::json;

use super::body;
use crate::engine::{Format, Request};

#[test]
fn the_body_is_the_audio_speech_shape() {
    let sent = body(
        "gpt-4o-mini-tts",
        &Request {
            text: "Done.".to_string(),
            voice: String::new(),
            speed: 1.2,
            format: Format::Mp3,
        },
    );
    assert_eq!(
        sent,
        json!({"model": "gpt-4o-mini-tts", "input": "Done.", "voice": "alloy", "response_format": "mp3", "speed": 1.2f32})
    );
}
