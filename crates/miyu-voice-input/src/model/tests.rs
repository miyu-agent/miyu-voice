use std::io::Write;

use sha2::{Digest, Sha256};

use super::{InstallError, Model, hex};

/// 一份 `.tar.bz2`：`files` 是（压缩包里的路径，内容）。
fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    for (path, body) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, path, *body).unwrap();
    }
    let tar = tar.into_inner().unwrap();
    let mut bz = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
    bz.write_all(&tar).unwrap();
    bz.finish().unwrap()
}

fn sha(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("miyu-voice-model-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn a_matching_archive_installs_the_two_files_only() {
    let cache = temp("ok");
    let bytes = archive(&[
        ("sv/model.int8.onnx", b"model"),
        ("sv/tokens.txt", b"tokens"),
        ("sv/test_wavs/zh.wav", b"skip me"),
    ]);
    let model = Model::in_cache(&cache);
    assert!(!model.ready());
    let mut seen = 0;
    model
        .install(bytes.as_slice(), &sha(&bytes), |n| seen = n)
        .unwrap();
    assert!(model.ready());
    assert_eq!(seen, bytes.len() as u64);
    assert_eq!(std::fs::read(model.model_file()).unwrap(), b"model");
    assert_eq!(std::fs::read(model.tokens_file()).unwrap(), b"tokens");
    assert!(!model.model_file().with_file_name("zh.wav").exists());
    let left: Vec<_> = std::fs::read_dir(&cache).unwrap().collect();
    assert_eq!(left.len(), 1, "no download or partial left behind");
    std::fs::remove_dir_all(&cache).unwrap();
}

#[test]
fn a_wrong_checksum_installs_nothing() {
    let cache = temp("bad");
    let bytes = archive(&[("model.int8.onnx", b"m"), ("tokens.txt", b"t")]);
    let model = Model::in_cache(&cache);
    let error = model.install(bytes.as_slice(), &"0".repeat(64), |_| {});
    assert!(
        matches!(error, Err(InstallError::Mismatch { .. })),
        "{error:?}"
    );
    assert!(!model.ready());
    assert_eq!(
        std::fs::read_dir(&cache).unwrap().count(),
        0,
        "the download is removed"
    );
    std::fs::remove_dir_all(&cache).unwrap();
}

#[test]
fn an_archive_without_tokens_is_refused() {
    let cache = temp("missing");
    let bytes = archive(&[("model.int8.onnx", b"m")]);
    let model = Model::in_cache(&cache);
    let error = model.install(bytes.as_slice(), &sha(&bytes), |_| {});
    assert!(
        matches!(error, Err(InstallError::Missing("tokens.txt"))),
        "{error:?}"
    );
    assert!(!model.ready());
    std::fs::remove_dir_all(&cache).unwrap();
}
