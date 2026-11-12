//! 语音助手（设计 20 第九节）：常驻后台，叫名字就能说，命令 miyu listen。
//!
//! 核心照清单 `[process] args` 带 `serve` 拉起（[`service`]）；人敲 `miyu listen …` 时核心把后面的参数原样交过来：命令当
//! 本机的头连核心，调在跑的助手的 `listen`，开、关和她的对话（唤醒词装着就一直在听，不归它管）。唤醒词、人声检测的模型（[`models`]），唤醒词编成唤醒模型认的写法
//! （[`keywords`]），听（[`listen`]），收麦（[`capture`]），提示音（[`chime`]）；会话那一段等核心 VA-1。

mod capture;
mod chime;
mod conversation;
mod echo;
mod keywords;
mod listen;
mod models;
mod notify;
mod reply;
mod service;

use std::path::PathBuf;
use std::process::ExitCode;

use miyu_ext::Extension;

const USAGE: &str = "usage: miyu listen [on|off]                               start or end a conversation (no argument: toggle)
       miyu listen wake-test <wake word>              how the wake word model will hear it
       miyu listen wake-test --wav FILE <wake word>   run a 16 kHz mono wav through the listener";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("serve") => {
            serve();
            ExitCode::SUCCESS
        }
        Some("wake-test") => wake_test(&args[1..]),
        None | Some("on" | "off") => match switch(args.first().map_or("toggle", String::as_str)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(why) => {
                eprintln!("miyu listen: {why}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn serve() {
    let dir = |key: &str| std::env::var_os(key).map(PathBuf::from);
    let Some(cache) = dir("MIYU_CACHE_DIR") else {
        eprintln!("miyu-voice-assistant: MIYU_CACHE_DIR is not set");
        std::process::exit(miyu_ext::EXIT_CONFIG);
    };
    let service = service::Service::new(cache);
    let mut ext = Extension::new("miyu-voice-assistant", env!("CARGO_PKG_VERSION"));
    service.register(&mut ext);
    let on_ready = std::sync::Arc::clone(&service);
    ext.ready(move |core| {
        on_ready.reload(core);
        on_ready.start(core);
    });
    let on_config = std::sync::Arc::clone(&service);
    ext.on_config(move |_, core| on_config.reload(core));
    ext.run();
}

/// `miyu listen [on|off]`：连核心，开、关和在跑的助手的对话，印现在开没开。
fn switch(to: &str) -> Result<(), String> {
    let home = std::env::var_os("MIYU_HOME")
        .map(PathBuf::from)
        .ok_or("MIYU_HOME is not set; run it as `miyu listen`.")?;
    let mut core = voice_common::client::Client::connect(&home, "miyu-listen")?;
    let answer = core.call(
        "package.call",
        &serde_json::json!({"package": "voice-assistant", "method": "listen", "params": {"to": to}}),
    )?;
    println!(
        "{}",
        if answer["listening"] == true {
            "listening"
        } else {
            "not listening"
        }
    );
    Ok(())
}

/// `wake-test [--wav 文件] <词>`：模型没有就下，照模型的词表编，一行一种念法印出来；给了 wav 的，再整个过一遍听的那一套，
/// 印听到的（开发、调灵敏度用）。
fn wake_test(args: &[String]) -> ExitCode {
    let wav = args
        .iter()
        .position(|arg| arg == "--wav")
        .and_then(|at| args.get(at + 1));
    let mut words = Vec::new();
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
        } else if arg == "--wav" {
            skip = true;
        } else {
            words.push(arg.as_str());
        }
    }
    let word = words.join(" ");
    let Some(cache) = std::env::var_os("MIYU_CACHE_DIR").map(PathBuf::from) else {
        eprintln!("miyu listen: MIYU_CACHE_DIR is not set");
        return ExitCode::FAILURE;
    };
    let models = models::Models::in_cache(&cache);
    if let Err(why) = models.ensure() {
        eprintln!("miyu listen: {why}");
        return ExitCode::FAILURE;
    }
    let vocabulary = match std::fs::read_to_string(&models.kws()[3]) {
        Ok(text) => keywords::tokens(&text),
        Err(error) => {
            eprintln!("miyu listen: {error}");
            return ExitCode::FAILURE;
        }
    };
    let lines = match keywords::encode(&word, &vocabulary) {
        Ok(lines) => lines,
        Err(why) => {
            eprintln!("{}", why.0);
            return ExitCode::FAILURE;
        }
    };
    for line in &lines {
        println!("{line}");
    }
    let Some(wav) = wav else {
        return ExitCode::SUCCESS;
    };
    match run_listener(&models, &lines, wav) {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("miyu listen: {why}");
            ExitCode::FAILURE
        }
    }
}

/// 一个 wav 一帧帧喂给听的那一套，印听到的。
fn run_listener(models: &models::Models, lines: &[String], wav: &str) -> Result<(), String> {
    let bytes = std::fs::read(wav).map_err(|error| format!("{wav}: {error}"))?;
    let pcm = voice_common::audio::read_wav(&bytes).map_err(|why| why.0)?;
    if pcm.rate as usize != listen::RATE {
        return Err("give a 16 kHz wav".to_string());
    }
    let mut listener = listen::Listener::new(models, lines, listen::Sensitivity::default())?;
    let mut padded = pcm.samples;
    padded.extend(std::iter::repeat_n(0.0, listen::RATE * 2));
    for frame in padded.chunks(512) {
        for event in listener.feed(frame) {
            match event {
                listen::Event::Wake { keyword, rest } => {
                    println!(
                        "wake: {keyword} (then {} ms)",
                        rest.len() * 1000 / listen::RATE
                    );
                }
                listen::Event::Speech(audio) => {
                    println!("speech: {} ms", audio.len() * 1000 / listen::RATE);
                }
                listen::Event::Onset => println!("onset"),
            }
        }
    }
    Ok(())
}
