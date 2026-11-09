use std::sync::mpsc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use miyu_ext::{Core, Extension, FakeCore};
use serde_json::{Map, Value, json};

use super::{CHUNK, get, put};

/// 起一个扩展，假核心照 `answer` 答，答法装好以后在扩展里跑 `run`；交回 `run` 的结果和假核心收到的请求。
fn run_in_extension<T: Send + 'static>(
    answer: impl Fn(&str, &Value) -> Value + Send + Sync + Copy + 'static,
    methods: &[&str],
    run: impl Fn(&Core) -> T + Send + Sync + 'static,
) -> (T, Vec<Value>) {
    let (go, wait) = mpsc::channel::<()>();
    let wait = std::sync::Mutex::new(wait);
    let (done, result) = mpsc::channel();
    let mut ext = Extension::new("t", "0");
    ext.ready(move |core| {
        wait.lock().unwrap().recv().unwrap();
        done.send(run(core)).unwrap();
    });
    let core = FakeCore::start(ext, Map::new());
    for method in methods {
        let name = (*method).to_string();
        core.answer(method, move |params| Ok(answer(&name, params)));
    }
    go.send(()).unwrap();
    let got = result.recv().unwrap();
    let received = core.received();
    core.close().unwrap();
    (got, received)
}

#[test]
fn small_audio_goes_in_one_put_and_big_audio_in_chunks() {
    let (blobs, received) = run_in_extension(
        |method, _| match method {
            "blob.put" => json!({"blob": "sha256:a"}),
            "blob.open" => json!({"upload": "u1"}),
            "blob.write" => json!({"received": 1}),
            _ => json!({"blob": "sha256:b"}),
        },
        &["blob.put", "blob.open", "blob.write", "blob.close"],
        |core| {
            let small = put(core, b"ID3", "a.mp3", "audio/mpeg").unwrap();
            let big = put(core, &vec![7u8; CHUNK * 2 + 10], "b.mp3", "audio/mpeg").unwrap();
            (small, big)
        },
    );
    assert_eq!(blobs, ("sha256:a".to_string(), "sha256:b".to_string()));
    let offsets: Vec<u64> = received
        .iter()
        .filter(|m| m["method"] == "blob.write")
        .map(|m| m["params"]["offset"].as_u64().unwrap())
        .collect();
    assert_eq!(offsets, [0, CHUNK as u64, 2 * CHUNK as u64]);
}

#[test]
fn reading_asks_the_size_then_reads_the_parts() {
    let (bytes, received) = run_in_extension(
        |_, params| {
            if params["length"] == 0 {
                json!({"data": "", "size": 8})
            } else {
                json!({"data": STANDARD.encode(b"RIFFdata"), "size": 8})
            }
        },
        &["blob.get"],
        |core| get(core, "sha256:x").unwrap(),
    );
    assert_eq!(bytes, b"RIFFdata");
    assert_eq!(
        received
            .iter()
            .filter(|m| m["method"] == "blob.get")
            .count(),
        2
    );
}
