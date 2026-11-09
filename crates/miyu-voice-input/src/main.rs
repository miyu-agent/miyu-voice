//! 语音输入（设计 20 第五节）：本机识别，在总线上提供 stt。
//!
//! 核心照清单 `[process] args` 带 `serve` 拉起，一行一条的 JSON-RPC 走标准输入输出（开发包 `miyu-ext`）；人敲
//! `miyu voice-input …` 时核心把后面的参数原样交过来（[`cli`]）。起来时没有模型就在后台下；认都经识别的子进程
//! （`recognize-worker`，[`worker`]）；录音照整机的麦克风锁（[`record`]）。总线上的方法 VO-9 登记。

mod cli;
mod endpoint;
mod fetch;
mod filter;
mod model;
mod recognize;
mod record;
mod segment;
mod worker;

use std::path::PathBuf;
use std::process::ExitCode;

use miyu_ext::Extension;

use crate::model::Model;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("serve") => {
            serve();
            ExitCode::SUCCESS
        }
        Some("recognize-worker") => match cache_dir() {
            Some(cache) => {
                worker::serve(Model::in_cache(&cache));
                ExitCode::SUCCESS
            }
            None => ExitCode::FAILURE,
        },
        _ => cli::run(&args),
    }
}

fn serve() {
    let mut ext = Extension::new("miyu-voice-input", env!("CARGO_PKG_VERSION"));
    ext.ready(|_core| {
        let Some(cache) = cache_dir() else {
            eprintln!("miyu-voice-input: MIYU_CACHE_DIR is not set");
            return;
        };
        let model = Model::in_cache(&cache);
        if !model.ready() {
            eprintln!(
                "miyu-voice-input: downloading the speech model ({} MB)",
                model::SIZE / 1_000_000
            );
            if let Err(why) = fetch::download(&model, |_| {}) {
                eprintln!("miyu-voice-input: {why}");
            }
        }
    });
    ext.run();
}

/// 这个包的缓存目录：核心拉起、转交命令时给的 `MIYU_CACHE_DIR`。
fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("MIYU_CACHE_DIR").map(PathBuf::from)
}
