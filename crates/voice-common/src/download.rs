//! 下模型（设计 20 第五节「模型文件」）：照钉死的 SHA-256 校验过才用。压缩包先原样写进临时文件、一边算哈希，对得上才
//! 解，只解要的那几份（照文件名认，压缩包里的路径、链接一概不照着写），解进旁边的临时目录、写下记号文件、改名成正式的；
//! 下到一半断了、解到一半停了的，下次从头来。走系统的代理设置（`HTTPS_PROXY` 这类）。

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};

/// 有它才算下好、校验过、解完了。
const DONE: &str = ".done";

/// 装不上的原因。
#[derive(Debug)]
pub enum InstallError {
    /// 读写文件、网络读到一半出错。
    Io(io::Error),
    /// 下来的文件和钉死的 SHA-256 对不上。
    Mismatch {
        /// 实际算出来的。
        got: String,
        /// 钉死的。
        expected: String,
    },
    /// 压缩包里少了要的那一份。
    Missing(String),
}

impl From<io::Error> for InstallError {
    fn from(error: io::Error) -> InstallError {
        InstallError::Io(error)
    }
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::Io(error) => write!(f, "{error}"),
            InstallError::Mismatch { got, expected } => {
                write!(f, "the download's SHA-256 is {got}, expected {expected}")
            }
            InstallError::Missing(name) => write!(f, "the archive has no {name}"),
        }
    }
}

/// 开始下 `url`，交回读得出内容的一头。
///
/// # Errors
///
/// 连不上、超时、对方回错。
pub fn fetch(url: &str) -> Result<impl Read, String> {
    fetch_within(url, HEADERS_WITHIN, WHOLE_WITHIN)
}

/// 连上、收到回应头最多等多久。
const HEADERS_WITHIN: Duration = Duration::from_secs(30);
/// 整个下载最多多久：最大的模型 163 MB，慢的网也够。
const WHOLE_WITHIN: Duration = Duration::from_mins(30);

/// 照给的时限下（审计 #127：原来没有时限，对方连上了不回话的一直挂着）。
fn fetch_within(url: &str, headers: Duration, whole: Duration) -> Result<impl Read, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(headers))
        .timeout_recv_response(Some(headers))
        .timeout_global(Some(whole))
        .build()
        .into();
    let response = agent
        .get(url)
        .call()
        .map_err(|error| format!("could not download {url}: {error}"))?;
    Ok(response.into_body().into_reader())
}

/// 目录 `dir` 里这几份都在、有记号，才算装好了。
#[must_use]
pub fn ready(dir: &Path, files: &[&str]) -> bool {
    dir.join(DONE).is_file() && files.iter().all(|name| dir.join(name).is_file())
}

/// 从 `archive`（`.tar.bz2`）装进目录 `dir`：只解 `wanted` 这几份，平放。`progress` 收到目前写了多少字节。
///
/// # Errors
///
/// 读、写、改名出错；校验对不上；压缩包里少了要的。
pub fn install_archive(
    archive: impl Read,
    sha256: &str,
    dir: &Path,
    wanted: &[&str],
    mut progress: impl FnMut(u64),
) -> Result<(), InstallError> {
    let (download, staging) = side_paths(dir)?;
    check(
        save_hashing(archive, &download, &mut progress)?,
        sha256,
        &download,
    )?;
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    let unpacked = unpack(&download, &staging, wanted);
    let _ = fs::remove_file(&download);
    unpacked?;
    finish(&staging, dir, wanted)
}

/// 单个文件（不是压缩包）装进目录 `dir`，名字是 `name`。
///
/// # Errors
///
/// 读、写、改名出错；校验对不上。
pub fn install_file(
    file: impl Read,
    sha256: &str,
    dir: &Path,
    name: &str,
    mut progress: impl FnMut(u64),
) -> Result<(), InstallError> {
    let (download, staging) = side_paths(dir)?;
    check(
        save_hashing(file, &download, &mut progress)?,
        sha256,
        &download,
    )?;
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    fs::rename(&download, staging.join(name))?;
    finish(&staging, dir, &[name])
}

