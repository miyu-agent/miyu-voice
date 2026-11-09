//! 语音助手（设计 20 第九节）：常驻后台，叫名字就能说，命令 miyu listen。
//!
//! 核心照清单 `[process] args` 带 `serve` 拉起，一行一条的 JSON-RPC 走标准输入输出（开发包 `miyu-ext`）；人敲
//! `miyu listen …` 时核心把后面的参数原样交过来。骨架这一步（施工 VO-4）只握手、什么也不登记。

use std::process::ExitCode;

use miyu_ext::Extension;

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("serve") {
        Extension::new("miyu-voice-assistant", env!("CARGO_PKG_VERSION")).run();
        ExitCode::SUCCESS
    } else {
        eprintln!("miyu listen: not ready yet");
        ExitCode::FAILURE
    }
}
