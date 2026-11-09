use serde_json::json;

use super::{body, parse};
use crate::engine::{EngineError, Format, Request};

fn request(voice: &str, speed: f32) -> Request {
    Request {
        text: "好的，三点见。".to_string(),
        voice: voice.to_string(),
        speed,
        format: Format::Mp3,
    }
}

#[test]
fn the_body_follows_the_t2a_v2_shape() {
    let sent = body("speech-2.8-turbo", &request("", 3.0));
    assert_eq!(sent["model"], "speech-2.8-turbo");
    assert_eq!(sent["text"], "好的，三点见。");
    assert_eq!(sent["stream"], false);
    assert_eq!(sent["output_format"], "hex");
    assert_eq!(
        sent["voice_setting"]["voice_id"], "female-tianmei",
        "default voice"
    );
    assert_eq!(sent["voice_setting"]["speed"], 2.0, "clamped");
    assert_eq!(sent["audio_setting"]["format"], "mp3");
}

#[test]
fn hex_audio_and_length_are_read() {
    let answer = json!({
        "data": {"audio": "49443303", "status": 2},
        "extra_info": {"audio_length": 1650, "audio_format": "mp3"},
        "base_resp": {"status_code": 0, "status_msg": "success"}
    });
    let audio = parse(answer.to_string().as_bytes(), Format::Mp3).unwrap();
    assert_eq!(audio.bytes, [0x49, 0x44, 0x33, 0x03]);
    assert_eq!(audio.media_type, "audio/mpeg");
    assert_eq!(audio.duration_ms, Some(1650));
}

#[test]
fn a_refusal_says_the_code_and_message() {
    let answer = json!({"base_resp": {"status_code": 1004, "status_msg": "authentication failed"}});
    let error = parse(answer.to_string().as_bytes(), Format::Mp3).unwrap_err();
    assert_eq!(
        error,
        EngineError::Failed("MiniMax refused (1004): authentication failed.".to_string())
    );
    assert_eq!(error.code(), "engine_failed");
}
