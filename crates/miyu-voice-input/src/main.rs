//! 语音输入（设计 20 第五节）：本机识别，在总线上提供 stt。
//!
//! 核心照清单 `[process] args` 带 `serve` 拉起，一行一条的 JSON-RPC 走标准输入输出（开发包 `miyu-ext`）；人敲
//! `miyu voice-input …` 时核心把后面的参数原样交过来。骨架这一步（施工 VO-4）只握手、什么也不登记。

use std::process::ExitCode;

use miyu_ext::Extension;

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("serve") {
        Extension::new("miyu-voice-input", env!("CARGO_PKG_VERSION")).run();
        ExitCode::SUCCESS
    } else {
        eprintln!("miyu voice-input: not ready yet");
        ExitCode::FAILURE
    }
}
