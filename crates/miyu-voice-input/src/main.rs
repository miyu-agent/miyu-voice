//! 语音输入（设计 20 第五节）：本机识别，在总线上提供 stt。
//!
//! 核心照清单 `[process] args` 带 `serve` 拉起，一行一条的 JSON-RPC 走标准输入输出（开发包 `miyu-ext`）；人敲
//! `miyu voice-input …` 时核心把后面的参数原样交过来（[`cli`]）。起来时没有模型就在后台下（选了社区引擎的不下）；认都经识别的子进程
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

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError};

use miyu_ext::{Core, Extension};

use crate::model::Model;
use crate::service::Service;

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
    let downloading = Arc::new(Mutex::new(()));
    let (on_ready, cache_ready, lock_ready) = (
        Arc::clone(&service),
        cache.clone(),
        Arc::clone(&downloading),
    );
    ext.ready(move |core| {
        on_ready.reload(core);
        make_ready(&on_ready, core, &cache_ready, &lock_ready);
    });
    // 配置推送的处理跑在读核心回应的线程上（开发包 0.1，SDK-2 前）：这里只重读，登记、下模型另开线程。
    let on_config = Arc::clone(&service);
    ext.on_config(move |_, core| {
        on_config.reload(core);
        let (service, core, cache, downloading) = (
            Arc::clone(&on_config),
            core.clone(),
            cache.clone(),
            Arc::clone(&downloading),
        );
        std::thread::spawn(move || make_ready(&service, &core, &cache, &downloading));
    });
    ext.run();
}

/// 照选的引擎登记 `stt.*`（施工 VO-29）：选了社区引擎的马上登记，不下自带的模型；用自带的，模型下好、校验过才登记（设计
/// 20 第五节，头这时还画不出麦克风），没下好的先撤掉登记（从社区引擎换回来时）、下好再登记。两次推送赶在一起的只下一次。
fn make_ready(service: &Service, core: &Core, cache: &Path, downloading: &Mutex<()>) {
    if service.own_model() {
        let model = Model::in_cache(cache);
        if !model.ready() {
            let _ = core.disable_methods(&service::METHODS);
            let _one = downloading.lock().unwrap_or_else(PoisonError::into_inner);
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
        }
    }
    if let Err(error) = core.enable_methods(&service::METHODS) {
        eprintln!("miyu-voice-input: package.methods failed: {error}");
    }
}

/// 这个包的缓存目录：核心拉起、转交命令时给的 `MIYU_CACHE_DIR`。
fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("MIYU_CACHE_DIR").map(PathBuf::from)
}
