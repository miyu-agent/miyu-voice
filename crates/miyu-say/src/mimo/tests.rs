use serde_json::json;

use super::{body, parse};
use crate::engine::{Format, Request};

#[test]
fn the_text_goes_in_an_assistant_message() {
    let sent = body(
        "mimo-v2.5-tts",
        &Request {
            text: "编译完了。".to_string(),
            voice: "冰糖".to_string(),
            speed: 1.0,
            format: Format::Wav,
        },
    );
    assert_eq!(
        sent["messages"],
        json!([{"role": "assistant", "content": "编译完了。"}])
    );
    assert_eq!(sent["audio"], json!({"format": "wav", "voice": "冰糖"}));
}

#[test]
fn base64_audio_is_read_and_errors_are_said() {
    let answer =
        json!({"choices": [{"message": {"role": "assistant", "audio": {"data": "UklGRg=="}}}]});
    let audio = parse(answer.to_string().as_bytes(), Format::Wav).unwrap();
    assert_eq!(audio.bytes, b"RIFF");
    assert_eq!(audio.media_type, "audio/wav");
    let refused = json!({"error": {"message": "Invalid API key"}});
    let error = parse(refused.to_string().as_bytes(), Format::Wav).unwrap_err();
    assert_eq!(error.message(), "MiMo refused: Invalid API key");
}
