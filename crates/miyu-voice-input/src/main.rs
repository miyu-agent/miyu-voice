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
mod service;
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
    let Some(cache) = cache_dir() else {
        eprintln!("miyu-voice-input: MIYU_CACHE_DIR is not set");
        std::process::exit(miyu_ext::EXIT_CONFIG);
    };
    let service = service::Service::new(cache.clone());
    let mut ext = Extension::new("miyu-voice-input", env!("CARGO_PKG_VERSION"));
    service.register(&mut ext);
    let on_ready = std::sync::Arc::clone(&service);
    ext.ready(move |core| {
        on_ready.reload(core);
        // 模型下好、校验过才登记 stt.*（设计 20 第五节）：没下好的先下，头这时还画不出麦克风。
        let model = Model::in_cache(&cache);
        if !model.ready() {
            eprintln!(
                "miyu-voice-input: downloading the speech model ({} MB)",
                model::SIZE / 1_000_000
            );
            if let Err(why) = fetch::download(&model, |_| {}) {
                eprintln!("miyu-voice-input: {why}");
                return;
            }
        }
        if let Err(error) = core.enable_methods(&service::METHODS) {
            eprintln!("miyu-voice-input: package.methods failed: {error}");
        }
    });
    // 配置推送的处理跑在读核心回应的线程上（开发包 0.1，SDK-2 前）：里面不调核心，只重读。
    let on_config = std::sync::Arc::clone(&service);
    ext.on_config(move |_, core| on_config.reload(core));
    ext.run();
}

/// 这个包的缓存目录：核心拉起、转交命令时给的 `MIYU_CACHE_DIR`。
fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("MIYU_CACHE_DIR").map(PathBuf::from)
}