/// 下载用的临时文件、解开用的临时目录：都在 `dir` 旁边，名字带进程号和序号（审计 #127）：常驻的和人敲的命令同时下同一份
/// 时各写各的，不互踩。上次断在半路、一个钟头没动过的顺手删掉。
fn side_paths(dir: &Path) -> io::Result<(PathBuf, PathBuf)> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = dir.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let name = dir.file_name().map_or_else(
        || "model".into(),
        |name| name.to_string_lossy().into_owned(),
    );
    remove_stale(parent, &name);
    let own = format!(
        "{name}.{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    Ok((
        parent.join(format!("{own}.download")),
        parent.join(format!("{own}.partial")),
    ))
}

/// 临时文件多久没动过算断在半路：下着、解着的一直在写。
const STALE: Duration = Duration::from_hours(1);

/// 删掉 `parent` 里 `name` 的、[`STALE`] 没动过的临时文件和目录（进程被杀、关机留下的）。
fn remove_stale(parent: &Path, name: &str) {
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let ours = file_name
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('.'))
            .is_some_and(|rest| matches!(rest.rsplit('.').next(), Some("download" | "partial")));
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > STALE);
        if ours && stale {
            let path = entry.path();
            let _ = fs::remove_file(&path).or_else(|_| fs::remove_dir_all(&path));
        }
    }
}

fn check(got: String, expected: &str, download: &Path) -> Result<(), InstallError> {
    if got == expected {
        return Ok(());
    }
    let _ = fs::remove_file(download);
    Err(InstallError::Mismatch {
        got,
        expected: expected.to_string(),
    })
}

/// 解好的临时目录改名成正式的。别人（另一个进程）在这期间装好了的，留着别人的、丢掉自己的（审计 #127）。
fn finish(staging: &Path, dir: &Path, files: &[&str]) -> Result<(), InstallError> {
    if ready(dir, files) {
        let _ = fs::remove_dir_all(staging);
        return Ok(());
    }
    File::create(staging.join(DONE))?;
    let _ = fs::remove_dir_all(dir);
    fs::rename(staging, dir)?;
    Ok(())
}

/// 原样写进 `path`，交回 SHA-256 的十六进制。
fn save_hashing(
    mut from: impl Read,
    path: &Path,
    progress: &mut impl FnMut(u64),
) -> io::Result<String> {
    let mut out = BufWriter::new(File::create(path)?);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut written = 0u64;
    loop {
        let n = match from.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        hasher.update(&buffer[..n]);
        out.write_all(&buffer[..n])?;
        written += n as u64;
        progress(written);
    }
    out.flush()?;
    out.into_inner()
        .map_err(io::IntoInnerError::into_error)?
        .sync_all()?;
    Ok(hex(&hasher.finalize()))
}

/// 从 `.tar.bz2` 里解出 `wanted` 这几份（照文件名认、只要普通文件），平放进 `into`。
fn unpack(archive: &Path, into: &Path, wanted: &[&str]) -> Result<(), InstallError> {
    let decoder = bzip2::read::BzDecoder::new(BufReader::new(File::open(archive)?));
    let mut tar = tar::Archive::new(decoder);
    let mut found = vec![false; wanted.len()];
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(slot) = wanted.iter().position(|want| *want == name) else {
            continue;
        };
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let mut out = File::create(into.join(wanted[slot]))?;
        io::copy(&mut entry, &mut out)?;
        out.sync_all()?;
        found[slot] = true;
    }
    match found.iter().position(|got| !got) {
        Some(missing) => Err(InstallError::Missing(wanted[missing].to_string())),
        None => Ok(()),
    }
}

/// 字节的十六进制。
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

#[cfg(test)]
mod tests;
