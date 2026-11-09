//! 三个语音包共用的（核心仓库 `docs/designs/20-语音.md`）：wav 读成单声道的样本（第三节「形状」，施工 VO-5）；
//! 整机的麦克风锁（第九节，施工 VO-6，[`mic_lock`]）；Markdown 转成念得出来的话（第四节，施工 VO-7）后面几步加。

pub mod audio;
pub mod mic_lock;
