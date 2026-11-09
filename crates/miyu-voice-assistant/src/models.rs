//! 助手的模型（设计 20 第九节「规程」第 1 步）：人声检测 Silero VAD、唤醒词 sherpa-onnx 的 KWS（wenetspeech 3.3M，旧版
//! 0.7.4 用的就是它，「密友密友」实测 5/5）。第一次启动时下到这个包的缓存目录，照钉死的 SHA-256 校验过才用
//! （[`voice_common::download`]）。识别不在这里：唤醒以后的那句话交总线上的 `stt`。

use std::path::{Path, PathBuf};

use voice_common::download;

/// 唤醒词模型的目录名。
const KWS_NAME: &str = "kws-zipformer-wenetspeech-3.3M-2024-01-01";
/// 唤醒词模型的下载地址（sherpa-onnx 官方的 Release）。
const KWS_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/kws-models/sherpa-onnx-kws-zipformer-wenetspeech-3.3M-2024-01-01.tar.bz2";
/// 压缩包的 SHA-256：GitHub 没给这个老文件记，2026-11-10 下下来自己算的（32,654,866 字节）。
const KWS_SHA256: &str = "b2f7c89690dc8ce4c6ed6afeab7cd800c36ad1421fb6b6302b4a4b194cf7f35f";
/// 压缩包里要的几份：int8 的那一套（同旧版）、建模单元表。
const KWS_FILES: [&str; 4] = [
    "encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx",
    "decoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx",
    "joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx",
    "tokens.txt",
];

/// 人声检测模型的目录名、文件名。
const VAD_NAME: &str = "silero-vad";
const VAD_FILE: &str = "silero_vad.onnx";
/// 人声检测模型的下载地址。
const VAD_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx";
/// SHA-256（GitHub 给这个文件记的，2026-11-10 核对过）。
const VAD_SHA256: &str = "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6";

/// 缓存目录里的两样模型。
#[derive(Debug, Clone)]
pub struct Models {
    cache: PathBuf,
}

impl Models {
    /// `cache` 是这个包的缓存目录（`MIYU_CACHE_DIR`）。
    #[must_use]
    pub fn in_cache(cache: &Path) -> Models {
        Models {
            cache: cache.to_path_buf(),
        }
    }

    /// 都下好了没有。
    #[expect(dead_code, reason = "唤醒的管线（VO-11 下一段）起来前照它判")]
    #[must_use]
    pub fn ready(&self) -> bool {
        download::ready(&self.kws_dir(), &KWS_FILES)
            && download::ready(&self.cache.join(VAD_NAME), &[VAD_FILE])
    }

    /// 没下好的下好。
    ///
    /// # Errors
    ///
    /// 连不上、校验对不上、写不进缓存目录。
    pub fn ensure(&self) -> Result<(), String> {
        if !download::ready(&self.cache.join(VAD_NAME), &[VAD_FILE]) {
            let file = download::fetch(VAD_URL)?;
            download::install_file(
                file,
                VAD_SHA256,
                &self.cache.join(VAD_NAME),
                VAD_FILE,
                |_| {},
            )
            .map_err(|error| format!("the voice activity model: {error}"))?;
        }
        if !download::ready(&self.kws_dir(), &KWS_FILES) {
            let archive = download::fetch(KWS_URL)?;
            download::install_archive(archive, KWS_SHA256, &self.kws_dir(), &KWS_FILES, |_| {})
                .map_err(|error| format!("the wake word model: {error}"))?;
        }
        Ok(())
    }

    /// 人声检测模型。
    #[must_use]
    pub fn vad(&self) -> PathBuf {
        self.cache.join(VAD_NAME).join(VAD_FILE)
    }

    /// 唤醒词模型的编码器、解码器、连接器、建模单元表。
    #[must_use]
    pub fn kws(&self) -> [PathBuf; 4] {
        KWS_FILES.map(|name| self.kws_dir().join(name))
    }

    fn kws_dir(&self) -> PathBuf {
        self.cache.join(KWS_NAME)
    }
}
