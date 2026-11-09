//! 三个语音包共用的（核心仓库 `docs/designs/20-语音.md`）：wav 读成单声道的样本（第三节「形状」，施工 VO-5）；
//! 整机的麦克风锁（第九节，施工 VO-6，[`mic_lock`]）；Markdown 转成念得出来的话、按句切（第四节，施工 VO-7，
//! [`speakable`]）；经核心存、读 blob（[`blobs`]）；人敲的命令连核心（[`client`]）；下模型、照钉死的 SHA-256 校验（[`download`]）；打开麦克风、从音箱放（`mic`、`speaker`，各要打开同名的 feature）。

pub mod audio;
pub mod blobs;
pub mod client;
pub mod download;
#[cfg(feature = "mic")]
pub mod mic;
pub mod mic_lock;
pub mod speakable;
#[cfg(feature = "speaker")]
pub mod speaker;
