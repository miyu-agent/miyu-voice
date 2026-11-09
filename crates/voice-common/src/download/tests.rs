use std::io::Write;

use sha2::{Digest, Sha256};

use super::{InstallError, hex, install_archive, install_file, ready};

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
    let dir =
        std::env::temp_dir().join(format!("miyu-voice-download-{name}-{}", std::process::id()));
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
    let dir = cache.join("sv");
    let wanted = ["model.int8.onnx", "tokens.txt"];
    assert!(!ready(&dir, &wanted));
    let mut seen = 0;
    install_archive(bytes.as_slice(), &sha(&bytes), &dir, &wanted, |n| seen = n).unwrap();
    assert!(ready(&dir, &wanted));
    assert_eq!(seen, bytes.len() as u64);
    assert_eq!(
        std::fs::read(dir.join("model.int8.onnx")).unwrap(),
        b"model"
    );
    assert_eq!(std::fs::read(dir.join("tokens.txt")).unwrap(), b"tokens");
    assert!(!dir.join("zh.wav").exists());
    let left: Vec<_> = std::fs::read_dir(&cache).unwrap().collect();
    assert_eq!(left.len(), 1, "no download or partial left behind");
    std::fs::remove_dir_all(&cache).unwrap();
}

#[test]
fn a_wrong_checksum_installs_nothing() {
    let cache = temp("bad");
    let bytes = archive(&[("model.int8.onnx", b"m"), ("tokens.txt", b"t")]);
    let dir = cache.join("sv");
    let error = install_archive(
        bytes.as_slice(),
        &"0".repeat(64),
        &dir,
        &["model.int8.onnx"],
        |_| {},
    );
    assert!(
        matches!(error, Err(InstallError::Mismatch { .. })),
        "{error:?}"
    );
    assert!(!ready(&dir, &["model.int8.onnx"]));
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
    let dir = cache.join("sv");
    let error = install_archive(
        bytes.as_slice(),
        &sha(&bytes),
        &dir,
        &["model.int8.onnx", "tokens.txt"],
        |_| {},
    );
    assert!(
        matches!(&error, Err(InstallError::Missing(name)) if name == "tokens.txt"),
        "{error:?}"
    );
    assert!(!ready(&dir, &["model.int8.onnx", "tokens.txt"]));
    std::fs::remove_dir_all(&cache).unwrap();
}

#[test]
fn a_single_file_is_checked_and_installed() {
    let cache = temp("file");
    let dir = cache.join("vad");
    install_file(&b"onnx"[..], &sha(b"onnx"), &dir, "silero_vad.onnx", |_| {}).unwrap();
    assert!(ready(&dir, &["silero_vad.onnx"]));
    let other = cache.join("bad");
    assert!(install_file(&b"onnx"[..], &"0".repeat(64), &other, "x.onnx", |_| {}).is_err());
    assert!(!ready(&other, &["x.onnx"]));
    std::fs::remove_dir_all(&cache).unwrap();
}
