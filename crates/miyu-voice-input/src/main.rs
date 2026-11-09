//! 语音输入（设计 20 第五节）：本机识别，在总线上提供 stt。
//!
//! 核心照清单 `[process] args` 带 `serve` 拉起，一行一条的 JSON-RPC 走标准输入输出（开发包 `miyu-ext`）；人敲
//! `miyu voice-input …` 时核心把后面的参数原样交过来。这一步（施工 VO-5）：起来时没有模型就在后台下；命令行
//! `transcribe <wav>` 认一个文件（开发、查问题用），和以后总线上的 `stt.transcribe` 一样经子进程认
//! （`recognize-worker`，[`worker`]）。总线上的方法 VO-9 登记。

mod fetch;
mod filter;
mod model;
mod recognize;
mod segment;
mod worker;

use miyu_ext::Extension;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::model::Model;
use crate::worker::Workers;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("serve") => {
            serve();
            ExitCode::SUCCESS
        }
        Some("transcribe") => transcribe(&args[1..]),
        Some("recognize-worker") => match cache_dir() {
            Some(cache) => {
                worker::serve(Model::in_cache(&cache));
                ExitCode::SUCCESS
            }
            None => ExitCode::FAILURE,
        },
        _ => {
            eprintln!(
                "usage: miyu voice-input transcribe <file.wav> [--language zh|yue|en|ja|ko|auto]"
            );
            ExitCode::FAILURE
        }
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

/// `transcribe <wav> [--language L]`：认一个文件，印 JSON；模型没有就先下（进度印在标准错误上）。
fn transcribe(args: &[String]) -> ExitCode {
    let Some(path) = args.first() else {
        eprintln!("usage: miyu voice-input transcribe <file.wav> [--language zh]");
        return ExitCode::FAILURE;
    };
    let language = match args.iter().position(|arg| arg == "--language") {
        Some(at) => args.get(at + 1).map_or("zh", String::as_str),
        None => "zh",
    };
    let Some(cache) = cache_dir() else {
        eprintln!("miyu voice-input: MIYU_CACHE_DIR is not set");
        return ExitCode::FAILURE;
    };
    let model = Model::in_cache(&cache);
    if !model.ready() {
        eprintln!("downloading the speech model into {}", cache.display());
        let mut shown = 0;
        let result = fetch::download(&model, |n| {
            let percent = n * 100 / model::SIZE;
            if percent >= shown + 10 {
                shown = percent;
                eprintln!("  {percent}%");
            }
        });
        if let Err(why) = result {
            eprintln!("{why}");
            return ExitCode::FAILURE;
        }
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("cannot read {path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let pcm = match voice_common::audio::read_wav(&bytes) {
        Ok(pcm) => pcm,
        Err(why) => {
            eprintln!("{why}");
            return ExitCode::FAILURE;
        }
    };
    let workers = Workers::new(cache);
    match workers.transcribe(&pcm, language) {
        Ok(heard) => {
            let answer = serde_json::json!({
                "text": heard.text,
                "language": heard.language,
                "duration_ms": heard.duration_ms,
                "clear": heard.clear,
            });
            println!("{answer}");
            // 量内存（施工 VO-5 自查）：主进程认完、子进程收掉以后各印一次常驻内存。
            if std::env::var_os("MIYU_VOICE_MEASURE").is_some() {
                eprintln!("resident after recognizing: {} KiB", resident_kib());
                drop(workers);
                eprintln!("resident after unloading: {} KiB", resident_kib());
            }
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("{why:?}");
            ExitCode::FAILURE
        }
    }
}

/// 这个包的缓存目录：核心拉起、转交命令时给的 `MIYU_CACHE_DIR`。
fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("MIYU_CACHE_DIR").map(PathBuf::from)
}

/// 本进程现在的常驻内存（KiB），只有 Linux 读得到，别处是 0。
fn resident_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("VmRSS:"))
                .and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse().ok())
        })
        .unwrap_or(0)
}
