//! 要真的模型：设 `MIYU_VOICE_TEST_MODELS` 指到装好了助手模型的缓存目录、`MIYU_VOICE_TEST_WAVS` 指到唤醒词模型自带的
//! `test_wavs/` 才跑（模型 30 多 MB，CI 上不下）；没设的跳过。

use std::path::PathBuf;

use super::{Event, Listener, RATE, Sensitivity};
use crate::keywords;
use crate::models::Models;

fn setup() -> Option<(Models, PathBuf)> {
    let models = std::env::var_os("MIYU_VOICE_TEST_MODELS").map(PathBuf::from)?;
    let wavs = std::env::var_os("MIYU_VOICE_TEST_WAVS").map(PathBuf::from)?;
    Some((Models::in_cache(&models), wavs))
}

/// 读 16 kHz 单声道的 wav，前后各垫 `pad` 秒静音（带一点底噪），一帧帧喂进去，交回听到的。
fn listen(listener: &mut Listener, wav: &std::path::Path, pad: usize) -> Vec<Event> {
    let pcm = voice_common::audio::read_wav(&std::fs::read(wav).unwrap()).unwrap();
    assert_eq!(pcm.rate as usize, RATE);
    let noise = |n: usize| -> Vec<f32> {
        (0..n)
            .map(|i| if i % 2 == 0 { 0.0005 } else { -0.0005 })
            .collect()
    };
    let mut audio = noise(pad * RATE);
    audio.extend(&pcm.samples);
    audio.extend(noise(pad * RATE));
    audio
        .chunks(512)
        .flat_map(|frame| listener.feed(frame))
        .collect()
}

#[test]
fn the_wake_words_in_the_sample_clips_are_heard() {
    let Some((models, wavs)) = setup() else {
        eprintln!("skipped: set MIYU_VOICE_TEST_MODELS and MIYU_VOICE_TEST_WAVS");
        return;
    };
    let vocabulary = keywords::tokens(&std::fs::read_to_string(&models.kws()[3]).unwrap());
    let words = ["周望军", "文森特卡索", "朱丽楠", "蒋友伯"];
    let lines: Vec<String> = words
        .iter()
        .flat_map(|word| keywords::encode(word, &vocabulary).unwrap())
        .collect();
    let mut heard = Vec::new();
    for clip in 0..7 {
        let mut listener = Listener::new(&models, &lines, Sensitivity::default()).unwrap();
        for event in listen(&mut listener, &wavs.join(format!("{clip}.wav")), 2) {
            if let Event::Wake { keyword, .. } = event {
                heard.push(keyword);
            }
        }
    }
    eprintln!("heard: {heard:?}");
    for word in words {
        assert!(
            heard.iter().any(|keyword| keyword == word),
            "{word} not heard in {heard:?}"
        );
    }
}

#[test]
fn an_open_window_takes_any_speech_without_a_wake_word() {
    let Some((models, wavs)) = setup() else {
        eprintln!("skipped: set MIYU_VOICE_TEST_MODELS and MIYU_VOICE_TEST_WAVS");
        return;
    };
    let vocabulary = keywords::tokens(&std::fs::read_to_string(&models.kws()[3]).unwrap());
    let lines = keywords::encode("密友密友", &vocabulary).unwrap();
    let mut listener = Listener::new(&models, &lines, Sensitivity::default()).unwrap();
    assert!(
        listen(&mut listener, &wavs.join("0.wav"), 2).is_empty(),
        "no wake word: nothing"
    );
    listener.set_window(true);
    let events = listen(&mut listener, &wavs.join("0.wav"), 2);
    assert!(
        matches!(events.as_slice(), [Event::Speech(audio)] if audio.len() > RATE),
        "{} events",
        events.len()
    );
}
