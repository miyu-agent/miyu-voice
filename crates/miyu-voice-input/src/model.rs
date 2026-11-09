//! 识别模型（设计 20 第五节「模型文件」）：SenseVoice-Small 的 int8 版，第一次启动时从 sherpa-onnx 的 Release 下到这个包
//! 自己的缓存目录 `MIYU_CACHE_DIR`，对照钉死的 SHA-256 校验过才解开；只解出用得着的两份（模型、词表），压缩包删掉。
//! 下载、校验、解开照 [`voice_common::download`]。

use std::io::Read;
use std::path::{Path, PathBuf};

use voice_common::download::{self, InstallError};

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

/// 压缩包里要的两份。
const FILES: [&str; 2] = ["model.int8.onnx", "tokens.txt"];

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
        download::ready(&self.dir, &FILES)
    }

    /// 模型文件。
    #[must_use]
    pub fn model_file(&self) -> PathBuf {
        self.dir.join(FILES[0])
    }

    /// 词表。
    #[must_use]
    pub fn tokens_file(&self) -> PathBuf {
        self.dir.join(FILES[1])
    }

    /// 从下载来的压缩包装进缓存目录。
    ///
    /// # Errors
    ///
    /// 读写出错；校验对不上；压缩包里少了那两份。
    pub fn install(
        &self,
        archive: impl Read,
        progress: impl FnMut(u64),
    ) -> Result<(), InstallError> {
        download::install_archive(archive, SHA256, &self.dir, &FILES, progress)
    }
}
