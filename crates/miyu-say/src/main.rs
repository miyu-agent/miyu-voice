//! 朗读（设计 20 第六节）：云端合成，在总线上提供 tts、她的 say 工具、命令 miyu say。
//!
//! 核心照清单 `[process] args` 带 `serve` 拉起；人敲 `miyu say …` 时核心把后面的参数原样交过来。这一步（施工 VO-7）：
//! 三家引擎、Markdown 转念得出来的话、本机放；命令行先从环境变量拿 key（开发、查问题用），配置页、工具、经核心找到
//! 在跑的进程去放在 VO-8，总线上的方法在 VO-10。

mod engine;
mod http;
mod mimo;
mod minimax;
mod openai;
mod say;
mod speaker;

use std::process::ExitCode;

use miyu_ext::Extension;

use crate::engine::{Engine, Format};

const USAGE: &str = "usage: miyu say [--engine minimax|mimo|openai] [--voice ID] [--speed 1.0] [-o FILE] <text>
       miyu say --file AUDIO
       miyu say --voices [--engine E]
  keys come from MINIMAX_API_KEY (+ MINIMAX_REGION=international), MIMO_API_KEY, OPENAI_API_KEY (+ OPENAI_BASE_URL, OPENAI_TTS_MODEL) for now";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("serve") => {
            Extension::new("miyu-say", env!("CARGO_PKG_VERSION")).run();
            ExitCode::SUCCESS
        }
        Some(_) => say(&args),
        None => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// `miyu say …`：合成、放，或者存成文件；`--file` 只放一个现成的。
fn say(args: &[String]) -> ExitCode {
    let speaker = speaker::Speaker::default();
    if let Some(path) = option(args, "--file") {
        return match std::fs::read(path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| speaker.play(bytes).map_err(|error| format!("{error:?}")))
        {
            Ok(_) => ExitCode::SUCCESS,
            Err(why) => {
                eprintln!("{why}");
                ExitCode::FAILURE
            }
        };
    }
    if args.iter().any(|arg| arg == "--voices") {
        for voice in from_env(option(args, "--engine").unwrap_or("minimax")).voices() {
            println!("{}\t{}", voice.id, voice.name);
        }
        return ExitCode::SUCCESS;
    }
    let text = words(args);
    if text.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    }
    let engine = from_env(option(args, "--engine").unwrap_or("minimax"));
    let voice = option(args, "--voice").unwrap_or("");
    let speed = option(args, "--speed")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0);
    let out = option(args, "-o");
    let wav = out.is_some_and(|path| {
        std::path::Path::new(path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
    });
    let format = if wav { Format::Wav } else { Format::Mp3 };
    let audio = match say::synthesize(engine.as_ref(), &text, voice, speed, format) {
        Ok(audio) => audio,
        Err(error) => {
            eprintln!("{}: {}", error.code(), error.message());
            return ExitCode::FAILURE;
        }
    };
    let result = match out {
        Some(path) => std::fs::write(path, &audio.bytes).map_err(|error| error.to_string()),
        None => speaker
            .play(audio.bytes)
            .map(drop)
            .map_err(|error| format!("{error:?}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

/// 开发用：照环境变量配一家引擎。
fn from_env(name: &str) -> Box<dyn Engine> {
    let env = |key: &str| std::env::var(key).unwrap_or_default();
    match name {
        "mimo" => Box::new(mimo::MiMo {
            base_url: mimo::BASE_URL.to_string(),
            key: env("MIMO_API_KEY"),
            model: mimo::MODEL.to_string(),
        }),
        "openai" => Box::new(openai::OpenAiCompatible {
            base_url: std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            key: env("OPENAI_API_KEY"),
            model: std::env::var("OPENAI_TTS_MODEL")
                .unwrap_or_else(|_| "gpt-4o-mini-tts".to_string()),
        }),
        _ => Box::new(minimax::MiniMax {
            base_url: if env("MINIMAX_REGION") == "international" {
                minimax::INTERNATIONAL.to_string()
            } else {
                minimax::MAINLAND.to_string()
            },
            key: env("MINIMAX_API_KEY"),
            model: minimax::MODEL.to_string(),
        }),
    }
}

/// `--name value` 的 value。
fn option<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let at = args.iter().position(|arg| arg == name)?;
    args.get(at + 1).map(String::as_str)
}

/// 不是选项、也不是选项的值的那些词，拼成要念的字。
fn words(args: &[String]) -> String {
    let mut out = Vec::new();
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
            continue;
        }
        if arg == "--voices" {
            continue;
        }
        if matches!(
            arg.as_str(),
            "--engine" | "--voice" | "--speed" | "-o" | "--file"
        ) {
            skip = true;
            continue;
        }
        out.push(arg.as_str());
    }
    out.join(" ")
}
