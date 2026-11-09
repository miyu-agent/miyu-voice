//! 下载识别模型（设计 20 第五节「模型文件」）：照 [`crate::model::URL`] 下，经 [`crate::model::Model::install`] 校验、解开。
//! 走系统的代理设置（`HTTPS_PROXY` 这类）。

use voice_common::download::{self, InstallError};

use crate::model::{self, Model};

/// 下载、校验、装好。`progress` 收到目前下了多少字节。
///
/// # Errors
///
/// 连不上、对方回错、读到一半断了、校验对不上、写不进缓存目录。
pub fn download(model: &Model, progress: impl FnMut(u64)) -> Result<(), String> {
    let reader = download::fetch(model::URL)
        .map_err(|why| format!("could not download the speech model: {why}"))?;
    model
        .install(reader, progress)
        .map_err(|error| match error {
            InstallError::Io(error) => format!("could not save the speech model: {error}"),
            other => format!("the speech model download is not right: {other}"),
        })
}
