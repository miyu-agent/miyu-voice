//! 三个语音包共用的（核心仓库 `docs/designs/20-语音.md`）：wav 读成单声道的样本（第三节「形状」，施工 VO-5）；
//! 整机的麦克风锁（第九节，施工 VO-6，[`mic_lock`]）；Markdown 转成念得出来的话、按句切（第四节，施工 VO-7，
//! [`speakable`]）；经核心存、读 blob（[`blobs`]）；下模型、照钉死的 SHA-256 校验（[`download`]）。

pub mod audio;
pub mod blobs;
pub mod download;
pub mod mic_lock;
pub mod speakable;
