//! 识别模型（设计 20 第五节「模型文件」）：SenseVoice-Small 的 int8 版，第一次启动时从 sherpa-onnx 的 Release 下到这个包
//! 自己的缓存目录 `MIYU_CACHE_DIR`，对照钉死的 SHA-256 校验过才解开；只解出用得着的两份（模型、词表），压缩包删掉。
//! 下好的目录里放一个记号文件，有它才算下好：下到一半断了、解到一半停了的，下次从头来。

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// 用哪一版：换版本改这四个，旧的目录照留着由人删（缓存目录本来就能整个清掉）。用 `FunAudioLLM` 原版的 2024-07-17：
/// 同一个 Release 里 2025-09-09 那份是 ASLP-lab 拿粤语再训的（`WSYue-ASR`），普通话、英语明显变差，也不做逆文本正则
/// （2026-11-09 实测，`MIYU_VOICE_DEBUG=1` 印出的模型信息里写着）。
pub const NAME: &str = "sense-voice-int8-2024-07-17";
/// 下载地址：sherpa-onnx 官方的 Release。
pub const URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17.tar.bz2";
/// 压缩包的 SHA-256（GitHub 给这个文件记的，2026-11-09 照它核对过）。
pub const SHA256: &str = "7d1efa2138a65b0b488df37f8b89e3d91a60676e416f515b952358d83dfd347e";
/// 压缩包多大（字节），印进度用。
pub const SIZE: u64 = 163_002_883;

/// 压缩包里要的两份，照文件名认（前面的目录名不管）。
const MODEL_FILE: &str = "model.int8.onnx";
const TOKENS_FILE: &str = "tokens.txt";
/// 有它才算下好、校验过、解完了。
const DONE: &str = ".done";

/// 缓存目录里的模型。
#[derive(Debug, Clone)]
pub struct Model {
    dir: PathBuf,
}

impl Model {
    /// `cache` 是这个包的缓存目录（`MIYU_CACHE_DIR`）。
    #[must_use]
    pub fn in_cache(cache: &Path) -> Model {
        Model {
            dir: cache.join(NAME),
        }
    }

    /// 下好了没有。
    #[must_use]
    pub fn ready(&self) -> bool {
        self.dir.join(DONE).is_file() && self.model_file().is_file() && self.tokens_file().is_file()
    }

    /// 模型文件。
    #[must_use]
    pub fn model_file(&self) -> PathBuf {
        self.dir.join(MODEL_FILE)
    }

    /// 词表。
    #[must_use]
    pub fn tokens_file(&self) -> PathBuf {
        self.dir.join(TOKENS_FILE)
    }

    /// 从 `archive`（下载来的 `.tar.bz2`）装进缓存目录：先原样写进一个临时文件、一边算 SHA-256，对得上 `sha256` 才解，
    /// 解进旁边的临时目录、写下记号、改名成正式的。`progress` 收到目前写了多少字节。
    ///
    /// # Errors
    ///
    /// 读、写、改名出错；校验对不上（文件被换了、下坏了）；压缩包里少了那两份。
    pub fn install(
        &self,
        archive: impl Read,
        sha256: &str,
        mut progress: impl FnMut(u64),
    ) -> Result<(), InstallError> {
        let parent = self.dir.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let download = parent.join(format!("{NAME}.download"));
        let staging = parent.join(format!("{NAME}.partial"));
        let got = save_hashing(archive, &download, &mut progress)?;
        if got != sha256 {
            let _ = fs::remove_file(&download);
            return Err(InstallError::Mismatch { got });
        }
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(&staging)?;
        let unpacked = unpack(&download, &staging);
        let _ = fs::remove_file(&download);
        unpacked?;
        File::create(staging.join(DONE))?;
        let _ = fs::remove_dir_all(&self.dir);
        fs::rename(&staging, &self.dir)?;
        Ok(())
    }
}

/// 装不上的原因。
#[derive(Debug)]
pub enum InstallError {
    /// 读写文件、网络读到一半出错。
    Io(io::Error),
    /// 下来的文件和钉死的 SHA-256 对不上。
    Mismatch {
        /// 实际算出来的。
        got: String,
    },
    /// 压缩包里少了要的那一份。
    Missing(&'static str),
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
            InstallError::Mismatch { got } => {
                write!(f, "the download's SHA-256 is {got}, expected {SHA256}")
            }
            InstallError::Missing(name) => write!(f, "the archive has no {name}"),
        }
    }
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

/// 从 `.tar.bz2` 里解出那两份，平放进 `into`。
fn unpack(archive: &Path, into: &Path) -> Result<(), InstallError> {
    let decoder = bzip2::read::BzDecoder::new(BufReader::new(File::open(archive)?));
    let mut tar = tar::Archive::new(decoder);
    let mut found = [false, false];
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let slot = match name {
            MODEL_FILE => 0,
            TOKENS_FILE => 1,
            _ => continue,
        };
        // 只取普通文件，名字是我们定的：压缩包里的路径、链接一概不照着写。
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let mut out = File::create(into.join(name))?;
        io::copy(&mut entry, &mut out)?;
        out.sync_all()?;
        found[slot] = true;
    }
    if !found[0] {
        return Err(InstallError::Missing(MODEL_FILE));
    }
    if !found[1] {
        return Err(InstallError::Missing(TOKENS_FILE));
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

#[cfg(test)]
mod tests;
