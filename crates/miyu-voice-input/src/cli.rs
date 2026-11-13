//! 人敲的 `miyu voice-input …`（开发、查问题用）：`transcribe <wav>` 认一个文件，`record` 录一句再认，都印一行 JSON；
//! 和总线上的 `stt.transcribe`、`stt.record` 走同一条路（子进程认、整机的麦克风锁）。
//! `MIYU_VOICE_DEBUG=1` 让 sherpa-onnx 印它收到的配置；`MIYU_VOICE_MEASURE=1` 印常驻内存（施工 VO-5 自查）。

use std::path::PathBuf;
use std::process::ExitCode;

use voice_common::audio::{self, Pcm};

use crate::model::{self, Model};
use crate::record::{self, Stopper};
use crate::worker::Workers;

const USAGE: &str = "usage: miyu voice-input transcribe <file.wav> [--language L] | record [--language L] [--device NAME]
  L: zh (default), yue, en, ja, ko, auto";

/// 照第一个参数分。
pub fn run(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("transcribe") => transcribe(&args[1..]),
        Some("record") => record_once(&args[1..]),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn transcribe(args: &[String]) -> ExitCode {
    let Some(path) = args.first() else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    let language = option(args, "--language").unwrap_or("zh");
    let Some(cache) = ensure_model() else {
        return ExitCode::FAILURE;
    };
    let pcm = match std::fs::read(path)
        .map_err(|error| format!("cannot read {path}: {error}"))
        .and_then(|bytes| audio::read_wav(&bytes).map_err(|why| why.0))
    {
        Ok(pcm) => pcm,
        Err(why) => {
            eprintln!("{why}");
            return ExitCode::FAILURE;
        }
    };
    recognize_and_print(cache, &pcm, language, None)
}

/// 录一句（静下来 1.2 秒就停，最长一分钟），认。
fn record_once(args: &[String]) -> ExitCode {
    let language = option(args, "--language").unwrap_or("zh");
    let Some(cache) = ensure_model() else {
        return ExitCode::FAILURE;
    };
    eprintln!("listening… (stops when you go quiet)");
    let speaker = voice_common::speaker::Speaker::default();
    match record::record(
        option(args, "--device"),
        1200,
        60_000,
        &Stopper::default(),
        Some(&speaker),
    ) {
        Ok((pcm, ended)) => recognize_and_print(cache, &pcm, language, Some(ended.as_str())),
        Err(why) => {
            eprintln!("{why:?}");
            ExitCode::FAILURE
        }
    }
}

/// `--name value` 的 value。
fn option<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let at = args.iter().position(|arg| arg == name)?;
    args.get(at + 1).map(String::as_str)
}

/// 模型没有就下（进度印在标准错误上），交回缓存目录。
fn ensure_model() -> Option<PathBuf> {
    let Some(cache) = crate::cache_dir() else {
        eprintln!("miyu voice-input: MIYU_CACHE_DIR is not set");
        return None;
    };
    let model = Model::in_cache(&cache);
    if !model.ready() {
        eprintln!("downloading the speech model into {}", cache.display());
        let mut shown = 0;
        let result = crate::fetch::download(&model, |n| {
            let percent = n * 100 / model::SIZE;
            if percent >= shown + 10 {
                shown = percent;
                eprintln!("  {percent}%");
            }
        });
        if let Err(why) = result {
            eprintln!("{why}");
            return None;
        }
    }
    Some(cache)
}

/// 交给识别的子进程认，印 JSON；`ended` 是录音为什么停的（`record` 才有）。
fn recognize_and_print(cache: PathBuf, pcm: &Pcm, language: &str, ended: Option<&str>) -> ExitCode {
    let workers = Workers::new(cache);
    match workers.transcribe(pcm, language) {
        Ok(heard) => {
            let mut answer = serde_json::json!({
                "text": heard.text,
                "language": heard.language,
                "duration_ms": heard.duration_ms,
                "clear": heard.clear,
            });
            if let Some(ended) = ended {
                answer["ended"] = ended.into();
            }
            println!("{answer}");
            if std::env::var_os("MIYU_VOICE_MEASURE").is_some() {
                eprintln!("resident after recognizing: {} KiB", resident_kib());
                drop(workers);
                eprintln!("resident after the worker is gone: {} KiB", resident_kib());
            }
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("{why:?}");
            ExitCode::FAILURE
        }
    }
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
