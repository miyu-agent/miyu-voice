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

/// 对方连上了不回话的，到时限就放弃（审计 #127）：没有时限的话识别、唤醒模型的下载会一直挂着。
#[test]
fn a_server_that_never_answers_is_given_up_on() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/model.tar.bz2", listener.local_addr().unwrap());
    // 接了连接不回话，测试完才关。
    let holder = std::thread::spawn(move || listener.accept().map(|(stream, _)| stream));
    let (sent, got) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = super::fetch_within(
            &url,
            std::time::Duration::from_millis(500),
            std::time::Duration::from_mins(1),
        )
        .map(drop);
        let _ = sent.send(result);
    });
    let result = got
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the download gives up within the time limit");
    assert!(result.is_err());
    drop(holder);
}

/// 两个进程（常驻的和人敲的命令）同时下同一份不互踩（审计 #127）：临时文件各用各的名字。
#[test]
fn each_download_gets_its_own_side_paths() {
    let cache = temp("side");
    let dir = cache.join("model");
    let first = super::side_paths(&dir).unwrap();
    let second = super::side_paths(&dir).unwrap();
    assert_ne!(first.0, second.0);
    assert_ne!(first.1, second.1);
    std::fs::remove_dir_all(&cache).unwrap();
}

/// 别人先装好了的留着别人的，丢掉自己下的（审计 #127），旁边不留临时文件。
#[test]
fn a_model_someone_else_installed_meanwhile_is_kept() {
    let cache = temp("raced");
    let dir = cache.join("vad");
    install_file(
        &b"first"[..],
        &sha(b"first"),
        &dir,
        "silero_vad.onnx",
        |_| {},
    )
    .unwrap();
    install_file(
        &b"second"[..],
        &sha(b"second"),
        &dir,
        "silero_vad.onnx",
        |_| {},
    )
    .unwrap();
    assert_eq!(
        std::fs::read(dir.join("silero_vad.onnx")).unwrap(),
        b"first"
    );
    let names: Vec<_> = std::fs::read_dir(&cache)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["vad"], "no side files left behind");
    std::fs::remove_dir_all(&cache).unwrap();
}

/// 断在半路（进程被杀、关机）留下的临时文件一个钟头没动过的，下一次下的时候删掉；别人正在写的、别的模型的不碰。
#[test]
fn stale_side_files_are_cleaned_up() {
    let cache = temp("stale");
    std::fs::create_dir_all(&cache).unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_hours(2);
    for name in [
        "model.111-0.download",
        "model.download",
        "model.partial",
        "model.222-0.download",
        "other.333-0.download",
    ] {
        let file = std::fs::File::create(cache.join(name)).unwrap();
        if name != "model.222-0.download" {
            file.set_modified(old).unwrap();
        }
    }
    let _ = super::side_paths(&cache.join("model")).unwrap();
    let mut names: Vec<_> = std::fs::read_dir(&cache)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["model.222-0.download", "other.333-0.download"]);
    std::fs::remove_dir_all(&cache).unwrap();
}
